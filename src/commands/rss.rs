use std::sync::Arc;

use anyhow::Context;
use either::Either;
use pinyin::{Pinyin, ToPinyin};
use teloxide::requests::Requester;
use teloxide::types::Message;
use teloxide::utils::command::parse_command;
use teloxide::Bot;
use tokio::sync::Mutex;

use crate::data::Database;
use crate::messages::{format_large_msg, Escape};

use super::{check_channel_permission, update_response, MsgTarget};

pub async fn rss(bot: Bot, msg: Message, db: Arc<Mutex<Database>>) -> Result<(), anyhow::Error> {
    let chat_id = msg.chat.id;
    let (_, args) = parse_command(
        msg.text().context("content of command text is empty")?,
        crate::BOT_NAME.get().unwrap(),
    )
    .context("failed to parse command")?;
    let channel = args.get(0);
    let mut target_id = chat_id;
    let target = &mut MsgTarget::new(chat_id, msg.id);

    if let Some(channel) = channel {
        let channel_id = check_channel_permission(&bot, &msg, &channel, target).await?;
        if channel_id.is_none() {
            return Ok(());
        }
        target_id = channel_id.unwrap();
    }

    let feeds = db.lock().await.subscribed_feeds(target_id.0);
    let mut msgs = if let Some(mut feeds) = feeds {
        feeds.sort_by_cached_key(|feed| {
            feed.title
                .chars()
                .map(|c| {
                    c.to_pinyin()
                        .map(Pinyin::plain)
                        .map(Either::Right)
                        .unwrap_or_else(|| Either::Left(c))
                })
                .collect::<Vec<Either<char, &str>>>()
        });
        format_large_msg(tr!("subscription_list").to_string(), &feeds, |feed| {
            format!(
                "<a href=\"{}\">{}</a>",
                Escape(&feed.link),
                Escape(&feed.title)
            )
        })
    } else {
        vec![tr!("subscription_list_empty").to_string()]
    };

    let first_msg = msgs.remove(0);
    update_response(
        &bot,
        target,
        &first_msg,
        Some(teloxide::types::ParseMode::Html),
    )
    .await?;

    let mut prev_msg = target.message_id;
    for msg in msgs {
        let mut send = bot.send_message(chat_id, msg);
        send.reply_to_message_id = Some(prev_msg);
        send.disable_web_page_preview = Some(true);
        send.parse_mode = Some(teloxide::types::ParseMode::Html);
        let msg = send.await?;
        prev_msg = msg.id;
    }
    Ok(())
}
