use crate::utils;
use crate::simpledb::SimpleDatabase;
//use serenity::model::channel::GuildChannel;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use std::sync::Arc;

pub struct DiscordMessage {
    timestamp: u64,
    message: String,
}

#[allow(dead_code)]
impl DiscordMessage {
    pub fn get_message(&self) -> &String {
        &self.message
    }
}

pub struct Handler {
    pub discord_context: Arc<Mutex<Option<Context>>>,
    pub db_client: Arc<Mutex<SimpleDatabase>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, msg: Message) {
        // Check if the channel is followed
        let follows = self.db_client.lock().await.get_follows();
        if !follows.contains_key(&msg.channel_id.to_string()) {
            return;
        }

        let discord_message = DiscordMessage {
            timestamp: msg.timestamp.timestamp() as u64,
            message: msg.content.clone(),
        };

        get_discord_event(&discord_message).await;
    }

    async fn ready(&self, context: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let mut discord_context = self.discord_context.lock().await;
        *discord_context = Some(context);
    }
}



pub async fn channel_exists(channel_id: &ChannelId, ctx: Arc<Context>) -> bool {
    match channel_id.to_channel(&(*ctx)).await {
        Ok(_) => true,
        Err(_) => false,
    }
}
#[allow(dead_code)]
pub async fn get_channel_name(channel_id: &ChannelId, ctx: Arc<Context>) -> Result<String, Box<dyn std::error::Error>> {
    let channel = channel_id.to_channel(&(*ctx)).await?;

    match channel {
        serenity::model::channel::Channel::Guild(channel) => Ok(channel.name().to_owned()),
        _ => Err("The provided channel is not a GuildChannel".into())
    }
}

pub async fn get_new_messages(
    ctx: Arc<Context>,
    channel_id: ChannelId,
    since: chrono::DateTime<chrono::offset::Local>,
    until: chrono::DateTime<chrono::offset::Local>,
) -> Result<Vec<DiscordMessage>, String> {
    let messages = channel_id.messages(&ctx, |retriever| retriever.limit(100)).await;

    match messages {
        Ok(retrieved_messages) => {
            let mut new_messages = vec![];
            for message in retrieved_messages {
                let message_timestamp = message.timestamp.timestamp();
                if message_timestamp >= since.timestamp() && message_timestamp < until.timestamp() {
                    new_messages.push(DiscordMessage {
                        timestamp: message_timestamp as u64,
                        message: message.content,
                    });
                }
            }
            Ok(new_messages)
        }
        Err(why) => Err(format!("Error getting messages: {:?}", why)),
    }
}

pub async fn get_discord_event(discord_message: &DiscordMessage) -> nostr_bot::EventNonSigned {
    // Print the message content to the console
    println!("Discord message content: {:?}", discord_message.message);

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        tags: vec![],
        kind: 1,
        content: discord_message.message.clone(),
    }
}
