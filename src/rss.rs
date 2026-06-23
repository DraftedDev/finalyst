use std::sync::LazyLock;

use reqwest::Client;
use serde::{Deserialize, Serialize};

static CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

pub async fn fetch(source: &str) -> Vec<FeedEntry> {
    let mut entries = Vec::new();

    let response = CLIENT
        .get(source)
        .send()
        .await
        .expect("Failed to send request")
        .bytes()
        .await
        .expect("Failed to get response body");

    let feed = feed_rs::parser::parse(response.as_ref()).expect("Failed to parse feed");

    for entry in feed.entries {
        let title = entry.title.expect("No title specified").content;
        let timestamp = entry
            .updated
            .unwrap_or_else(|| entry.published.expect("No published tag"));
        let content = entry
            .content
            .map(|c| c.body.expect("Failed to get content body"))
            .unwrap_or_else(|| {
                entry
                    .summary
                    .map(|txt| txt.content)
                    .expect("No content or summary tag")
            });

        if title.is_empty() || content.is_empty() {
            tracing::warn!(
                "Skipping entry with empty title/content!\nTitle: '{}'\nContent: '{}'",
                title,
                content
            );
            continue;
        }

        entries.push(FeedEntry {
            title,
            content,
            timestamp: timestamp.to_rfc3339(),
            timestamp_unix: timestamp.timestamp() as u64,
        });
    }

    entries
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedEntry {
    pub title: String,
    pub content: String,
    pub timestamp: String,
    pub timestamp_unix: u64,
}
