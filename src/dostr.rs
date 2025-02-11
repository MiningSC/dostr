use log::{error, debug, info};
use std::fmt::Write;
use rand::Rng;
use crate::simpledb;
use crate::fetch;
use crate::utils;
use serenity::model::id::ChannelId;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write as IoWrite;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

type Receiver = tokio::sync::mpsc::Receiver<ConnectionMessage>;
type ErrorSender = tokio::sync::mpsc::Sender<ConnectionMessage>;

#[derive(PartialEq, Debug)]
enum ConnectionStatus {
    Success,
    Failed,
}

#[derive(Debug)]
pub struct ConnectionMessage {
    status: ConnectionStatus,
    timestamp: std::time::SystemTime,
}

pub struct DostrState {
    pub config: utils::Config,
    pub db: simpledb::Database,
    pub sender: nostr_bot::Sender,
    pub error_sender: tokio::sync::mpsc::Sender<ConnectionMessage>,
    pub started_timestamp: u64,
    pub discord_context: std::sync::Arc<tokio::sync::Mutex<Option<serenity::prelude::Context>>>,
}

#[derive(Serialize, Deserialize)]
struct NameDirectory {
    names: HashMap<String, String>,
}

pub type State = nostr_bot::State<DostrState>;

pub async fn error_listener(
    mut rx: Receiver,
    sender: nostr_bot::Sender,
    keypair: secp256k1::KeyPair,
) {
    // If the message of the same kind as last one was received in less than this, discard it to
    // prevent spamming
    let discard_period = std::time::Duration::from_secs(360000);

    let mut last_accepted_message = ConnectionMessage {
        status: ConnectionStatus::Success,
        timestamp: std::time::SystemTime::now() - discard_period,
    };

    while let Some(message) = rx.recv().await {
        let mut message_to_send = std::option::Option::<String>::None;
        
        if message.status != last_accepted_message.status {
            match message.status {
                ConnectionStatus::Success => {
                 //   message_to_send = Some("Connection to Discord reestablished! :)".to_string());
                }
                ConnectionStatus::Failed => {
                 //   message_to_send = Some("I can't connect to Discord right now :(.".to_string());
                }
            }

            last_accepted_message = message;
            
        } else {
            let duration_since_last_accepted = message
                .timestamp
                .duration_since(last_accepted_message.timestamp)
                .unwrap();

            debug!(
                "Since last accepted message: {:?}, discard period: {:?}",
                duration_since_last_accepted, discard_period
            );

            if duration_since_last_accepted >= discard_period {
                match message.status {
                    ConnectionStatus::Success => {}
                    ConnectionStatus::Failed => {
                        message_to_send =
                            Some("I'm still unable to connect to Discord :(".to_string());
                    }
                }
                last_accepted_message = message;
            }
        }
     //   println!("Message to send: {:?}", message_to_send);

        if let Some(message_to_send) = message_to_send {
            let event = nostr_bot::EventNonSigned {
                created_at: utils::unix_timestamp(),
                kind: 1,
                tags: vec![],
                content: message_to_send,
            }
            .sign(&keypair);

            sender.lock().await.send(event).await;
        }
    }
}

pub async fn channel_relays(
    event: nostr_bot::Event,
    _state: State,
    bot: nostr_bot::BotInfo,
) -> nostr_bot::EventNonSigned {
    let mut text = "Right now I'm connected to these relays:\n".to_string();

    let relays = bot.connected_relays().await;
    for relay in relays {
        writeln!(text, "{}", relay).unwrap();
    }

    nostr_bot::get_reply(event, text)
}

pub async fn channel_list(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let follows = state.lock().await.db.lock().unwrap().get_follows();
    let mut channel_ids = follows.keys().collect::<Vec<_>>();
    channel_ids.sort();

    let mut tags = nostr_bot::tags_for_reply(event);
    let orig_tags_count = tags.len();

    let mut text = format!("Hi, I'm following {} accounts:\n", channel_ids.len());
    for (index, &channel_id) in channel_ids.iter().enumerate() {
        let (keypair, _name) = follows.get(channel_id).unwrap();
        let public_key = keypair.x_only_public_key();
        tags.push(vec![
            "p".to_string(),
            public_key.0.to_string(),
        ]);
        writeln!(text, "#[{}]", index + orig_tags_count).unwrap();
    }

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        kind: 1,
        tags,
        content: text,
    }
}



