use std::sync::Arc;

use anyhow::Context;
use teloxide::{
    Bot,
    payloads::SendDocumentSetters,
    requests::Requester,
    types::{InputFile, Message, ReplyParameters},
    utils::command::parse_command,
};
use tokio::sync::Mutex;

use crate::opml::into_opml;
use crate::{BOT_NAME, data::Database};

use super::{MsgTarget, check_channel_permission, update_response};

pub async fn export(bot: Bot, msg: Message, db: Arc<Mutex<Database>>) -> Result<(), anyhow::Error> {
    let chat_id = msg.chat.id;
    let mut target_id = chat_id;
    let target = &mut MsgTarget::new(chat_id, msg.id);

    let (_, args) = parse_command(
        msg.text().context("content of command text is empty")?,
        BOT_NAME.get().unwrap(),
    )
    .context("failed to parse command")?;
    let channel = args.first();

    if let Some(channel) = channel {
        let channel_id = check_channel_permission(&bot, &msg, channel, target).await?;
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
    let send = bot
        .send_document(chat_id, file)
        .reply_parameters(ReplyParameters {
            message_id: msg.id,
            ..Default::default()
        });
    send.await?;
    Ok(())
}
