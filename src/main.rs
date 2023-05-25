use log::debug;
use nostr_bot::FunctorType;

mod simpledb;
mod dostr;
mod discord;
mod utils;

use dostr::State;
use discord::{Handler};
use serenity::Client;
use tokio::sync::Mutex;
use std::sync::Arc;
use serenity::prelude::Context;
use std::env;

#[tokio::main]
async fn main() {
    nostr_bot::init_logger();

    let discord_context: Arc<Mutex<Option<Context>>> = Arc::new(Mutex::new(None));

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("Usage: {} --clearnet|--tor", args[0]);
        std::process::exit(1);
    }

    // Discord bot setup and start.
    // Replace "DISCORD_TOKEN" with your actual Discord bot token
    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a Discord token in the environment");

    let mut discord_client = Client::builder(&discord_token)
        .event_handler(Handler { discord_context: Arc::clone(&discord_context) })
        .await
        .expect("Err creating Discord client");

    let discord_future = discord_client.start();

    let config_path = std::path::PathBuf::from("config");
    let config = utils::parse_config(&config_path);
    debug!("{:?}", config);

    let keypair = nostr_bot::keypair_from_secret(&config.secret);
    let sender = nostr_bot::new_sender();

    let (tx, rx) = tokio::sync::mpsc::channel::<dostr::ConnectionMessage>(64);
    let state = nostr_bot::wrap_state(dostr::DostrState {
        config: config.clone(),
        sender: sender.clone(),
        db: std::sync::Arc::new(std::sync::Mutex::new(simpledb::SimpleDatabase::from_file(
            "data/users".to_string(),
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
            nostr_bot::Command::new("!add", nostr_bot::wrap!(dostr::handle_add))
                .description("Add new account to be followed by the bot."),
        )
        .command(
            nostr_bot::Command::new("!random", nostr_bot::wrap!(dostr::handle_random))
                .description("Returns random account the bot is following."),
        )
        .command(
            nostr_bot::Command::new("!list", nostr_bot::wrap!(dostr::handle_list))
                .description("Returns list of all accounts that the bot follows."),
        )
        .command(
            nostr_bot::Command::new("!relays", nostr_bot::wrap_extra!(dostr::handle_relays))
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
        "--tor" => bot = bot.use_socks5("127.0.0.1:9050"),
        _ => panic!("Incorrect network settings"),
    }

    // Run both the Nostr bot and the Discord bot concurrently
    tokio::select! {
        _ = bot.run() => {}
        _ = discord_future => {}
    }

}
