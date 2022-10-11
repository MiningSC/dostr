use isahc::prelude::*;
use log::{debug, info, warn};

pub type TwitterInfo = std::sync::Arc<std::sync::Mutex<ConnectionInfo>>;

#[derive(Clone)]
pub struct ConnectionInfo {
    pub bearer: Option<String>,
    pub guest_token: Option<String>,
    pub conn_type: nostr_bot::ConnectionType,
}

async fn send_request(url: &str, info: TwitterInfo) -> Result<String, String> {
    let result = send_request_impl(url, info.clone()).await;
    match result {
        Ok(result) => {
            let js: serde_json::Value = serde_json::from_str(&result).unwrap();

            if js.is_object() {
                let obj = match js.as_object() {
                    Some(obj) => obj,
                    None => panic!("Failed to send request"),
                };

                if obj.contains_key("errors") {
                    warn!("Failed to send request, trying to get new access tokens and sending again.");
                    refresh_tokens(info.clone()).await?;
                    return send_request_impl(url, info.clone())
                        .await
                        .map_err(|e| e.to_string());
                };

                Ok(result)
            } else {
                Ok(result)
            }
        }
        Err(_) => {
            warn!("Failed to send request, trying to get new access tokens and sending again.");
            refresh_tokens(info.clone()).await?;

            match send_request_impl(url, info).await {
                Ok(result) => {
                    debug!("New access tokens working.");
                    Ok(result)
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

async fn refresh_tokens(info: TwitterInfo) -> Result<(), String> {
    let conn_type = info.lock().unwrap().conn_type.clone();
    let new_info = get_info(conn_type).await.map_err(|e| e.to_string())?;
    info!(
        "Got new guest token: {:?}",
        new_info.lock().unwrap().guest_token
    );

    *info.lock().unwrap() = new_info.lock().unwrap().clone();

    Ok(())
}

async fn send_request_impl(url: &str, info: TwitterInfo) -> Result<String, std::io::Error> {
    log::debug!(
        "Running send_request_impl with bearer >{:?}<",
        info.lock().unwrap().bearer
    );
    let req = isahc::Request::get(url);
    let info = info.lock().unwrap().clone();

    debug!(
        "Sending request to {} with authorization={:?}, x-guest-token={:?}",
        url, info.bearer, info.guest_token
    );

    let req = match &info.bearer {
        Some(bearer) => req.header("authorization", bearer),
        None => req,
    };

    let req = match &info.guest_token {
        Some(guest_token) => req.header("x-guest-token", guest_token),
        None => req,
    };

    let req = match info.conn_type {
        nostr_bot::ConnectionType::Direct => req,
        nostr_bot::ConnectionType::Socks5 => req.proxy(isahc::http::uri::Uri::from_static(
            "socks5h://127.0.0.1:9050",
        )),
    };

    req.body("").unwrap().send_async().await?.text().await
}

pub async fn get_info(conn_type: nostr_bot::ConnectionType) -> Result<TwitterInfo, std::io::Error> {
    let dummy_info = std::sync::Arc::new(std::sync::Mutex::new(ConnectionInfo {
        bearer: None,
        guest_token: None,
        conn_type: conn_type.clone(),
    }));

    // Rewrite of the Python code found at https://unam.re/blog/making-twitter-api
    let text =
        send_request_impl("https://twitter.com/home?precache=1", dummy_info.clone()).await?;

    let js_with_bearer = {
        let d = select::document::Document::from(text.as_str());

        d.find(select::predicate::Name("script"))
            .filter_map(|n| n.attr("src"))
            .filter(|x| x.contains("/main"))
            .collect::<Vec<_>>()[0]
            .to_string()
    };

    let re = regex::Regex::new(r#""gt=(\d{19})"#).unwrap();

    let guest_token = match re.captures_iter(&text).next() {
        Some(gt) => match gt.get(1) {
            Some(gt) => gt.as_str().to_string(),
            None => panic!("Unable to get guest token"),
        },
        None => panic!(),
    };

    // let guest_token = None;
    info!("guest_token: {:?}", guest_token);

    let text = send_request_impl(&js_with_bearer, dummy_info.clone()).await?;

    // Regexp from twint: grep -E ",[a-z]=\"[^\"]*\",[a-z]=\"[0-9]{8}
    // Orig Rust regexp
    // let re = regex::Regex::new(r#",[a-zA-Z]="([^"]*)",[a-zA-Z]="\d{8}"#).unwrap();
    let re = regex::Regex::new(r#",s="([^"]*)",[a-zA-Z]="\d{8}"#).unwrap();
    let bearer = match re.captures_iter(&text).last() {
        Some(gt) => match gt.get(1) {
            Some(gt) => gt.as_str(),
            None => panic!(),
        },
        None => panic!(),
    };
    info!("bearer: {}", bearer);

    Ok(std::sync::Arc::new(std::sync::Mutex::new(ConnectionInfo {
        bearer: Some(format!("Bearer {}", bearer)),
        guest_token: Some(guest_token),
        conn_type,
    })))
}

async fn tweet_request(
    username: &str,
    since_timestamp: u64,
    until_timestamp: u64,
    cursor: &str,
    info: TwitterInfo,
) -> Result<String, String> {
    let url = "https://api.twitter.com/2/search/adaptive.json";
    let search_str = format!(
        "from:{}%20since:{}%20until:{}",
        username, since_timestamp, until_timestamp
    );
    let params = vec![
        ("include_can_media_tag", "1"),
        ("include_ext_alt_text", "true"),
        ("include_quote_count", "true"),
        ("include_reply_count", "1"),
        ("tweet_mode", "extended"),
        ("include_entities", "true"),
        ("include_user_entities", "true"),
        ("include_ext_media_availability", "true"),
        ("send_error_codes", "true"),
        ("simple_quoted_tweet", "true"),
        ("count", "100"),
        ("query_source", "typed_query"),
        ("cursor", cursor),
        ("spelling_corrections", "1"),
        ("ext", "mediaStats%2ChighlightedLabel"),
        ("tweet_search_mode", "live"),
        ("f", "tweets"),
        ("q", &search_str),
    ];
    // let headers = vec![
    // ("authorization", format!("Bearer {}", bearer)),
    // ("x-guest-token", guest_token.to_string()),
    // ];

    let params = params
        .iter()
        .map(|x| format!("{}={}", x.0, x.1))
        .collect::<Vec<_>>();
    let x = params.join("&");

    let complete = format!("{}?{}", url, x);
    send_request(&complete, info.clone()).await
}

fn parse_tweets(username: &str, json: serde_json::Value) -> Vec<crate::twitter::Tweet> {
    let date_format = "%a %h %d %T %z %Y";
    let mut tweets = vec![];
    for (key, value) in json["globalObjects"]["tweets"].as_object().unwrap() {
        let created_at = value.as_object().unwrap()["created_at"]
            .as_str()
            .unwrap()
            .to_string();
        let text = value.as_object().unwrap()["full_text"]
            .as_str()
            .unwrap()
            .to_string();

        let timestamp = chrono::DateTime::parse_from_str(&created_at, date_format)
            .unwrap()
            .timestamp() as u64;
        let link = format!("https://twitter.com/{}/status/{}", username, key);

        tweets.push(crate::twitter::Tweet {
            timestamp,
            // created_at,
            tweet: text,
            link,
        });
    }

    tweets
}

pub async fn get_tweets(
    username: &str,
    since_timestamp: u64,
    until_timestamp: u64,
    info: TwitterInfo,
) -> Result<Vec<crate::twitter::Tweet>, String> {
    let mut all_tweets = vec![];
    let mut cursor = "-1".to_string();
    // Tweets are not sent in one batch but cursor needs to be set for new "page",
    // assuming here 10 requests should be usually enough
    for _i in 0..10 {
        let response = tweet_request(
            username,
            since_timestamp,
            until_timestamp,
            &cursor,
            info.clone(),
        )
        .await
        .map_err(|e| format!("Fail while calling tweet_request: {}", e))?;

        let js: serde_json::Value = serde_json::from_str(&response).unwrap();

        cursor = get_cursor(&js)?;

        let mut new_tweets = parse_tweets(username, js);
        if new_tweets.is_empty() {
            break;
        }

        all_tweets.append(&mut new_tweets)
    }

    all_tweets.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap());
    let mut all_tweets = all_tweets
        .into_iter()
        .filter(|t| t.timestamp >= since_timestamp && !t.tweet.starts_with('@'))
        .collect::<Vec<_>>();

    // Follow links to the final destinations
    crate::twitter::follow_links(&mut all_tweets, info.clone()).await;

    Ok(all_tweets)
}

fn try_get_cursor(js: &serde_json::Value) -> Result<String, String> {
    let error_message = format!(
        "Getting cursor but json response does not contain expected data, response: >{}<.",
        js
    );

    let entries = js["timeline"]["instructions"][0]["addEntries"]["entries"]
        .as_array()
        .ok_or(&error_message)?;

    let entry = entries.iter().last().ok_or(&error_message)?;

    if let Some(cursor) = entry["content"]["operation"]["cursor"]["value"].as_str() {
        Ok(cursor.to_string())
    } else {
        let cursor = js["timeline"]["instructions"]
            .as_array()
            .ok_or(&error_message)?
            .iter()
            .last()
            .ok_or(&error_message)?["replaceEntry"]["entry"]["content"]["operation"]["cursor"]
            ["value"]
            .as_str()
            .ok_or(&error_message)?
            .to_string();

        Ok(cursor)
    }
}

// https://github.com/minamotorin/twint/blob/9bfe0d5fb708c9e09cd2b9456bad08576569cb51/twint/feed.py#L57
fn get_cursor(js: &serde_json::Value) -> Result<String, String> {
    let error_message = String::from("Cursor find fallback failed");

    match try_get_cursor(js) {
        Ok(cursor) => Ok(cursor),
        Err(e) => {
            log::warn!("Unable to get cursor on the first try: {}", e);
            // Fallback
            let cursor = js
                .as_object()
                .ok_or(&error_message)?
                .into_iter()
                .last()
                .ok_or(&error_message)?
                .1
                .to_string();
            info!("Got cursor on the second try.");
            Ok(cursor)
        }
    }
}

pub async fn get_pic_url(username: &str, info: TwitterInfo) -> Result<String, String> {
    let profile_url = format!("https://api.twitter.com/graphql/jMaTS-_Ea8vh9rpKggJbCQ/UserByScreenName?variables=%7B%22screen_name%22%3A%20%22{}%22%2C%20%22withHighlightedLabel%22%3A%20false%7D", username);

    let response = send_request(&profile_url, info)
        .await
        .map_err(|e| format!("Unable to connect to Twitter.: {}", e))?;

    let js: serde_json::Value = serde_json::from_str(&response).unwrap();

    let url = js["data"]["user"]["legacy"]["profile_image_url_https"].to_string();
    if url == "null" {
        return Err(format!("Unable to find user {}.", username));
    }

    if url.len() > 2 {
        // Remove ""
        let mut url = url.chars();
        url.next();
        url.next_back();
        Ok(url.as_str().to_string())
    } else {
        Ok(url)
    }
}
