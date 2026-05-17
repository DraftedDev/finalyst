use std::sync::LazyLock;

use redb::TypeName;
use reqwest::Client;
use rig_core::Embed;
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
        let content = entry.summary.map(|t| t.content).unwrap_or_else(|| {
            entry
                .content
                .map(|c| c.body.expect("Failed to get content body"))
                .expect("No content given")
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
            links: entry.links.into_iter().map(|l| l.href).collect(),
            content,
            timestamp: timestamp.to_rfc3339(),
            timestamp_unix: timestamp.timestamp() as u64,
            rank: 0,
        });
    }

    entries
}

#[derive(Clone, Debug, PartialEq, Eq, Embed, Serialize, Deserialize)]
pub struct FeedEntry {
    pub title: String,
    pub links: Vec<String>,
    #[embed]
    pub content: String,
    pub rank: u8,
    pub timestamp: String,
    pub timestamp_unix: u64,
}

impl FeedEntry {
    pub fn display(&self, rank: bool) -> String {
        let result = format!(
            r#"Title: {}
Content: {}
Links: {}
Timestamp: {}"#,
            self.title,
            self.content,
            self.links.join(", "),
            self.timestamp
        );

        if rank {
            let rank_desc = match self.rank {
                0 => "irrelevant",
                1 => "general",
                2 => "sector-specific",
                3 => "definitive",
                _ => panic!("Invalid rank: {}", self.rank),
            };

            format!("Rank: {} ({})\n{}", self.rank, rank_desc, result)
        } else {
            result
        }
    }
}

impl redb::Value for FeedEntry {
    type SelfType<'a> = FeedEntry;

    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        serde_json::from_slice(data).expect("Failed to decode feed entry")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        serde_json::to_vec(value).expect("Failed to encode feed entry")
    }

    fn type_name() -> redb::TypeName {
        TypeName::new("FeedEntry")
    }
}
