use std::sync::Arc;

use anyhow::Context;
use either::Either;
use pinyin::{Pinyin, ToPinyin};
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::requests::Requester;
use teloxide::types::{LinkPreviewOptions, Message, ReplyParameters};
use teloxide::utils::command::parse_command;
use tokio::sync::Mutex;

use crate::data::Database;
use crate::messages::{Escape, format_large_msg};

use super::{MsgTarget, check_channel_permission, update_response};

pub async fn rss(bot: Bot, msg: Message, db: Arc<Mutex<Database>>) -> Result<(), anyhow::Error> {
    let chat_id = msg.chat.id;
    let (_, mut args) = parse_command(
        msg.text().context("content of command text is empty")?,
        crate::BOT_NAME.get().unwrap(),
    )
    .context("failed to parse command")?;

    let raw = if let Some(i) = args.iter().position(|arg| *arg == "raw") {
        args.remove(i);
        true
    } else {
        false
    };
    let channel = args.first();
    let mut target_id = chat_id;
    let target = &mut MsgTarget::new(chat_id, msg.id);

    if let Some(channel) = channel {
        let channel_id = check_channel_permission(&bot, &msg, channel, target).await?;
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
            if !raw {
                format!(
                    "<a href=\"{}\">{}</a>",
                    Escape(&feed.link),
                    Escape(&feed.title)
                )
            } else {
                format!(
                    "<b>{}</b>\n<code>{}</code>\n",
                    Escape(&feed.title),
                    Escape(&feed.link)
                )
            }
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
        let mut send = bot
            .send_message(chat_id, msg)
            .link_preview_options(LinkPreviewOptions {
                is_disabled: true,
                url: None,
                prefer_large_media: false,
                prefer_small_media: false,
                show_above_text: false,
            })
            .reply_parameters(ReplyParameters {
                message_id: prev_msg,
                ..Default::default()
            });
        send.parse_mode = Some(teloxide::types::ParseMode::Html);
        let msg = send.await?;
        prev_msg = msg.id;
    }
    Ok(())
}
