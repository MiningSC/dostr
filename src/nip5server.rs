use crate::utils;

use serde_json::json;
use std::collections::HashMap;
use std::env;
use warp::Filter;
use tokio::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Entry {
    names: HashMap<String, String>,
}

async fn load_data() -> Result<Entry, Box<dyn std::error::Error + Send + Sync>> {
    let current_dir = env::current_dir()?;
    let data = fs::read_to_string(current_dir.join("web/.well-known/nostr.json")).await?;
    let entries: Entry = serde_json::from_str(&data)?;
    Ok(entries)
}

pub async fn start_server() {
    let well_known = warp::path(".well-known")
        .and(warp::path("nostr.json"))
        .and(warp::query::<HashMap<String, String>>().or_else(|_| async { Ok::<_, warp::Rejection>((HashMap::new(),)) }))
        .and_then(|mut query: HashMap<String, String>| async move {
            let data = load_data().await.unwrap();
            let name = query.remove("name").unwrap_or_default();
            let mut response = HashMap::new();
            let names = data.names.get(&name).cloned().unwrap_or_else(|| "Not found".to_string());
            response.insert("names", json!({name: names}));
            Ok::<_, warp::Rejection>(warp::reply::json(&response))
        });

    let current_dir = env::current_dir().expect("Failed to get current directory");

    let static_files = warp::fs::dir(current_dir.join("webstatic"));

    let routes = well_known.or(static_files);
    let config = utils::parse_config();
    let port = config.web_port;

    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}