mod simpledb;
mod dostr;
mod fetch;
mod utils;
mod nip5server;

use env_logger::Builder;
use log::LevelFilter;
use log::debug;
use nostr_bot::FunctorType;
use dostr::State;
use fetch::Handler;
use serenity::Client;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::env;
use serenity::prelude::Context;
use simpledb::SimpleDatabase;
use dotenv::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();

    Builder::new()
        .filter_module("tracing::span", LevelFilter::Off) // Exclude tracing::span logs
        .filter(None, LevelFilter::Warn) // Set the desired logging level here
        .init();

    let _server_handle = tokio::spawn(nip5server::start_server());

    let discord_context: Arc<Mutex<Option<Context>>> = Arc::new(Mutex::new(None));

    let current_dir = env::current_dir().unwrap();
    let db_file_path = current_dir.join("data/channels");
    let db_client = Arc::new(Mutex::new(SimpleDatabase::from_file(db_file_path.to_string_lossy().to_string())));

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("Usage: {} --clearnet|--tor", args[0]);
        std::process::exit(1);
    }

    // Instead of reading a configuration file, create the config directly from environment variables
    let config = utils::parse_config();
    debug!("{:?}", config);

    // Discord bot setup and start.

    let discord_token = &config.apik;

    let mut discord_client = Client::builder(&discord_token)
        .event_handler(Handler {
            discord_context: Arc::clone(&discord_context),
            db_client: Arc::clone(&db_client),
            sender: nostr_bot::new_sender(),
            keypair: nostr_bot::keypair_from_secret(&config.secret),
        })
        .await
        .expect("Err creating Discord client");

    let discord_future = discord_client.start();

    let keypair = nostr_bot::keypair_from_secret(&config.secret);
    let sender = nostr_bot::new_sender();

    let (tx, rx) = tokio::sync::mpsc::channel::<dostr::ConnectionMessage>(64);

    let current_dir = env::current_dir().unwrap();
    let db_file_path = current_dir.join("data/channels");
    
    let state = nostr_bot::wrap_state(dostr::DostrState {
        config: config.clone(),
        sender: sender.clone(),
        db: std::sync::Arc::new(std::sync::Mutex::new(simpledb::SimpleDatabase::from_file(
            db_file_path.to_string_lossy().to_string(),
        ))),
        error_sender: tx.clone(),
        started_timestamp: nostr_bot::unix_timestamp(),
        discord_context: Arc::clone(&discord_context),
    });

    let start_existing = {
        let state = state.clone();
        async move {
            dostr::start_existing(state).await;
        }
    };

    let error_listener = {
        let state = state.clone();
        let sender = state.lock().await.sender.clone();
        async move {
            dostr::error_listener(rx, sender, keypair).await;
        }
    };

    let relays = config.relays.iter().map(|r| r.as_str()).collect::<Vec<_>>();

    let mut bot = nostr_bot::Bot::<State>::new(keypair, relays, state)
        .name(&config.name)
        .about(&config.about)
        .picture(&config.picture_url)  
        .intro_message(&config.hello_message)
        .command(
            nostr_bot::Command::new("!add", nostr_bot::wrap!(dostr::channel_add))
                .description("Add new Twitter acount to be followed by the bot. For example, !add @nasa")
        )
        .command(
            nostr_bot::Command::new("!random", nostr_bot::wrap!(dostr::channel_random))
                .description("Returns random Twitter account the bot is following."),
        )
        .command(
            nostr_bot::Command::new("!list", nostr_bot::wrap!(dostr::channel_list))
                .description("Returns list of all Twitter accounts that the bot follows."),
        )
        .command(
            nostr_bot::Command::new("!relays", nostr_bot::wrap_extra!(dostr::channel_relays))
                .description("Show connected relay."),
        )
        .command(
            nostr_bot::Command::new("!uptime", nostr_bot::wrap!(dostr::uptime))
                .description("Prints for how long is the bot running."),
        )
        .help()
        .sender(sender)
        .spawn(Box::pin(start_existing))
        .spawn(Box::pin(error_listener));

    match args[1].as_str() {
        "--clearnet" => {}
        "--tor" => bot = bot.use_socks5("0.0.0.0:9050"),
        _ => panic!("Incorrect network settings"),
    }

    // Run both the Nostr bot and the Discord bot concurrently
    tokio::select! {
        _ = bot.run() => {}
        _ = discord_future => {}
    }
}