pub async fn channel_random(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let follows = state.lock().await.db.lock().unwrap().get_follows();

    if follows.is_empty() {
        return nostr_bot::get_reply(
            event,
            String::from(
                "Hi, there are no channels. Try to add some using 'add discord_channel_id' command.",
            ),
        );
    }

    let random_index = rand::thread_rng().gen_range(0..follows.len());
    let random_channel_id = follows.keys().nth(random_index).unwrap();
    let (keypair, _) = follows.get(random_channel_id).unwrap();
    let public_key = keypair.x_only_public_key();

    let mut tags = nostr_bot::tags_for_reply(event);
    tags.push(vec![
        "p".to_string(),
        public_key.0.to_string(),
    ]);
    let mention_index = tags.len() - 1;

    debug!("Command random: returning {}", random_channel_id);
    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        kind: 1,
        tags,
        content: format!("Hi, random channel to follow: #[{}]", mention_index),
    }
}



pub async fn channel_add(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    
let config = utils::parse_config();
let words = event.content.split_whitespace().collect::<Vec<_>>();
if words.len() < 2 {
    debug!("Invalid !add command >{}< (missing account name).", event.content);
    return nostr_bot::get_reply(event, "Error: Missing account name.".to_string());
}

let input = words[1].trim();
let (channel_id, channel_name) = if input.starts_with('@') {
    // This is a Twitter handle.
    let channel_id = format!("https://{}/{}/rss", &config.nitter_instance, &input[1..]);  // removing '@'
    let channel_name = input[1..].to_string();  // removing '@', no reference
    (channel_id, channel_name)
} else {
    // This is a Discord channel ID.
    let data: Vec<String> = input
        .splitn(2, ':')
        .map(String::from)
        .collect();

    let channel_id = data[0].clone();
    let channel_name = if data.len() > 1 { 
        data[1..].join(":").trim().to_string() 
    } else { 
        channel_id.clone()
    }; // if name is not provided, use the channel id as the name
    (channel_id, channel_name)
};

    let db = state.lock().await.db.clone();
    let config = state.lock().await.config.clone();

    if db.lock().unwrap().contains_key(&channel_id) {
        let keypair = simpledb::get_channel_keypair(&channel_id, db);
        let (pubkey, _parity) = keypair.x_only_public_key();
        debug!(
            "Channel ID {} already added before. Sending existing pubkey {}",
            channel_id, pubkey
        );
        return get_channel_response(event, &pubkey.to_string());
    }

    if db.lock().unwrap().follows_count() + 1 > config.max_follows {
        return nostr_bot::get_reply(event,
            format!("Hi, sorry, couldn't add new channel. I'm already running at my max capacity ({} channels).", config.max_follows));
    }

    let state_lock = state.lock().await;
    let discord_context_option = state_lock.discord_context.lock().await.clone();
    let keypair = utils::get_random_keypair();

    db.lock()
        .unwrap()
        .insert( channel_id.clone(), keypair.display_secret().to_string(), channel_name.clone(),)
        .unwrap();

    let (xonly_pubkey, _) = keypair.x_only_public_key();

    info!(
        "Starting worker for channel ID {}, pubkey {}",
        channel_id, xonly_pubkey
    );

    // Convert the xonly_pubkey to a string for use in the JSON file.
    let public_key_string = format!("{}", xonly_pubkey); 

    // Update the JSON file
    let result = update_json_file(channel_name.clone(), public_key_string);
    if let Err(e) = result {
        error!("Failed to update JSON file: {:?}", e);
        // you could return an error here or decide how to handle it
    }

    let sender = state_lock.sender.clone();
    let tx = state_lock.error_sender.clone();
    let refresh_interval_secs = config.refresh_interval_secs;
    let state_clone = state.clone();

    // Check if channel_id is a number (Discord) or a URL (RSS feed)
    match channel_id.parse::<u64>() {
        Ok(channel_id_num) => {
            // Handle as Discord channel
            let channel_id_num = ChannelId(channel_id_num);

            if let Some(discord_context) = discord_context_option {
                let channel_type = fetch::ChannelType::Discord(channel_id_num);
            
                if fetch::channel_exists(&channel_id_num, Arc::new(discord_context)).await {
                    tokio::spawn(async move {
                        update_channel(
                            channel_type,
                            &keypair,
                            sender,
                            tx,
                            refresh_interval_secs,
                            state_clone,
                            channel_name,
                        )
                        .await;
                    });
            
                    return get_channel_response(event, &xonly_pubkey.to_string());
                } else {
                    return nostr_bot::get_reply(
                        event,
                        format!("Hi, I wasn't able to find channel ID {} on Discord.", channel_id),
                    );
                }
            } else {
                return nostr_bot::get_reply(
                    event,
                    format!("Hi, I can't add Discord channels at this time because I don't have access to Discord."),
                );
            }
        }
        Err(_) => {
            // Handle as RSS feed
            let channel_type = fetch::ChannelType::RSS(channel_id.clone());

            tokio::spawn(async move {
                update_channel(
                    channel_type,
                    &keypair,
                    sender,
                    tx,
                    refresh_interval_secs,
                    state_clone,
                    channel_name,
                )
                .await;
            });
        
            return get_channel_response(event, &xonly_pubkey.to_string());
        }
    }
}

