
#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub about: String,
    pub picture_url: String,
    pub hello_message: String,
    pub secret: String,
    pub botpub: String,
    pub apik: String,
    pub web_port: u16,
    pub nitter_instance: String,
    pub domain: String,
    pub refresh_interval_secs: u64,
    pub relays: Vec<String>,
    pub max_follows: usize,
}


impl std::fmt::Debug for Config {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Config")
            .field("name", &self.name)
            .field("about", &self.about)
            .field("picture_url", &self.picture_url)
            .field("hello_message", &self.hello_message)
            .field("secret", &"***")
            .field("botpub", &self.botpub)
            .field("apik", &"***")
            .field("web_port", &self.web_port)
            .field("nitter_instance", &self.nitter_instance)
            .field("domain", &self.domain)
            .field("refresh_interval_secs", &self.refresh_interval_secs)
            .field("relays", &self.relays)
            .field("max_follows", &self.max_follows)
            .finish()
    }
}

pub fn parse_config() -> Config {
    let name = std::env::var("BOTNAME").unwrap_or_default();
    let about = std::env::var("ABOUT").unwrap_or_default();
    let picture_url = std::env::var("PICTURE_URL").unwrap_or_default();
    let hello_message = std::env::var("HELLO_MESSAGE").unwrap_or_default();
    let secret = std::env::var("SECRET").unwrap_or_default();
    let botpub = std::env::var("BOTPUB").unwrap_or_default();
    let apik = std::env::var("APIK").unwrap_or_default();
    let web_port = std::env::var("WEB_PORT").unwrap_or_default().parse::<u16>().unwrap_or_default();
    let nitter_instance = std::env::var("NITTER_INSTANCE").unwrap_or_default();
    let domain = std::env::var("DOMAIN").unwrap_or_default();
    let refresh_interval_secs = std::env::var("REFRESH_INTERVAL_SECS").unwrap_or_default().parse::<u64>().unwrap_or_default();
    let max_follows = std::env::var("MAX_FOLLOWS").unwrap_or_default().parse::<usize>().unwrap_or_default();
    let add_relay = std::env::var("ADD_RELAY").unwrap_or_default();
    let relays: Vec<String> = add_relay.split(',').map(|s| s.to_string()).collect();

    assert!(!name.is_empty(), "The NAME environment variable is not set.");
    assert!(!about.is_empty(), "The ABOUT environment variable is not set.");
    assert!(!picture_url.is_empty(), "The PICTURE_URL environment variable is not set.");
    assert!(!hello_message.is_empty(), "The HELLO_MESSAGE environment variable is not set.");
    assert!(!secret.is_empty(), "The SECRET environment variable is not set.");
    assert!(!botpub.is_empty(), "The BOTPUB environment variable is not set");
    assert!(!apik.is_empty(), "The APIK environment variable is not set.");
    assert!(web_port > 0, "The WEB_PORT environment variable is not set or zero.");
    assert!(!nitter_instance.is_empty(), "The NITTER_INSTANCE environment variable is not set.");
    assert!(!domain.is_empty(), "The DOMAIN environment variable is not set.");
    assert!(refresh_interval_secs > 0, "The REFRESH_INTERVAL_SECS environment variable is not set or zero.");
    assert!(!relays.is_empty(), "The ADD_RELAY environment variable is not set.");
    assert!(max_follows > 0, "The MAX_FOLLOWS environment variable is not set or zero.");

    Config {
        name,
        about,
        picture_url,
        hello_message,
        secret,
        botpub,
        apik,
        web_port,
        nitter_instance,
        domain,
        refresh_interval_secs,
        relays,
        max_follows,
    }
}


pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn get_random_keypair() -> secp256k1::KeyPair {
    let secp = secp256k1::Secp256k1::new();
    let secret = secp256k1::SecretKey::new(&mut rand::thread_rng());
    secret.keypair(&secp)
}
