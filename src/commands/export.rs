use std::sync::Arc;

use anyhow::Context;
use teloxide::{
    requests::Requester,
    types::{InputFile, Message},
    Bot, utils::command::parse_command,
};
use tokio::sync::Mutex;

use crate::{data::Database, BOT_NAME};
use crate::opml::into_opml;

use super::{check_channel_permission, update_response, MsgTarget};

pub async fn export(
    bot: Bot,
    msg: Message,
    db: Arc<Mutex<Database>>,
) -> Result<(), anyhow::Error> {
    let chat_id = msg.chat.id;
    let mut target_id = chat_id;
    let target = &mut MsgTarget::new(chat_id, msg.id);

    let (_, args) = parse_command(msg.text().context("content of command text is empty")?, BOT_NAME.get().unwrap())
        .context("failed to parse command")?;
    let channel = args.get(0);

    if let Some(channel) = channel {
        let channel_id = check_channel_permission(&bot, &msg, &channel, target).await?;
        if channel_id.is_none() {
            return Ok(());
        }
        target_id = channel_id.unwrap();
    }

    let feeds = db.lock().await.subscribed_feeds(target_id.0);
    if feeds.is_none() {
        update_response(&bot, target, tr!("subscription_list_empty"), None).await?;
        return Ok(());
    }
    let opml = into_opml(feeds.unwrap());

    let file = InputFile::memory(opml.into_bytes()).file_name("feeds.opml");
    let mut send = bot.send_document(chat_id, file);
    send.reply_to_message_id = Some(msg.id);
    send.await?;
    Ok(())
}
