use std::cmp;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures::{future::FutureExt, select_biased};
use teloxide::requests::Requester;
use teloxide::types::ChatId;
use teloxide::{ApiError, Bot, RequestError};
use tokio::{
    self,
    sync::{Mutex, Notify},
    time::{self, Duration, Instant},
};
use tokio_stream::StreamExt;
use tokio_util::time::DelayQueue;

use crate::client::pull_feed;
use crate::data::{Database, Feed, FeedUpdate};
use crate::messages::{format_large_msg, Escape};

pub fn start(bot: Bot, db: Arc<Mutex<Database>>, min_interval: u32, max_interval: u32) {
    let mut queue = FetchQueue::new();
    // TODO: Don't use interval, it can accumulate ticks
    // replace it with delay_until
    let mut interval = time::interval_at(Instant::now(), Duration::from_secs(min_interval as u64));
    let throttle = Throttle::new(min_interval as usize);
    tokio::spawn(async move {
        loop {
            select_biased! {
                feed = queue.next().fuse() => {
                    let feed = feed.expect("unreachable");
                    let bot = bot.clone();
                    let db = db.clone();
                    let opportunity = throttle.acquire();
                    tokio::spawn(async move {
                        opportunity.wait().await;
                        if let Err(e) = fetch_and_push_updates(bot, db, feed).await {
                            // crate::print_error(e);
                            eprintln!("Error: {}", e);
                            e.chain().skip(1).for_each(|cause| eprintln!("caused by: {}", cause));
                        }
                    });
                }
                _ = interval.tick().fuse() => {
                    let feeds = db.lock().await.all_feeds();
                    for feed in feeds {
                        let feed_interval = cmp::min(
                            cmp::max(
                                feed.ttl.map(|ttl| ttl * 60).unwrap_or_default(),
                                min_interval,
                            ),
                            max_interval,
                        ) as u64 - 1; // after -1, we can stagger with `interval`
                        queue.enqueue(feed, Duration::from_secs(feed_interval));
                    }
                }
            }
        }
    });
}

async fn fetch_and_push_updates(
    bot: Bot,
    db: Arc<Mutex<Database>>,
    feed: Feed,
) -> Result<(), anyhow::Error> {
    let new_feed = match pull_feed(&feed.link).await {
        Ok(feed) => feed,
        Err(e) => {
            let down_time = db.lock().await.get_or_update_down_time(&feed.link);
            if down_time.is_none() {
                // user unsubscribed while fetching the feed
                return Ok(());
            }
            // 5 days
            if down_time.unwrap().as_secs() > 5 * 24 * 60 * 60 {
                db.lock().await.reset_down_time(&feed.link);
                let msg = tr!(
                    "continuous_fetch_error",
                    link = Escape(&feed.link),
                    title = Escape(&feed.title),
                    error = Escape(&e.to_user_friendly())
                );
                push_updates(
                    &bot,
                    &db,
                    feed.subscribers,
                    &msg,
                    Some(teloxide::types::ParseMode::Html),
                )
                .await?;
            }
            return Ok(());
        }
    };

    let updates = db.lock().await.update(&feed.link, new_feed);
    for update in updates {
        match update {
            FeedUpdate::Items(items) => {
                let msgs =
                    format_large_msg(format!("<b>{}</b>", Escape(&feed.title)), &items, |item| {
                        let title = item.title.as_deref().unwrap_or(&feed.title);
                        let link = item.link.as_deref().unwrap_or(&feed.link);
                        format!("<a href=\"{}\">{}</a>", Escape(link), Escape(title))
                    });
                for msg in msgs {
                    push_updates(
                        &bot,
                        &db,
                        feed.subscribers.iter().copied(),
                        &msg,
                        Some(teloxide::types::ParseMode::Html),
                    )
                    .await?;
                }
            }
            FeedUpdate::Title(new_title) => {
                let msg = tr!(
                    "feed_renamed",
                    link = Escape(&feed.link),
                    title = Escape(&feed.title),
                    new_title = Escape(&new_title)
                );
                push_updates(
                    &bot,
                    &db,
                    feed.subscribers.iter().copied(),
                    &msg,
                    Some(teloxide::types::ParseMode::Html),
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn push_updates<I: IntoIterator<Item = i64>>(
    bot: &Bot,
    db: &Arc<Mutex<Database>>,
    subscribers: I,
    msg: &str,
    mode: Option<teloxide::types::ParseMode>,
) -> Result<(), anyhow::Error> {
    for mut subscriber in subscribers {
        'retry: for _ in 0..3 {
            use ApiError::*;
            match {
                let mut send = bot.send_message(ChatId(subscriber), msg);
                send.parse_mode = mode;
                send.await
            } {
                // Err(RequestError::Api(e)) if chat_is_unavailable(&e.to_string()) => {
                //     db.lock().await.delete_subscriber(subscriber);
                // }
                Err(RequestError::Api(
                    BotBlocked
                    | BotKickedFromSupergroup
                    | BotKicked
                    | UserDeactivated
                    | ChatNotFound
                    | NotEnoughRightsToPostMessages,
                )) => {
                    db.lock().await.delete_subscriber(subscriber);
                }
                Err(RequestError::MigrateToChatId(new_chat_id)) => {
                    db.lock().await.update_subscriber(subscriber, new_chat_id);
                    subscriber = new_chat_id;
                    continue 'retry;
                }
                Err(RequestError::RetryAfter(delay)) => {
                    time::sleep(delay).await;
                    continue 'retry;
                }
                Err(RequestError::Api(e)) if chat_is_unavailable(&e.to_string()) => {
                    db.lock().await.delete_subscriber(subscriber);
                }
                other => {
                    other?;
                }
            }
            break 'retry;
        }
    }
    Ok(())
}

pub fn chat_is_unavailable(s: &str) -> bool {
    s.contains("Forbidden")
        || s.contains("chat not found")
        || s.contains("have no rights")
        || s.contains("need administrator rights")
}

#[derive(Default)]
struct FetchQueue {
    feeds: HashMap<String, Feed>,
    notifies: DelayQueue<String>,
    wakeup: Notify,
}

impl FetchQueue {
    fn new() -> Self {
        Self::default()
    }

    fn enqueue(&mut self, feed: Feed, delay: Duration) -> bool {
        let exists = self.feeds.contains_key(&feed.link);
        if !exists {
            self.notifies.insert(feed.link.clone(), delay);
            self.feeds.insert(feed.link.clone(), feed);
            self.wakeup.notify_waiters();
        }
        !exists
    }

    async fn next(&mut self) -> Result<Feed, time::error::Error> {
        loop {
            if let Some(feed_id) = self.notifies.next().await {
                let feed = self.feeds.remove(feed_id.get_ref()).unwrap();
                break Ok(feed);
            } else {
                self.wakeup.notified().await;
            }
        }
    }
}

struct Throttle {
    pieces: usize,
    counter: Arc<AtomicUsize>,
}

impl Throttle {
    fn new(pieces: usize) -> Self {
        Throttle {
            pieces,
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn acquire(&self) -> Opportunity {
        Opportunity {
            n: self.counter.fetch_add(1, Ordering::AcqRel) % self.pieces,
            counter: self.counter.clone(),
        }
    }
}

#[must_use = "Don't lose your opportunity"]
struct Opportunity {
    n: usize,
    counter: Arc<AtomicUsize>,
}

impl Opportunity {
    async fn wait(&self) {
        time::sleep(Duration::from_secs(self.n as u64)).await
    }
}

impl Drop for Opportunity {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}