fn update_json_file(channel_name: String, public_key: String) -> std::io::Result<()> {
    // Load the JSON file
    let mut file = File::open("web/.well-known/nostr.json")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Parse the JSON data
    let mut directory: NameDirectory = serde_json::from_str(&contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Add new channel_name and public key to the directory
    directory.names.insert(channel_name, public_key);

    // Convert the updated directory back to JSON
    let updated_json = serde_json::to_string_pretty(&directory)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Write the updated JSON back to the file
    let mut file = File::create("web/.well-known/nostr.json")?;
    file.write_all(updated_json.as_bytes())?;

    Ok(())
}

pub async fn uptime(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let uptime_seconds = nostr_bot::unix_timestamp() - state.lock().await.started_timestamp;
    nostr_bot::get_reply(
        event,
        format!(
            "Running for {}.",
            compound_duration::format_dhms(uptime_seconds)
        ),
    )
}

fn get_channel_response(event: nostr_bot::Event, new_bot_pubkey: &str) -> nostr_bot::EventNonSigned {
    let mut all_tags = nostr_bot::tags_for_reply(event);
    all_tags.push(vec!["p".to_string(), new_bot_pubkey.to_string()]);
    let last_tag_position = all_tags.len() - 1;

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        kind: 1,
        tags: all_tags,
        content: format!(
            "Hi, messages will be forwarded to nostr by #[{}].",
            last_tag_position
        ),
    }
}

pub async fn start_existing(state: State) {

    let state_lock = state.lock().await;
    let follows = state_lock.db.lock().unwrap().get_follows();

    for (channel_id, (keypair, channel_name)) in follows {

        let refresh = state_lock.config.refresh_interval_secs;
        let sender_clone = state_lock.sender.clone();
        let error_sender_clone = state_lock.error_sender.clone();
        let state_clone = state.clone();

        if let Ok(channel_id_num) = channel_id.parse::<u64>() {
            tokio::spawn(async move {
                update_channel(
                    fetch::ChannelType::Discord(ChannelId(channel_id_num)),
                    &keypair,
                    sender_clone,
                    error_sender_clone,
                    refresh,
                    state_clone,
                    channel_name.clone(),
                )
                .await;
            });
        } else {
            tokio::spawn(async move {
                update_channel(
                    fetch::ChannelType::RSS(channel_id.clone()),
                    &keypair,
                    sender_clone,
                    error_sender_clone,
                    refresh,
                    state_clone,
                    channel_name.clone(),
                )
                .await;
            });
        }
    }

    info!("Done starting tasks for followed channels.");
}

#[allow(dead_code)]
async fn fake_worker(channel_id: String, refresh_interval_secs: u64) {
    loop {
        debug!(
            "Fake worker for channel {}  is going to sleep for {} s",
            channel_id, refresh_interval_secs
        );
        tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;
        debug!("Faking the work for channel {}", channel_id);
    }
}

