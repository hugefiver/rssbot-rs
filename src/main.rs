#![feature(error_generic_member_access)]
#![recursion_limit = "256"]

use std::env;
use std::panic;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use once_cell::sync::OnceCell;

use teloxide::types::UserId;
use tokio::{self, sync::Mutex};

// Include the tr! macro and localizations
include!(concat!(env!("OUT_DIR"), "/ctl10n_macros.rs"));

mod client;
mod commands;
mod data;
mod feed;
mod fetcher;
mod gardener;
mod messages;
mod opml;

use crate::data::Database;

static BOT_NAME: OnceCell<String> = OnceCell::new();
static BOT_ID: OnceCell<UserId> = OnceCell::new();

#[derive(Debug, clap::Parser)]
#[command(
    about = "A simple Telegram RSS bot.",
    after_help = "NOTE: You can get <user id> using bots like @userinfobot @getidsbot"
)]
pub struct Opt {
    /// Telegram bot token
    token: String,
    /// Path to database
    #[arg(
        short = 'd',
        long,
        value_name = "path",
        default_value = "./rssbot.json"
    )]
    database: PathBuf,
    /// Minimum fetch interval
    #[arg(
        long,
        value_name = "seconds",
        default_value = "300",
        value_parser(parse_check_interval)
    )]
    // default is 5 minutes
    min_interval: u32,
    /// Maximum fetch interval
    #[arg(
        long,
        value_name = "seconds",
        default_value = "43200",
        value_parser(parse_check_interval)
    )]
    // default is 12 hours
    max_interval: u32,
    /// Maximum feed size, 0 is unlimited
    #[arg(long, value_name = "bytes", default_value = "2097152")]
    // default is 2MiB
    max_feed_size: u64,
    /// Private mode, only specified user can use this bot.
    /// This argument can be passed multiple times to allow multiple admins
    #[arg(
        long,
        value_name = "user id",
        number_of_values = 1,
        alias = "single_user" // For compatibility
    )]
    admin: Vec<i64>,
    /// Make bot commands only accessible for group admins.
    #[arg(long)]
    restricted: bool,
    /// DANGER: Insecure mode, accept invalid TLS certificates
    #[arg(long)]
    insecure: bool,
}

