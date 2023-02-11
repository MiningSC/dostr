use log::debug;

use crate::twitter_api;
use crate::utils;

pub struct Tweet {
    pub timestamp: u64,
    pub tweet: String,
    pub link: String,
}

pub fn get_tweet_event(tweet: &Tweet) -> nostr_bot::EventNonSigned {
    let formatted = format!("{} (source: {})", tweet.tweet, tweet.link);

    nostr_bot::EventNonSigned {
        created_at: utils::unix_timestamp(),
        kind: 1,
        tags: vec![vec![
            "tweet_timestamp".to_string(),
            format!("{}", tweet.timestamp),
        ]],
        content: formatted,
    }
}

pub async fn follow_links(tweets: &mut Vec<Tweet>, info: twitter_api::TwitterInfo) {
    let finder = linkify::LinkFinder::new();

    for tweet in tweets {
        let text = &tweet.tweet;
        let links: Vec<_> = finder.links(text).collect();

        let mut curr_pos = 0;
        let mut final_tweet = String::new();

        for link in &links {
            let start = link.start();
            let end = link.end();

            // TODO: Use only one crate for http request. Currently, reqwest is returning 404 for
            // twitter.com and isohc doesn't seem to support returning final url after redirects
            //
            let client = match info.lock().unwrap().conn_type {
                nostr_bot::ConnectionType::Direct => reqwest::ClientBuilder::new(),
                nostr_bot::ConnectionType::Socks5 => reqwest::ClientBuilder::new()
                    .proxy(reqwest::Proxy::all("socks5h://127.0.0.1:9050").unwrap()),
            };
            let request = client.build().unwrap().get(link.as_str());

            let final_url = match request.send().await {
                Ok(response) => response.url().as_str().to_string(),
                Err(e) => {
                    debug!(
                        "Failed to follow link >{}< ({}), using orignal url",
                        link.as_str().to_string(),
                        e
                    );
                    link.as_str().to_string()
                }
            };

            final_tweet.push_str(&text[curr_pos..start]);
            final_tweet.push_str(&final_url);
            curr_pos = end;
        }

        final_tweet.push_str(&text[curr_pos..]);

        debug!(
            "follow_links: orig. tweet >{}<, final tweet >{}<",
            text, final_tweet
        );
        tweet.tweet = final_tweet;
    }
}
