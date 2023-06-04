use log::{debug, info};
use crate::utils;
use crate::simpledb::SimpleDatabase;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use std::sync::Arc;
use rss::Channel;
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};

pub enum ChannelType {
    Discord(ChannelId),
    RSS(String),
}

#[allow(dead_code)]
pub struct DiscordMessage {
    timestamp: u64,
    message: String,
}

pub struct RSSItem {
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub link: String,
}

pub struct Handler {
    pub discord_context: Arc<Mutex<Option<Context>>>,
    pub db_client: Arc<Mutex<SimpleDatabase>>,
    pub sender: nostr_bot::Sender,
    pub keypair: secp256k1::KeyPair,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, msg: Message) {
        let follows = self.db_client.lock().await.get_follows();

        if follows.contains_key(&msg.channel_id.to_string()) {
            let discord_message = DiscordMessage {
                timestamp: msg.timestamp.timestamp() as u64,
                message: msg.content.clone(),
            };

            let event_non_signed = get_discord_event(&discord_message).await;
            let signed_event = event_non_signed.sign(&self.keypair); 
            self.sender.lock().await.send(signed_event).await;
        } else {

        }
    }
    

    async fn ready(&self, context: Context, _ready: Ready) {
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
    since: chrono::DateTime<chrono::offset::Utc>,
    until: chrono::DateTime<chrono::offset::Utc>,
) -> Result<Vec<DiscordMessage>, String> {

    let messages = channel_id.messages(&ctx, |retriever| retriever.limit(10)).await;
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
        Err(why) => {
            Err(format!("Error getting messages: {:?}", why))
        },
    }
}


pub async fn get_discord_event(discord_message: &DiscordMessage) -> nostr_bot::EventNonSigned {

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        tags: vec![],
        kind: 1,
        content: discord_message.message.clone(),
    }
}

pub async fn get_rss_event(item: &RSSItem) -> nostr_bot::EventNonSigned {

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        tags: vec![],
        kind: 1,
        content: format!(
            "{}\n\n{}",
            item.description, item.link
        ),
    }
}

