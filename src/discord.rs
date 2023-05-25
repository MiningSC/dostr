use crate::utils;

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::{ChannelId, UserId}},
    prelude::*,
};
// use std::env;
use std::sync::Arc;

struct DiscordMessage {
    timestamp: u64,
    message: String,
}

pub struct Handler {
    discord_context: std::sync::Arc<tokio::sync::Mutex<Option<serenity::prelude::Context>>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let discord_message = DiscordMessage {
            timestamp: msg.timestamp.timestamp() as u64,
            message: msg.content.clone(),
        };
        
        get_discord_event(&discord_message, &msg.content).await;
    }

    async fn ready(&self, context: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let mut discord_context = self.discord_context.lock().await;
        *discord_context = Some(context);
    }
}

pub async fn user_exists(user_id: &str, ctx: Arc<Context>) -> bool {
    let user_id: u64 = match user_id.parse() {
        Ok(id) => id,
        Err(_) => return false, // Invalid user_id was provided
    };

    match UserId(user_id).to_user(&(*ctx)).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub async fn get_pic_url(user_id: &str, ctx: Arc<Context>) -> Result<String, Box<dyn std::error::Error>> {
    match user_id.parse::<u64>() {
        Ok(id) => {
            let user = UserId(id)
                .to_user(&(*ctx))
                .await?;

            Ok(user.face().to_owned())
        },
        Err(_) => {
            Err("Failed to parse user ID".into())
        }
    }
}

pub async fn get_new_messages(
    ctx: Arc<Context>,
    channel_id: ChannelId,
) -> Result<Vec<DiscordMessage>, String> {
    let messages = channel_id.messages(&ctx, |retriever| retriever.limit(100)).await;

    match messages {
        Ok(retrieved_messages) => {
            let mut new_messages = vec![];
            for message in retrieved_messages {
                new_messages.push(DiscordMessage {
                    timestamp: message.timestamp.timestamp() as u64,
                    message: message.content,
                });
            }
            Ok(new_messages)
        }
        Err(why) => Err(format!("Error getting messages: {:?}", why)),
    }
}

pub async fn get_discord_event(discord_message: &DiscordMessage, message_content: &String) -> nostr_bot::EventNonSigned {
    // Here, you would use the details from `discord_message` to construct an event,
    // similar to how you were constructing a Tweet event in the Twitter code.
    // Please fill this part with your own logic.
    let formatted = format!("{}", discord_message.message);
    
        nostr_bot::EventNonSigned {
            created_at: utils::unix_timestamp(),
            tags: vec![],
            kind: 1,
            content: formatted,
        }
}
