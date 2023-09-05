use std::sync::Arc;

use teloxide::{requests::Requester, types::ChatId, Bot};
use tokio::{
    self,
    sync::Mutex,
    time::{self, Duration},
};

use crate::data::Database;
use crate::BOT_ID;

pub fn start_pruning(bot: Bot, db: Arc<Mutex<Database>>) {
    let mut interval = time::interval(Duration::from_secs(24 * 60 * 60));
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            if let Err(e) = prune(&bot, &db).await {
                // crate::print_error(e);
                eprintln!("Error: {}", e);
                e.chain().skip(1).for_each(|cause| eprintln!("caused by: {}", cause));
            }
        }
    });
}

async fn prune(bot: &Bot, db: &Mutex<Database>) -> Result<(), anyhow::Error> {
    let subscribers = db.lock().await.all_subscribers();
    for subscriber in subscribers {
        let chat_id = ChatId(subscriber);
        let chat = bot.get_chat(chat_id).await?;
        if chat.is_group() || chat.is_supergroup() || chat.is_channel() {
            let me = bot.get_chat_member(chat_id, *BOT_ID.get().unwrap()).await?;
            // Bots can only be added as administrators in channel,
            // so we don't need to check that.
            // And just ignore `can_post_messages` or `can_send_messages`
            if me.is_left() || me.is_banned() {
                db.lock().await.delete_subscriber(subscriber);
            }
        }
    }
    Ok(())
}
