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
use std::collections::{HashSet, HashMap};
use url::{Url, ParseError as UrlParseError};
use reqwest::Client;
use futures::stream::StreamExt;

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

    let new_items_stream = futures::stream::iter(
        items.into_iter().filter_map(move |item| {
            let pub_date = item
                .pub_date()
                .and_then(|pub_date| chrono::DateTime::parse_from_rfc2822(pub_date).ok())
                .map(|datetime| datetime.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::DateTime::from_utc(chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap_or_else(|| chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap_or_else(|| {panic!("Invalid timestamp");})), chrono::Utc));

            if pub_date > *since && pub_date <= *until {
                Some(item)
            } else {
                None
            }
        })
    ).then(|item| async move {
        let description = item.description().unwrap_or_default();
        let titletest = "title";

        // Fetch the linked page and find the video link (if any)
        let video_link = match item.link() {
            Some(link) => {
                match find_video_link(link).await {
                    Ok(video_link) => video_link.to_owned(),
                    Err(err) => {
                        info!("Error finding video link: {}", err);
                        String::new() // or handle the error as desired
                    }
                }
            }
            None => String::new() // handle the case where link is None
        };

        let video_link_found = !video_link.is_empty();
        // Pass the video_link_found boolean to the remove_html_tags function
        let stripped_description = remove_html_tags(&description, video_link_found);

        // Append the video link to the description
        let description_with_video = format!("{}\n\n{}", stripped_description, video_link);

        RSSItem {
            timestamp: item
                .pub_date()
                .and_then(|pub_date| chrono::DateTime::parse_from_str(pub_date, "%a, %d %b %Y %H:%M:%S GMT").ok())
                .map(|datetime| datetime.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::DateTime::from_utc(chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap_or_else(|| { chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap_or_else(||{ panic!("Invalid timestamp");}) }), chrono::Utc)),
            title: titletest.to_string(),
            description: description_with_video,
            link: item.link().unwrap_or_default().to_string(),
        }
    });

    let new_items: Vec<_> = new_items_stream.collect().await;
    Ok(new_items)
}



// Helper function to find the video link on the linked page
async fn find_video_link(link: &str) -> Result<String, reqwest::Error> {
    // Create a reqwest client
    let client = Client::new();

    // Send a GET request to the link and fetch the HTML content
    let response = client.get(link).send().await?;
    let body = response.text().await?;

    // Parse the HTML content using scraper
    let fragment = Html::parse_document(&body);

    // Define a CSS selector to find the video source element
    let video_selector = Selector::parse("source").unwrap();

    // Find the first source element matching the selector
    if let Some(video_element) = fragment.select(&video_selector).next() {
        // Extract the video URL from the "src" attribute
        if let Some(content) = video_element.value().attr("src") {
            return Ok(content.to_owned());
        }
    }
    // Return an empty string if no video link was found
    Ok(String::new())
}

// Function to remove HTML tags using a regular expression
fn remove_html_tags(description: &str, video_link_found: bool) -> String {
    let fragment = Html::parse_fragment(description);
    let img_selector = Selector::parse("img").unwrap();
    let a_selector = Selector::parse("a").unwrap();
    let video_selector = Selector::parse("source").unwrap();

    let mut text_parts = Vec::new();
    let mut links = HashMap::new();
    let mut excluded_links = HashSet::new();

    for element in fragment.select(&img_selector) {
        if let Some(link) = element.value().attr("src") {
            let normalized_link = normalize_link(link);
            if !links.contains_key(&normalized_link) {
                links.insert(normalized_link, link.to_string());
            }
        }
    }

    for element in fragment.select(&a_selector) {
        if let Some(link) = element.value().attr("href") {
            let normalized_link = normalize_link(link);
            // Check if the link is the same as the text of the <a> tag
            let text = element.inner_html();
            if normalize_link(&text) == normalized_link {
                excluded_links.insert(normalized_link.clone());
            }
            if contains_profile_link(&normalized_link, description) || contains_search_link(&normalized_link) {
                excluded_links.insert(normalized_link.clone());
            } else if !links.contains_key(&normalized_link) {
                links.insert(normalized_link.clone(), normalize_link(link));
            }
        }
    }

    for element in fragment.select(&video_selector) {
        if let Some(link) = element.value().attr("src") {
            let normalized_link = normalize_link(link);
            if !links.contains_key(&normalized_link) {
                links.insert(normalized_link.clone(), normalize_link(link));
            }
        }
    }

    // Remove HTML tags from the text parts
    let re = regex::Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(description, "").to_string();
    text_parts.push(text);

    // Combine the text and unique media parts
    let mut result = text_parts.join(" ");
    for (normalized_link, original_link) in &links {
        if !excluded_links.contains(normalized_link) {
            // Check if a video link was found and the current link is an image link
            if !(video_link_found && original_link.starts_with("https://")) {
                result.push_str(" ");
                result.push_str(original_link);
            }
        }
    }

    result
}


fn normalize_link(link: &str) -> String {
    let url = Url::parse(link);
    match url {
        Ok(mut url) => {
            url.set_scheme("https").unwrap();
            url.to_string()
        },
        Err(UrlParseError::RelativeUrlWithoutBase) => format!("https://{}", link),
        Err(e) => panic!("URL parsing error: {}", e),
    }
}

fn contains_profile_link(link: &str, description: &str) -> bool {
    let config = utils::parse_config();

    // Check for @mentions in hyperlinks
    let hyperlink_username_regex = regex::Regex::new(r"<a[^>]*>@(\w+)</a>").unwrap();
    let hyperlink_referenced_usernames: Vec<&str> = hyperlink_username_regex
        .captures_iter(description)
        .map(|capture| capture.get(1).unwrap().as_str())
        .collect();

    // Check for @mentions in the overall description
    let description_username_regex = regex::Regex::new(r"@(\w+)").unwrap();
    let description_referenced_usernames: Vec<&str> = description_username_regex
        .captures_iter(description)
        .map(|capture| capture.get(1).unwrap().as_str())
        .collect();

    // Combine the two sets of usernames into one
    let all_referenced_usernames: Vec<&str> = [&hyperlink_referenced_usernames[..], &description_referenced_usernames[..]].concat();

    let link_lower = link.to_lowercase(); // Convert link to lowercase
    all_referenced_usernames.iter().any(|username| {
        let lower_username = username.to_lowercase();
        let username_link1 = format!("/{}", lower_username);
        let username_link2 = format!("https://{}/{}", config.nitter_instance, lower_username);

        if link == &username_link2 {
            return false;
        }
  
        let link = link_lower.trim_end_matches('/'); // Trim trailing slash
        link.contains(&username_link1)
    })
}


fn contains_search_link(link: &str) -> bool {
    link.contains("/search?q=")
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