fn parse_check_interval(s: &str) -> Result<u32, String> {
    s.parse::<u32>().map_err(|e| e.to_string()).and_then(|r| {
        if r < 1 {
            Err("must >= 1".into())
        } else {
            Ok(r)
        }
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_fail_fast();

    let opt = Opt::parse();
    let db = Arc::new(Mutex::new(Database::open(opt.database.clone())?));
    // let bot = if let Some(proxy) = init_proxy() {
    //     tbot::bot::Builder::with_string_token(opt.token.clone())
    //         .proxy(proxy)
    //         .build()
    // } else {
    //     tbot::Bot::new(opt.token.clone())
    // };
    let bot = teloxide::Bot::new(&opt.token);
    let me = bot
        .get_me()
        .await
        .context("Initialization failed, check your network and Telegram token")?;

    let bot_name = me.user.username.clone().context("Bot name is not set")?;
    let bot_id = me.user.id;
    crate::client::init_client(&bot_name, opt.insecure, opt.max_feed_size);

    BOT_NAME.set(bot_name).unwrap();
    BOT_ID.set(bot_id).unwrap();

    gardener::start_pruning(bot.clone(), db.clone());
    fetcher::start(bot.clone(), db.clone(), opt.min_interval, opt.max_interval);

    let opt = Arc::new(opt);

    use teloxide::prelude::*;

    // let handler = move |bot: Bot, msg: Message, cmd: commands::Command| {
    //     let opt_c = opt.clone();
    //     let db_c = db.clone();
    //     async move {
    //         commands::handle_command(bot, cmd, msg, db_c, opt_c).await;
    //     }
    // };
    // commands::Command::repl(bot, handler).await;

    let handler = Update::filter_message()
        .filter_command::<commands::Command>()
        .endpoint(
            |bot: Bot,
             cmd: commands::Command,
             msg: Message,
             db: Arc<Mutex<Database>>,
             opt: Arc<Opt>| async move {
                // let cmd =
                //     commands::Command::parse(msg.text().unwrap_or(""), BOT_NAME.get().unwrap())?;
                commands::handle_command(bot, cmd, msg, db, opt).await
            },
        );
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![db, opt])
        .default_handler(|_upd| async {})
        .error_handler(Arc::new(|e| async move {
            // eprintln!("tg error: {}", e);
            print_anyhow_error(e);
        }))
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    // let mut event_loop = bot.event_loop();
    // event_loop.username(me.user.username.unwrap());
    // commands::register_commands(&mut event_loop, opt, db);

    // event_loop
    //     .polling()
    //     .last_n_updates(NonZeroUsize::new(200).unwrap())
    //     // .limit(200)
    //     .allowed_updates(AllowedUpdates::none().message(true))
    //     .error_handler(|e| async {
    //         use tbot::errors::Polling::*;
    //         use tbot::errors::MethodCall;
    //         match e {
    //             Fetching(method_call) => match method_call {
    //                 MethodCall::Network(e) => {
    //                     eprintln!("[tbot polling] Network error: {}", e);
    //                 }
    //                 MethodCall::OutOfService => {
    //                     eprintln!("[tbot polling] Telegram is out of service");
    //                 }
    //                 MethodCall::Parse { error, response } => {
    //                     eprintln!("[tbot polling] Parse error: {}", error);
    //                     match String::from_utf8_lossy(&response[..]) {
    //                         s if s.is_empty() => {},
    //                         s => eprintln!("[tbot polling] Response: {}", s),
    //                     }
    //                 }
    //                 MethodCall::RequestError {
    //                     description,
    //                     error_code,
    //                     migrate_to_chat_id,
    //                     retry_after,
    //                 } => {
    //                     eprintln!(
    //                         "[tbot polling] Request error: {} (code: {}), migrate_to_chat_id: {:?}, retry_after: {:?}",
    //                         description, error_code, migrate_to_chat_id, retry_after
    //                     );
    //                 }
    //             }
    //             Timeout(_) => {
    //                 eprintln!("[tbot polling] Timeout");
    //             }
    //         }
    //     })
    //     .start()
    //     .await
    //     .unwrap();
    Ok(())
}

// Exit the process when any worker thread panicked
fn enable_fail_fast() {
    let default_panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |e| {
        default_panic_hook(e);
        process::exit(101);
    }));
}

// fn init_proxy() -> Option<Proxy> {
//     // Telegram Bot API only uses https, no need to check http_proxy
//     env::var("HTTPS_PROXY")
//         .or_else(|_| env::var("https_proxy"))
//         .map(|uri| {
//             let uri = uri
//                 .try_into()
//                 .unwrap_or_else(|e| panic!("Illegal HTTPS_PROXY: {}", e));
//             Proxy::new(Intercept::All, uri)
//         })
//         .ok()
// }

// fn print_error<E: std::error::Error>(err: E) {
//     eprintln!("Error: {}", err);
//     let mut err: &dyn std::error::Error = &err;
//     let mut deepest_backtrace = std::error::request_ref::<Backtrace>(err);
//     if let Some(e) = err.source() {
//         eprintln!("\nCaused by:");
//         let multiple = e.source().is_some();
//         let mut line_counter = 0..;
//         while let (Some(e), Some(line)) = (err.source(), line_counter.next()) {
//             if multiple {
//                 eprint!("{: >4}: ", line)
//             } else {
//                 eprint!("    ")
//             };
//             eprintln!("{}", e);

//             if let Some(backtrace) = std::error::request_ref::<Backtrace>(e) {
//                 deepest_backtrace = Some(backtrace);
//             }
//             err = e;
//         }
//     }

//     if let Some(backtrace) = deepest_backtrace {
//         eprintln!("\nBacktrace:\n{}", backtrace);
//     }
// }

fn print_anyhow_error(e: anyhow::Error) {
    eprintln!("Error: {}", e);
    e.chain()
        .skip(1)
        .take(10)
        .for_each(|e| eprintln!("  Caused by: {}", e));
    // if {
    //     if let Ok(v) = env::var("DEBUG") {
    //         !v.is_empty() && v != "0"
    //     } else {
    //         false
    //     }
    // } {
    //     let bt = e.backtrace();
    //     eprintln!("Backtrace:\n{:#?}", bt);
    // }
}