pub async fn get_pic_url(feed_url: &String) -> String {
    let content = match reqwest::get(feed_url).await {
        Ok(response) => response.bytes().await,
        Err(err) => {
            info!("Failed to fetch RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    let content_bytes = match content {
        Ok(bytes) => bytes,
        Err(err) => {
            info!("Failed to fetch content for RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    let content_str = match String::from_utf8(content_bytes.to_vec()) {
        Ok(string) => string,
        Err(err) => {
            info!("Failed to parse content for RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    let channel = match Channel::read_from(content_str.as_bytes()) {
        Ok(channel) => channel,
        Err(err) => {
            info!("Failed to parse RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    if let Some(image) = channel.image() {
        let pic_url = image.url().to_string();

        if pic_url.starts_with("http") {
            debug!("Found pic url {} for {}", pic_url, feed_url);
            return pic_url;
        }
    }

    info!("Unable to find picture for {}", feed_url);
    "".to_string()
}

pub async fn get_banner_link(feed_url: &str) -> String {
    let content = match reqwest::get(feed_url).await {
        Ok(response) => match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                info!("Failed to fetch content for RSS feed from URL {}: {}", feed_url, err);
                return "".to_string();
            }
        },
        Err(err) => {
            info!("Failed to fetch RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    let content_str = match String::from_utf8(content.to_vec()) {
        Ok(string) => string,
        Err(err) => {
            info!("Failed to parse content for RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    let channel = match Channel::read_from(content_str.as_bytes()) {
        Ok(channel) => channel,
        Err(err) => {
            info!("Failed to parse RSS feed from URL {}: {}", feed_url, err);
            return "".to_string();
        }
    };

    return channel.link().to_string();
}


pub async fn get_about(feed_url: &String) -> String {
    let content = reqwest::get(feed_url).await.unwrap().bytes().await.unwrap();
    let channel = Channel::read_from(&content[..]).unwrap();

    // Get the channel title
    let about = channel.description().to_string();

    let strippedabout = remove_about_html_tags(&about);

    if !about.is_empty() {
        debug!("Found about {} for {}", strippedabout, feed_url);
        return strippedabout.to_string();
    } else {
        info!("Unable to find about for {}", feed_url);
        "".to_string()
    }
}

pub async fn get_display_name(feed_url: &String) -> String {
    let content = reqwest::get(feed_url).await.unwrap().bytes().await.unwrap();
    let channel = Channel::read_from(&content[..]).unwrap();

    // Get the channel title
    let title = channel.title().to_string();

    // Truncate the title at the '/ @' marker
    let display_name = title.split("/ @").next().unwrap_or("");

    if !display_name.is_empty() {
        debug!("Found display name {} for {}", display_name, feed_url);
        return display_name.to_string();
    } else {
        info!("Unable to find display name for {}", feed_url);
        "".to_string()
    }
}

pub async fn get_new_rss_items(
    feed_url: &str,
    since: &chrono::DateTime<chrono::offset::Utc>,
    until: &chrono::DateTime<chrono::offset::Utc>,
) -> Result<Vec<RSSItem>, String> {

    let feed = match reqwest::get(feed_url).await {
        Ok(response) => response,
        Err(err) => return Err(format!("Failed to fetch RSS feed: {}", err)),
    };

    let body = match feed.text().await {
        Ok(body) => body,
        Err(err) => return Err(format!("Failed to read RSS feed response: {}", err)),
    };

    let channel = match Channel::read_from(body.as_bytes()) {
        Ok(channel) => channel,
        Err(err) => return Err(format!("Failed to parse RSS feed: {}", err)),
    };

    let items = channel.into_items();

    let new_items: Vec<RSSItem> = items
        .into_iter()
        .filter(|item| {
            let pub_date = item
                .pub_date()
                .and_then(|pub_date| chrono::DateTime::parse_from_rfc2822(pub_date).ok())
                .map(|datetime| datetime.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::DateTime::from_utc(chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(), chrono::Utc));
        
            pub_date > *since && pub_date <= *until
        })
        
        
        .map(|item| {
            let description = item.description().unwrap_or_default();
            let stripped_description = remove_html_tags(&description);
            let titletest = "title";
            RSSItem {
                timestamp: item
                    .pub_date()
                    .and_then(|pub_date| chrono::DateTime::parse_from_str(pub_date, "%a, %d %b %Y %H:%M:%S GMT").ok())
                    .map(|datetime| datetime.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::DateTime::from_utc(chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(), chrono::Utc)),
                title: titletest.to_string(),
                description: stripped_description,
                link: item.link().unwrap_or_default().to_string(),
            }
        })
        
        .collect();
    Ok(new_items)
}

// Function to remove HTML tags using a regular expression
fn remove_html_tags(description: &str) -> String {
    let fragment = Html::parse_fragment(description);
    let img_selector = Selector::parse("img").unwrap();
    let a_selector = Selector::parse("a").unwrap();
    let video_selector = Selector::parse("source").unwrap();

    let mut text_parts = Vec::new();
    let mut media_parts = Vec::new();

    for element in fragment.select(&img_selector) {
        if let Some(link) = element.value().attr("src") {
            media_parts.push(link.to_string());
        }
    }

    for element in fragment.select(&a_selector) {
        if let Some(link) = element.value().attr("href") {
            media_parts.push(link.to_string());
        }
    }

    for element in fragment.select(&video_selector) {
        if let Some(link) = element.value().attr("src") {
            media_parts.push(link.to_string());
        }
    }

    // Remove HTML tags from the text parts
    let re = regex::Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(description, "").to_string();
    text_parts.push(text);

    // Combine the text and media parts
    let mut result = text_parts.join(" ");
    for media in media_parts {
        result.push_str(" ");
        result.push_str(&media);
    }

    result
}

fn remove_about_html_tags(description: &str) -> String {
    let re_html_tags = regex::Regex::new(r"<[^>]*>").unwrap();
    let re_urls = regex::Regex::new(r"\bhttps?://\S+\b").unwrap();
    let re_newlines = regex::Regex::new(r"\n").unwrap();
    let re_at_symbols = regex::Regex::new(r"@").unwrap();

    let text_without_html_tags = re_html_tags.replace_all(description, "").to_string();
    let text_without_urls = re_urls.replace_all(&text_without_html_tags, "").to_string();
    let text_without_newlines = re_newlines.replace_all(&text_without_urls, "").to_string();
    let text_without_at_symbols = re_at_symbols.replace_all(&text_without_newlines, "").to_string().trim().to_string();

    text_without_at_symbols
}