pub async fn update_channel(
    channel: fetch::ChannelType,
    keypair: &secp256k1::KeyPair,
    sender: nostr_bot::Sender,
    tx: ErrorSender,
    refresh_interval_secs: u64,
    state: Arc<Mutex<DostrState>>,
    channel_name: String,
) {
    let config = utils::parse_config();
    let state_lock = state.lock().await;
    let discord_context_option = state_lock.discord_context.lock().await.clone();
    drop(state_lock);

    match channel {
        fetch::ChannelType::Discord(channel_id) => {
            if let Some(discord_context) = discord_context_option {
                let discord_context = Arc::new(discord_context);
                let rssfeed = format!("https://{}/{}/rss", &config.nitter_instance, channel_name);
                let pfp = fetch::get_pic_url(&rssfeed).await;
                let about = fetch::get_about(&rssfeed).await;
                let display_name = fetch::get_display_name(&rssfeed).await;
                let banner = fetch::get_banner_link(&rssfeed).await;

                let event = nostr_bot::Event::new(
                    keypair,
                    utils::unix_timestamp(),
                    0,
                    vec![],
                    format!(
                        r#"{{
                            "name":"{}",
                            "display_name":"{}",
                            "about":"{} \n\nDiscord feed generated by @{}",
                            "picture":"{}",
                            "banner":"{}",
                            "nip05":"{}@{}"
                        }}"#,
                        channel_name, display_name, about, &config.botpub, pfp, banner, channel_name, &config.domain
                    ),
                );

                sender.lock().await.send(event).await;

                let mut since: chrono::DateTime<chrono::offset::Utc> =
                    std::time::SystemTime::now().into();

                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs))
                        .await;

                    let until = std::time::SystemTime::now().into();

                    let new_messages = fetch::get_new_messages(
                        discord_context.clone(),
                        channel_id,
                        since,
                        until,
                    )
                    .await;

                    match new_messages {
                        Ok(new_messages) => {
                            since = until;

                            for message in new_messages.iter().rev() {
                                let event_non_signed = fetch::get_discord_event(&message).await;
                                let signed_event = event_non_signed.sign(keypair);
                                sender.lock().await.send(signed_event).await;
                            }

                            tx.send(ConnectionMessage {
                                status: ConnectionStatus::Success,
                                timestamp: std::time::SystemTime::now(),
                            })
                            .await
                            .unwrap();
                        }
                        Err(e) => {
                            tx.send(ConnectionMessage {
                                status: ConnectionStatus::Failed,
                                timestamp: std::time::SystemTime::now(),
                            })
                            .await
                            .unwrap();

                            error!("Failed to get new messages for channel {}: {}", channel_id, e);
                        }
                    }
                }
            } else {
                error!(
                    "Failed to update channel {}: Discord context is not available.",
                    channel_id
                );
            }
        }
        fetch::ChannelType::RSS(channel_id) => {
            let rssfeed = format!("https://{}/{}/rss", &config.nitter_instance, channel_name);
            let pfp = fetch::get_pic_url(&rssfeed).await;
            let about = fetch::get_about(&rssfeed).await;
            let display_name = fetch::get_display_name(&rssfeed).await;
            let banner = fetch::get_banner_link(&rssfeed).await;

            let event = nostr_bot::Event::new(
                keypair,
                utils::unix_timestamp(),
                0,
                vec![],
                format!(
                    r#"{{
                        "name":"{}",
                        "display_name":"{}",
                        "about":"{} \n\nTwitter feed generated by @{}",
                        "picture":"{}",
                        "banner":"{}",
                        "nip05":"{}@{}"
                    }}"#,
                    channel_name, display_name, about, &config.botpub, pfp, banner, channel_name, &config.domain
                ),
            );

            sender.lock().await.send(event).await;
            
            let mut since: chrono::DateTime<chrono::offset::Utc> =
                chrono::offset::Utc::now();
        
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;
        
                let until = chrono::offset::Utc::now();
        
                let new_items = fetch::get_new_rss_items(&rssfeed, &since, &until).await;
        
                match new_items {
                    Ok(items) => {
                        since = until;
        
                        for item in items.into_iter() {
                            let event_non_signed = fetch::get_rss_event(&item).await;
                            let signed_event = event_non_signed.sign(keypair);
                            sender.lock().await.send(signed_event).await;
                        }
        
                        tx.send(ConnectionMessage {
                            status: ConnectionStatus::Success,
                            timestamp: std::time::SystemTime::now(),
                        })
                        .await
                        .unwrap();
                    }
                    Err(e) => {
                        tx.send(ConnectionMessage {
                            status: ConnectionStatus::Failed,
                            timestamp: std::time::SystemTime::now(),
                        })
                        .await
                        .unwrap();
        
                        error!(
                            "Failed to get new items for RSS channel {}: {}",
                            channel_id, e
                        );
                    }
                }
            }
        }        
    }
}