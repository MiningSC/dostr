use log::{debug, info};


use crate::utils;
use crate::twitter_api;

const DATE_FORMAT_STR: &str = "%Y-%m-%d %H:%M:%S";

pub struct Tweet {
    pub timestamp: u64,
    pub tweet: String,
    pub link: String,
}

pub fn get_tweet_event(tweet: &Tweet) -> nostr_bot::EventNonSigned {
    let formatted = format!("{} ([source]({}))", tweet.tweet, tweet.link);

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


pub async fn follow_links(tweets: &mut Vec<Tweet>) {
    let finder = linkify::LinkFinder::new();

    for tweet in tweets {
        let text = &tweet.tweet;
        let links: Vec<_> = finder.links(text).collect();

        let mut curr_pos = 0;
        let mut final_tweet = String::new();

        for link in &links {
            let start = link.start();
            let end = link.end();

            let request = reqwest::get(link.as_str()).await;

            let final_url = match request {
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
