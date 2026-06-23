use std::time::SystemTime;

use redb::{Database, ReadableTable, TableDefinition};
use rig_core::{
    agent::Agent,
    client::CompletionClient,
    completion::Prompt,
    providers::ollama::{self, Client},
};
use tracing::Level;
use tracing_indicatif::span_ext::IndicatifSpanExt;

use crate::{
    config::Config,
    extractor::{MarketExtraction, SubmitTool},
    fetcher::FinanceFetcher,
    rss::{self, FeedEntry},
    utils::{join_chunked, with_progress},
};

const ENTRY_DB_CACHE_SIZE: usize = 1024 * 1024 * 5; // 5 MiB
const ENTRY_DB_FILE: &str = "entry.db";
const ENTRY_TABLE: TableDefinition<'static, String, ()> = TableDefinition::new("entries");

pub struct Model {
    config: Config,
    entry_db: Database,
    agent: Agent<ollama::CompletionModel>,
    submit: SubmitTool,
}

impl Model {
    pub async fn new(config: Config) -> Self {
        let client = Client::builder()
            .api_key("")
            .base_url(&config.curl)
            .build()
            .expect("Failed to build Ollama client");

        let entry_db = Database::builder()
            .set_cache_size(ENTRY_DB_CACHE_SIZE) // 5 MiB
            .create(ENTRY_DB_FILE)
            .expect("Failed to open Entry DB");

        let submit = SubmitTool::new();

        let agent = client
            .agent(&config.agent)
            .preamble(&config.preamble)
            .max_tokens(config.max_tokens)
            .temperature(config.temperature)
            .default_max_turns(16)
            .tools(vec![Box::new(FinanceFetcher), Box::new(submit.clone())])
            .build();

        Self {
            config,
            entry_db,
            agent,
            submit,
        }
    }

    pub async fn analyze(&self) -> Vec<ModelOutput> {
        let mut entries = self.fetch().await;

        let write_tx = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry write");
        let table = write_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        let old_entries = entries.len();
        tracing::info!("Removing outdated and duplicate entries ...");
        entries.retain(|e| {
            let now = SystemTime::UNIX_EPOCH
                .elapsed()
                .expect("Failed to get current time")
                .as_secs();

            e.timestamp_unix >= now - self.config.analyze_max_age
        });
        entries.retain(|e| table.get(&e.title).expect("Failed to get entry").is_none());

        tracing::info!("Using {}/{} relevant entries.", entries.len(), old_entries);
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp_unix));

        entries.truncate(self.config.analyze_max_entries);
        tracing::info!(
            "Using {}/{} maximum entries.",
            entries.len(),
            self.config.analyze_max_entries
        );

        drop(table);
        write_tx.commit().expect("Failed to commit entry write");

        tracing::info!(
            "Processing {} entries with {} chunks...",
            entries.len(),
            self.config.process_chunks,
        );

        with_progress("Processing", entries.len() as u64, |span| async move {
            join_chunked(
                entries.into_iter().enumerate(),
                self.config.process_chunks,
                |(idx, e)| {
                    let span = span.clone();
                    async move {
                        let result = self.process(idx, e.clone()).await;

                        tracing::debug!("Processed entry {}.", idx);
                        span.pb_inc(1);

                        result
                    }
                },
            )
            .await
        })
        .await
    }

    #[tracing::instrument(skip(self, entry))]
    async fn process(&self, i: usize, entry: FeedEntry) -> ModelOutput {
        if tracing::enabled!(Level::TRACE) {
            tracing::trace!("Processing entry: {entry:?}");
        } else if tracing::enabled!(Level::DEBUG) {
            tracing::debug!("Processing entry: {}", entry.title);
        }

        let write_tx = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry write");
        let mut table = write_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        tracing::info!("Updating entry database...");
        table
            .insert(entry.title.clone(), ())
            .expect("Failed to insert entry into database");

        drop(table);
        write_tx
            .commit()
            .expect("Failed to commit entry database write actions");

        let prompt = format!("{} {}:\n{}", entry.timestamp, entry.title, entry.content);

        let response = self
            .agent
            .prompt(prompt)
            .await
            .expect("Failed to prompt agent");

        ModelOutput {
            response,
            extractions: self.submit.flush(),
        }
    }

    async fn fetch(&self) -> Vec<FeedEntry> {
        let mut entries = Vec::with_capacity(self.config.sources.len());

        for source in &self.config.sources {
            tracing::info!("Fetching RSS feed from '{}'...", source);
            entries.extend(rss::fetch(source).await);
        }

        entries
    }

    pub async fn reset(&self) {
        // Deletes a table only if it exists
        let write = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry database write");
        write
            .delete_table(ENTRY_TABLE)
            .expect("Failed to delete entry table");
        write.commit().expect("Failed to commit entry changes");
    }
}

pub struct ModelOutput {
    pub response: String,
    pub extractions: Vec<MarketExtraction>,
}
