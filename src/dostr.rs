use log::{debug, info, warn};
use std::fmt::Write;
use rand::Rng;
use crate::simpledb;
use crate::discord;
use crate::utils;
//use serenity::prelude::Context;
use serenity::model::id::ChannelId;
use tokio::sync::Mutex;
use std::sync::Arc;

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

    // error_receiver: tokio::sync::mpsc::Receiver<bot::ConnectionMessage>,
    pub error_sender: tokio::sync::mpsc::Sender<ConnectionMessage>,
    pub started_timestamp: u64,
    pub discord_context: std::sync::Arc<tokio::sync::Mutex<Option<serenity::prelude::Context>>>,
}

pub type State = nostr_bot::State<DostrState>;

pub async fn error_listener(
    mut rx: Receiver,
    sender: nostr_bot::Sender,
    keypair: secp256k1::KeyPair,
) {
    // If the message of the same kind as last one was received in less than this, discard it to
    // prevent spamming
    let discard_period = std::time::Duration::from_secs(3600);

    let mut last_accepted_message = ConnectionMessage {
        status: ConnectionStatus::Success,
        timestamp: std::time::SystemTime::now() - discard_period,
    };

    while let Some(message) = rx.recv().await {
        let mut message_to_send = std::option::Option::<String>::None;

        if message.status != last_accepted_message.status {
            match message.status {
                ConnectionStatus::Success => {
                    message_to_send = Some("Connection to Discord reestablished! :)".to_string());
                }
                ConnectionStatus::Failed => {
                    message_to_send = Some("I can't connect to Discord right now :(.".to_string());
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

    let mut text = format!("Hi, I'm following {} channels:\n", channel_ids.len());
    for (index, &channel_id) in channel_ids.iter().enumerate() {
        let secret = follows.get(channel_id).unwrap();
        tags.push(vec![
            "p".to_string(),
            secret.x_only_public_key().0.to_string(),
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

    let index = rand::thread_rng().gen_range(0..follows.len());

    let random_channel_id = follows.keys().collect::<Vec<_>>()[index];

    let secret = follows.get(random_channel_id).unwrap();

    let mut tags = nostr_bot::tags_for_reply(event);
    tags.push(vec![
        "p".to_string(),
        secret.x_only_public_key().0.to_string(),
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
    let words = event.content.split_whitespace().collect::<Vec<_>>();
    if words.len() < 2 {
        debug!("Invalid !add command >{}< (missing channel ID).", event.content);
        return nostr_bot::get_reply(event, "Error: Missing channel ID.".to_string());
    }

    let channel_id = words[1]
        .to_ascii_lowercase()
        .replace('#', "");

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
    let channel_id_num = ChannelId(channel_id.parse::<u64>().expect("Failed to parse channel ID"));
    // Check if the Context is present and pass it to the function
    if let Some(discord_context) = discord_context_option {
        if !discord::channel_exists(&channel_id_num, Arc::new(discord_context)).await {
            return nostr_bot::get_reply(
                event,
                format!("Hi, I wasn't able to find channel ID {} on Discord :(.", channel_id),
            );
        }
    } else {
        return nostr_bot::get_reply(
            event,
            format!("Hi, there is no active Discord context. Please ensure that the bot is connected to Discord."),
        );
    }

    let keypair = utils::get_random_keypair();

    db.lock()
        .unwrap()
        .insert(channel_id.clone(), keypair.display_secret().to_string())
        .unwrap();
    let (xonly_pubkey, _) = keypair.x_only_public_key();
    let channel_id = channel_id.to_string();
    info!(
        "Starting worker for channel ID {}, pubkey {}",
        channel_id, xonly_pubkey
    );

    {
        let sender = state_lock.sender.clone();
        let tx = state_lock.error_sender.clone();
        let refresh_interval_secs = config.refresh_interval_secs;
        let state_clone = state.clone();
        if let Ok(channel_id_num) = channel_id.parse::<u64>() {
            tokio::spawn(async move {
                update_channel(channel_id_num, &keypair, sender, tx, refresh_interval_secs, state_clone).await;
            });
        } else {
            warn!("Failed to parse channel_id to u64: {}", channel_id);
        }
    }
    

    get_channel_response(event, &xonly_pubkey.to_string())
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
    let db = state_lock.db.lock().unwrap();
    let error_sender = state_lock.error_sender.clone();
    let config_refresh_interval_secs = state_lock.config.refresh_interval_secs;
    let sender = state_lock.sender.clone();

    for (channel_id, keypair) in db.get_follows() {
        info!("Starting worker for channel_id {}", channel_id);

        let refresh = config_refresh_interval_secs;
        let sender_clone = sender.clone();
        let error_sender_clone = error_sender.clone();
        let state_clone = state.clone();
        
        if let Ok(channel_id_num) = channel_id.parse::<u64>() {
            tokio::spawn(async move {
                update_channel(channel_id_num, &keypair, sender_clone, error_sender_clone, refresh, state_clone).await;
            });
        } else {
            warn!("Failed to parse channel_id to u64: {}", channel_id);
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
    channel_id: u64, // Change from username: String to channel_id: u64
    keypair: &secp256k1::KeyPair,
    sender: nostr_bot::Sender,
    tx: ErrorSender,
    refresh_interval_secs: u64,
    state: Arc<Mutex<DostrState>>,
) {
    let state_lock = state.lock().await;
    let discord_context_option = state_lock.discord_context.lock().await.clone();

    if let Some(discord_context) = discord_context_option {
        let discord_context = Arc::new(discord_context);
        let event = nostr_bot::Event::new(
            keypair,
            utils::unix_timestamp(),
            0,
            vec![],
            format!(
                r#"{{"name":"dostr_{}","about":"Messages forwarded from https://discord.com/channels/{} by [dostr](https://github.com/MiningSC/dostr) bot."}}"#,
                channel_id, channel_id
            ),
        );

        sender.lock().await.send(event).await;

        let mut since: chrono::DateTime<chrono::offset::Local> = std::time::SystemTime::now().into();

        loop {
            debug!(
                "Worker for channel {} is going to sleep for {} s",
                channel_id, refresh_interval_secs
            );
            tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;

            let until = std::time::SystemTime::now().into();
            let new_messages = discord::get_new_messages(discord_context.clone(), ChannelId::from(channel_id), since, until).await;

            match new_messages {
                Ok(new_messages) => {
                    since = until;

                    for message in new_messages.iter().rev() {
                        let event_non_signed = discord::get_discord_event(&message, message.get_message()).await;
                        let signed_event = event_non_signed.sign(keypair);
                        sender
                            .lock()
                            .await
                            .send(signed_event)
                            .await;
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
                    warn!("{}", e);
                }
            }
        }
    } else {
        // Handle case where the Option is None
    }
}

