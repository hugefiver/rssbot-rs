use std::sync::Arc;

use anyhow::Context;
use teloxide::{
    requests::Requester,
    types::{ChatId, Message, MessageId, ParseMode},
    ApiError, Bot, RequestError,
};
use tokio::sync::Mutex;

use crate::data::Database;

mod export;
mod rss;
mod start;
mod sub;
mod unsub;

#[derive(teloxide::utils::command::BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Subscribe to a feed")]
    Sub,
    #[command(description = "Unsubscribe from a feed")]
    Unsub,
    #[command(description = "Export your subscriptions")]
    Export,
    #[command(description = "Show the list of subscribed feeds")]
    Rss,
}

type DbState = Arc<Mutex<Database>>;

pub async fn handle_command(
    bot: Bot,
    cmd: Command,
    msg: Message,
    db: DbState,
    opt: Arc<crate::Opt>,
) -> anyhow::Result<()> {
    if !check_command(&opt, &bot, &msg, &cmd).await {
        return Ok(());
    }
    match cmd {
        Command::Start => start::start(bot, msg).await,
        Command::Sub => sub::sub(bot, msg, db).await,
        Command::Unsub => unsub::unsub(bot, msg, db).await,
        Command::Export => export::export(bot, msg, db).await,
        Command::Rss => rss::rss(bot, msg, db).await,
    }
}

// macro_rules! add_handlers {
//     ($event_loop: ident, $opt: ident, $env: ident, [$( $cmd: ident),*]) => {
//         $({
//             let env = $env.clone();
//             let opt = $opt.clone();
//             let h = move |cmd: Arc<Command>| {
//                 let env = env.clone();
//                 let opt = opt.clone();
//                 async move {
//                     if check_command(&opt, &cmd).await {
//                         if let Err(e) = self::$cmd::$cmd(env, cmd).await {
//                             crate::print_error(e);
//                         }
//                     }
//                 }
//             };
//             $event_loop.command(stringify!($cmd), h);
//         })*
//     };
// }

// pub fn register_commands(
//     event_loop: &mut tbot::EventLoop,
//     opt: Arc<crate::Opt>,
//     db: Arc<Mutex<Database>>,
// ) {
//     add_handlers!(event_loop, opt, db, [start, rss, sub, unsub, export]);
// }

pub async fn check_command(opt: &crate::Opt, bot: &Bot, msg: &Message, cmd: &Command) -> bool {
    let reply_target = &mut MsgTarget::new(msg.chat.id, msg.id);

    // Private mode
    if !opt.admin.is_empty() && !is_from_bot_admin(msg, &opt.admin) {
        eprintln!(
            "Unauthenticated request from user/channel: {}, command: {:?}",
            msg.from().map_or("<nil>".to_string(), |u| u.full_name()),
            cmd
        );
        return false;
    }

    use teloxide::types::ChatKind::*;
    use teloxide::types::ChatPublic;
    use teloxide::types::PublicChatKind::*;
    match msg.chat.kind {
        Public(ChatPublic {
            kind: Channel(_), ..
        }) => {
            let msg = tr!("commands_in_private_channel");
            let _ignore_result = update_response(bot, reply_target, msg, None).await;
            return false;
        }
        // Restrict mode: bot commands are only accessible to admins.
        Public(ChatPublic {
            kind: Group(_) | Supergroup(_),
            ..
        }) if opt.restricted => {
            let user_is_admin = is_from_chat_admin(bot, msg).await;
            if !user_is_admin {
                let _ignore_result: Result<(), anyhow::Error> =
                    update_response(bot, reply_target, tr!("group_admin_only_command"), None).await;
            }
            return user_is_admin;
        }
        _ => (),
    }

    true
}

fn is_from_bot_admin(msg: &Message, admins: &[i64]) -> bool {
    // match &cmd.from {
    //     Some(from) => {
    //         let id = match from {
    //             From::User(user) => user.id.0,
    //             From::Chat(chat) => chat.id.0,
    //         };
    //         admins.contains(&id)
    //     }
    //     None => false,
    // }

    let from = msg.from();
    match from {
        None => false,
        Some(u) => {
            let user_id = u.id;
            admins.contains(&(user_id.0 as i64))
        }
    }
}

