use std::sync::Arc;

use anyhow::Context;
// use tbot::{contexts::Command, types::parameters};
use teloxide::{types::Message, utils::command::parse_command, Bot};
use tokio::sync::Mutex;

use crate::messages::Escape;
use crate::{data::Database, BOT_NAME};

use super::{check_channel_permission, update_response, MsgTarget};

pub async fn unsub(
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
            let msg = tr!("unsub_how_to_use");
            update_response(&bot, target, msg, None).await?;
            return Ok(());
        }
    };
    let msg = if let Some(feed) = db.lock().await.unsubscribe(target_id.0, feed_url) {
        tr!(
            "unsubscription_succeeded",
            link = Escape(&feed.link),
            title = Escape(&feed.title)
        )
    } else {
        tr!("unsubscribed_from_rss").into()
    };
    update_response(&bot, target, &msg, Some(teloxide::types::ParseMode::Html)).await?;
    Ok(())
}
