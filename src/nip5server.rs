use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use warp::Filter;
use tokio::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Entry {
    names: HashMap<String, String>,
}

// This function simulates loading data from JSON.
// Replace with your actual loading function.
async fn load_data() -> Result<Entry, Box<dyn std::error::Error + Send + Sync>> {
    let data = fs::read_to_string("./web/.well-known/nostr.json").await?;
    let entries: Entry = serde_json::from_str(&data)?;
    Ok(entries)
}

pub async fn start_server() {
    let data = Arc::new(load_data().await.unwrap());

    let well_known = warp::path(".well-known")
        .and(warp::path("nostr.json"))
        .and(warp::query::<HashMap<String, String>>())
        .map(move |mut query: HashMap<String, String>| {
            let data = Arc::clone(&data);
            let name = query.remove("name").unwrap_or_default();
            let mut response = HashMap::new();
            let names = data.names.get(&name).cloned().unwrap_or_else(|| "Not found".to_string());
            response.insert("names", json!({name: names}));
            warp::reply::json(&response)
        });

    let routes = well_known.or(warp::fs::dir("./web"));

    warp::serve(routes).run(([127, 0, 0, 1], 3033)).await;
}

