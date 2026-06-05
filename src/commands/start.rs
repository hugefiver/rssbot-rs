use teloxide::{Bot, types::Message};

use super::{MsgTarget, update_response};

pub async fn start(bot: Bot, msg: Message) -> Result<(), anyhow::Error> {
    let target = &mut MsgTarget::new(msg.chat.id, msg.id);
    let msg = tr!("start_message");
    update_response(
        &bot,
        target,
        msg,
        Some(teloxide::types::ParseMode::MarkdownV2),
    )
    .await?;
    Ok(())
}
