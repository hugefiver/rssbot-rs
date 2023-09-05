use std::sync::Arc;

use anyhow::Context;
use teloxide::{types::Message, utils::command::parse_command, Bot};
use tokio::sync::Mutex;

use crate::data::Database;
use crate::messages::Escape;
use crate::{client::pull_feed, BOT_NAME};

use super::{check_channel_permission, update_response, MsgTarget};

pub async fn sub(
    bot: Bot,
    msg: Message,
    db: Arc<Mutex<Database>>,
) -> Result<(), anyhow::Error> {
    let chat_id = msg.chat.id;
    // let text = msg.text().unwrap_or("");
    // let args = text.split_whitespace().collect::<Vec<_>>();
    let (_, args) = parse_command(
        msg.text().context("content of command text is empty")?,
        BOT_NAME.get().unwrap(),
    )
    .context("failed to parse command")?;
    let mut target_id = chat_id;
    let target = &mut MsgTarget::new(chat_id, msg.id);
    let feed_url;

    match &*args {
        [url] => feed_url = url,
        [channel, url] => {
            let channel_id = check_channel_permission(&bot, &msg, channel, target).await?;
            if channel_id.is_none() {
                return Ok(());
            }
            target_id = channel_id.unwrap();
            feed_url = url;
        }
        [..] => {
            let msg = tr!("sub_how_to_use");
            update_response(&bot, target, msg, None).await?;
            return Ok(());
        }
    };
    if db.lock().await.is_subscribed(target_id.0, feed_url) {
        update_response(&bot, target, tr!("subscribed_to_rss"), None).await?;
        return Ok(());
    }

    if cfg!(feature = "hosted-by-iovxw") && db.lock().await.all_feeds().len() >= 1500 {
        let msg = tr!("subscription_rate_limit");
        update_response(
            &bot,
            target,
            msg,
            Some(teloxide::types::ParseMode::MarkdownV2),
        )
        .await?;
        return Ok(());
    }
    update_response(&bot, target, tr!("processing_please_wait"), None).await?;
    let msg = match pull_feed(feed_url).await {
        Ok(feed) => {
            if db.lock().await.subscribe(target_id.0, feed_url, &feed) {
                tr!(
                    "subscription_succeeded",
                    link = Escape(&feed.link),
                    title = Escape(&feed.title)
                )
            } else {
                tr!("subscribed_to_rss").into()
            }
        }
        Err(e) => tr!("subscription_failed", error = Escape(&e.to_user_friendly())),
    };
    update_response(&bot, target, &msg, Some(teloxide::types::ParseMode::Html)).await?;
    Ok(())
}
