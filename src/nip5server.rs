use crate::utils;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use warp::Filter;
use tokio::fs;
use serde::{Deserialize, Serialize};
use warp::http::StatusCode;

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
            Ok::<_, warp::Rejection>(warp::reply::with_header(
                warp::reply::json(&response),
                "Access-Control-Allow-Origin",
                "*",
            ))
        });

    let current_dir = env::current_dir().expect("Failed to get current directory");

    let static_files = warp::fs::dir(current_dir.join("webstatic"));

    let routes = well_known.or(static_files);

    let config = utils::parse_config();
    let port = config.web_port;

    warp::serve(routes.recover(handle_rejection)).run(([0, 0, 0, 0], port)).await;
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "Not Found";
    } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
        code = StatusCode::BAD_REQUEST;
        message = "Invalid Body";
    } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "Method Not Allowed";
    } else {
        eprintln!("unhandled rejection: {:?}", err);
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error";
    }

    let json = warp::reply::json(&{
        let mut map = HashMap::new();
        map.insert("error", message);
        map
    });

    Ok(warp::reply::with_status(json, code))
}