async fn is_from_chat_admin(bot: &Bot, msg: &Message) -> bool {
    // match &cmd.from {
    //     Some(From::User(user)) => {
    //         let admins = match cmd.bot.get_chat_administrators(cmd.chat.id).call().await {
    //             Ok(r) => r,
    //             _ => return false,
    //         };
    //         admins.iter().any(|member| member.user.id == user.id)
    //     }
    //     Some(From::Chat(chat)) => chat.id == cmd.chat.id,
    //     None => false,
    // }
    let from = msg.from();
    match from {
        None => false,
        Some(u) => {
            if u.is_anonymous() {
                return true;
            }
            let user_id = u.id;
            let chat_id = msg.chat.id;
            let admins = bot
                .get_chat_administrators(chat_id)
                .await
                .unwrap_or_default();
            admins.iter().any(|member| member.user.id == user_id)
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct MsgTarget {
    chat_id: ChatId,
    message_id: MessageId,
    first_time: bool,
}

impl MsgTarget {
    fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        MsgTarget {
            chat_id,
            message_id,
            first_time: true,
        }
    }
    fn update(&mut self, message_id: MessageId) {
        self.message_id = message_id;
        self.first_time = false;
    }
}

async fn update_response(
    bot: &Bot,
    target: &mut MsgTarget,
    msg: &str,
    mode: Option<ParseMode>,
) -> Result<(), anyhow::Error> {
    let MsgTarget {
        chat_id,
        message_id,
        first_time,
    } = target;
    let msg = if *first_time {
        let mut send = bot.send_message(*chat_id, msg);
        send.reply_to_message_id = Some(*message_id);
        send.disable_web_page_preview = Some(true);
        send.parse_mode = mode;
        send.await?
    } else {
        let mut send = bot.edit_message_text(*chat_id, *message_id, msg);
        send.parse_mode = mode;
        send.await?
    };
    target.update(msg.id);
    Ok(())
}

async fn check_channel_permission(
    bot: &Bot,
    msg: &Message,
    channel: &str,
    target: &mut MsgTarget,
) -> Result<Option<ChatId>, anyhow::Error> {
    let user = msg.from().context("UNREACHABLE: message from channel")?;

    if user.is_anonymous() {
        // FIXME: error message
        return Ok(None);
    }

    let user_id = user.id;

    let channel_id = match channel.parse::<i64>() {
        Ok(id) => ChatId(id),
        Err(_) => bot.get_chat(channel.to_string()).await?.id,
    };

    update_response(bot, target, tr!("verifying_channel"), None).await?;

    let chat = match bot.get_chat(channel_id).await {
        Err(RequestError::Api(ApiError::ChatNotFound | ApiError::UserNotFound)) => {
            let msg = tr!("unable_to_find_target_channel", desc = "chat not found");
            update_response(bot, target, &msg, None).await?;
            return Ok(None);
        }
        e => e?,
    };
    if !chat.is_channel() {
        update_response(bot, target, tr!("target_must_be_a_channel"), None).await?;
        return Ok(None);
    }
    let admins = match bot.get_chat_administrators(channel_id).await {
        Err(RequestError::Api(ApiError::UserNotFound | ApiError::ChatNotFound)) => {
            let msg = tr!("unable_to_get_channel_info", desc = "not found");
            update_response(bot, target, &msg, None).await?;
            return Ok(None);
        }
        other => other?,
    };
    let user_is_admin = admins.iter().any(|member| member.user.id == user_id);
    if !user_is_admin {
        update_response(bot, target, tr!("channel_admin_only_command"), None).await?;
        return Ok(None);
    }
    let bot_is_admin = admins
        .iter()
        .any(|member| member.user.id == *crate::BOT_ID.get().unwrap());
    if !bot_is_admin {
        update_response(bot, target, tr!("make_bot_admin"), None).await?;
        return Ok(None);
    }
    Ok(Some(chat.id))
}
