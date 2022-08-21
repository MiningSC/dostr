use isahc::prelude::*;

// Inspired by https://unam.re/blog/making-twitter-api
#[derive(Clone)]
pub struct ConnectionInfo {
    bearer: Option<String>,
    guest_token: Option<String>,
    conn_type: nostr_bot::ConnectionType,
}

async fn send_request(url: &str, info: &ConnectionInfo) -> Result<String, std::io::Error> {
    log::debug!("Running send_request with bearer >{:?}<", info.bearer);
    let req = isahc::Request::get(url);

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

pub async fn get_info(
    conn_type: nostr_bot::ConnectionType,
) -> Result<ConnectionInfo, std::io::Error> {
    let dummy_info = ConnectionInfo {
        bearer: None,
        guest_token: None,
        conn_type: nostr_bot::ConnectionType::Direct,
    };

    let mut text = send_request("https://twitter.com/home?precache=1", &dummy_info).await?;

    let d = select::document::Document::from(text.as_str());
    let js = d
        .find(select::predicate::Name("script"))
        .filter_map(|n| n.attr("src"))
        .filter(|x| x.contains("/main"))
        .collect::<Vec<_>>();

    let js_with_bearer = js[0];
    let re = regex::Regex::new(r#""gt=(\d{19})"#).unwrap();

    let guest_token = match re.captures_iter(&text).next() {
        Some(gt) => match gt.get(1) {
            Some(gt) => gt.as_str(),
            None => panic!(),
        },
        None => panic!(),
    };
    println!("guest_token: {:?}", guest_token);

    let mut text = send_request(js_with_bearer, &dummy_info).await?;

    // grep -E ",[a-z]=\"[^\"]*\",[a-z]=\"[0-9]{8}
    let re = regex::Regex::new(r#",[a-zA-Z]="([^"]*)",[a-zA-Z]="\d{8}"#).unwrap();
    let bearer = match re.captures_iter(&text).last() {
        Some(gt) => match gt.get(1) {
            Some(gt) => gt.as_str(),
            None => panic!(),
        },
        None => panic!(),
    };
    println!("bearer: {}", bearer);

    Ok(ConnectionInfo {
        bearer: Some(format!("Bearer {}", bearer)),
        guest_token: Some(guest_token.to_string()),
        conn_type,
    })
}

async fn tweet_request(
    username: &str,
    since_timestamp: u64,
    until_timestamp: u64,
    cursor: &str,
    info: &ConnectionInfo,
) -> Result<String, std::io::Error> {
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
        .map(|x| format!("{}={}", x.0.to_string(), x.1.to_string()))
        .collect::<Vec<_>>();
    let x = params.join("&");

    let complete = format!("{}?{}", url, x);
    send_request(&complete, &info).await
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

pub async fn user_exists(username: &str, info: &ConnectionInfo) -> Result<bool, std::io::Error> {
    let now = crate::utils::unix_timestamp();
    let response = tweet_request(username, now, now, "-1", info).await?;
    let js: serde_json::Value = serde_json::from_str(&response).unwrap();
    if js["globalObjects"]["users"].as_object().unwrap().is_empty() {
        log::warn!("Returning false, js: {:?}", js);
        return Ok(false);
    }

    log::warn!("Returning true");
    return Ok(true);
}

const DATE_FORMAT_STR: &str = "%Y-%m-%d %H:%M:%S %z";
pub async fn get_tweets(
    username: &str,
    since_timestamp: u64,
    until_timestamp: u64,
    info: &ConnectionInfo,
) -> Result<Vec<crate::twitter::Tweet>, std::io::Error> {
    let mut all_tweets = vec![];
    let mut cursor = "-1".to_string();
    for i in 0..10 {
        let response =
            tweet_request(username, since_timestamp, until_timestamp, &cursor, info).await?;
        // println!("response {}", response);

        let js: serde_json::Value = serde_json::from_str(&response).unwrap();

        cursor = get_cursor(&js);

        let mut new_tweets = parse_tweets(username, js);
        if new_tweets.is_empty() {
            break;
        }

        all_tweets.append(&mut new_tweets)
    }

    all_tweets.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap());
    Ok(all_tweets
        .into_iter()
        .filter(|t| t.timestamp >= since_timestamp)
        .collect::<Vec<_>>())
}

fn get_cursor(js: &serde_json::Value) -> String {
    let entries = js["timeline"]["instructions"][0]["addEntries"]["entries"]
        .as_array()
        .unwrap(); //[-1]["content"]["operation"]["cursor"]["value"];

    let entry = match entries.into_iter().last() {
        Some(entry) => entry.as_object(),
        None => return "".to_string(),
    };

    match entry {
        Some(entry) => match entry["content"]["operation"]["cursor"]["value"].as_str() {
            Some(cursor) => cursor.to_string(),
            None => js["timeline"]["instructions"]
                .as_array()
                .unwrap()
                .into_iter()
                .last()
                .unwrap()["replaceEntry"]["entry"]["content"]["operation"]["cursor"]["value"]
                .as_str()
                .unwrap()
                .to_string(),
        },
        None => panic!("You should not be here"),
    }
}

pub async fn get_pic_url(username: &str, info: &ConnectionInfo) -> Result<String, String> {
    let profile_url = format!("https://api.twitter.com/graphql/jMaTS-_Ea8vh9rpKggJbCQ/UserByScreenName?variables=%7B%22screen_name%22%3A%20%22{}%22%2C%20%22withHighlightedLabel%22%3A%20false%7D", username);

    let mut response = send_request(&profile_url, info)
        .await
        .map_err(|e| "Unable to connect to Twitter.".to_string())?;

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
