use log::{debug, info, warn};
use std::fmt::Write;

use rand::Rng;

use crate::simpledb;
use crate::discord;
use crate::utils;
use serenity::prelude::Context;
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
                    message_to_send = Some("Connection to Twitter reestablished! :)".to_string());
                }
                ConnectionStatus::Failed => {
                    message_to_send = Some("I can't connect to Twitter right now :(.".to_string());
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
                            Some("I'm still unable to connect to Twitter :(".to_string());
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

pub async fn handle_relays(
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

pub async fn handle_list(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let follows = state.lock().await.db.lock().unwrap().get_follows();
    let mut usernames = follows.keys().collect::<Vec<_>>();
    usernames.sort();

    let mut tags = nostr_bot::tags_for_reply(event);
    let orig_tags_count = tags.len();

    let mut text = format!("Hi, I'm following {} accounts:\n", usernames.len());
    for (index, &username) in usernames.iter().enumerate() {
        let secret = follows.get(username).unwrap();
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

pub async fn handle_random(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let follows = state.lock().await.db.lock().unwrap().get_follows();

    if follows.is_empty() {
        return nostr_bot::get_reply(
            event,
            String::from(
                "Hi, there are no accounts. Try to add some using 'add discord_username' command.",
            ),
        );
    }

    let index = rand::thread_rng().gen_range(0..follows.len());

    let random_username = follows.keys().collect::<Vec<_>>()[index];

    let secret = follows.get(random_username).unwrap();

    let mut tags = nostr_bot::tags_for_reply(event);
    tags.push(vec![
        "p".to_string(),
        secret.x_only_public_key().0.to_string(),
    ]);
    let mention_index = tags.len() - 1;

    debug!("Command random: returning {}", random_username);
    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        kind: 1,
        tags,
        content: format!("Hi, random account to follow: #[{}]", mention_index),
    }
}

pub async fn handle_add(event: nostr_bot::Event, state: State) -> nostr_bot::EventNonSigned {
    let words = event.content.split_whitespace().collect::<Vec<_>>();
    if words.len() < 2 {
        debug!("Invalid !add command >{}< (missing username).", event.content);
        return nostr_bot::get_reply(event, "Error: Missing username.".to_string());
    }

    let username = words[1]
        .to_ascii_lowercase()
        .replace('@', "");

    let db = state.lock().await.db.clone();
    let config = state.lock().await.config.clone();

    if db.lock().unwrap().contains_key(&username) {
        let keypair = simpledb::get_user_keypair(&username, db);
        let (pubkey, _parity) = keypair.x_only_public_key();
        debug!(
            "User {} already added before. Sending existing pubkey {}",
            username, pubkey
        );
        return get_handle_response(event, &pubkey.to_string());
    }

    if db.lock().unwrap().follows_count() + 1 > config.max_follows {
        return nostr_bot::get_reply(event,
            format!("Hi, sorry, couldn't add new account. I'm already running at my max capacity ({} users).", config.max_follows));
    }

    let state_lock = state.lock().await;
    let discord_context_option = state_lock.discord_context.lock().await.clone();

    // Check if the Context is present and pass it to the function
    if let Some(discord_context) = discord_context_option {
        if !discord::user_exists(&username, Arc::new(discord_context)).await {
            return nostr_bot::get_reply(
                event,
                format!("Hi, I wasn't able to find {} on Discord :(.", username),
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
        .insert(username.clone(), keypair.display_secret().to_string())
        .unwrap();
    let (xonly_pubkey, _) = keypair.x_only_public_key();
    let username = username.to_string();
    info!(
        "Starting worker for username {}, pubkey {}",
        username, xonly_pubkey
    );

    {
        let sender = state_lock.sender.clone();
        let tx = state_lock.error_sender.clone();
        let refresh_interval_secs = config.refresh_interval_secs;
        let state_clone = state.clone();
        tokio::spawn(async move {
            update_user(username, &keypair, sender, tx, refresh_interval_secs, state_clone).await;
        });
    }

    get_handle_response(event, &xonly_pubkey.to_string())
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

fn get_handle_response(event: nostr_bot::Event, new_bot_pubkey: &str) -> nostr_bot::EventNonSigned {
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
    let db;
    let error_sender;
    let config_refresh_interval_secs;
    let sender;
    {
        let state_lock = state.lock().await;
        db = state_lock.db.lock().unwrap().clone();
        error_sender = state_lock.error_sender.clone();
        config_refresh_interval_secs = state_lock.config.refresh_interval_secs;
        sender = state_lock.sender.clone();
    }

    for (username, keypair) in db.get_follows() {
        info!("Starting worker for username {}", username);
        
        let refresh = config_refresh_interval_secs;
        let sender_clone = sender.clone();
        let error_sender_clone = error_sender.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            update_user(username, &keypair, sender_clone, error_sender_clone, refresh, state_clone).await;
        });
    }

    info!("Done starting tasks for followed accounts.");
}

#[allow(dead_code)]
async fn fake_worker(username: String, refresh_interval_secs: u64) {
    loop {
        debug!(
            "Fake worker for user {}  is going to sleep for {} s",
            username, refresh_interval_secs
        );
        tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;
        debug!("Faking the work for user {}", username);
    }
}

pub async fn update_user(
    username: String,
    keypair: &secp256k1::KeyPair,
    sender: nostr_bot::Sender,
    tx: ErrorSender,
    refresh_interval_secs: u64,
    state: Arc<Mutex<DostrState>>,
) {
    // fake_worker(username, refresh_interval_secs).await;
    // return;
    let state_lock = state.lock().await;
    let discord_context = &state_lock.discord_context;

    let pic_url = discord::get_pic_url(discord_context.clone(), &username).await.unwrap_or_default();
    let event = nostr_bot::Event::new(
        keypair,
        utils::unix_timestamp(),
        0,
        vec![],
        format!(
            r#"{{"name":"dostr_{}","about":"Messages forwarded from https://discord.com/users/{} by [dostr](https://github.com/MiningSC/dostr) bot.","picture":"{}"}}"#,
            username, username, pic_url
        ),
    );

    sender.lock().await.send(event).await;

    let mut since: chrono::DateTime<chrono::offset::Local> = std::time::SystemTime::now().into();

    loop {
        debug!(
            "Worker for @{} is going to sleep for {} s",
            username, refresh_interval_secs
        );
        tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;

        let until = std::time::SystemTime::now().into();
        let new_messages = discord::get_new_messages(discord_context, &username, since, until).await;

        match new_messages {
            Ok(new_messages) => {
                // --since seems to be inclusive and --until exclusive so this should be fine
                since = until;

                // twint returns newest messages first, reverse the Vec here so that messages are send to relays
                // in order they were published. Still the created_at field can easily be the same so in the
                // end it depends on how the relays handle it
                for message in new_messages.iter().rev() {
                    sender
                        .lock()
                        .await
                        .send(discord::get_discord_event(message).sign(keypair))
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
        // break;
    }
}
