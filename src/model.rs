use std::{fs, path::Path, time::Duration};

use kdam::BarExt;
use qdrant_client::{
    Qdrant,
    config::QdrantConfig,
    qdrant::{CreateCollectionBuilder, QueryPointsBuilder, VectorParamsBuilder},
};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use rig_core::{
    agent::Agent,
    client::{CompletionClient, EmbeddingsClient},
    completion::Prompt,
    embeddings::EmbeddingsBuilder,
    providers::ollama::{self, Client},
    vector_store::InsertDocuments,
};
use rig_qdrant::QdrantVectorStore;
use serde::{Deserialize, Serialize};

use crate::{
    rss::{self, FeedEntry},
    utils::progress_bar,
};

type OllamaEmbeddingModel = ollama::EmbeddingModel;
type OllamaCompletionModel = ollama::CompletionModel;

const ENTRY_DB_CACHE_SIZE: usize = 1024 * 1024 * 5; // 5 MiB
const ENTRY_DB_FILE: &str = "entry.db";
const QDRANT_COLLECTION: &str = "embed-entries";
const ENTRY_TABLE: TableDefinition<'static, String, String> = TableDefinition::new("entries");
const DEFAULT_SOURCES: &[&str] = &[
    "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=20910258",
    "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=15839135",
    "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=10000664",
    "https://www.forbes.com/innovation/feed",
];

pub struct Model {
    config: ModelConfig,
    entry_db: Database,
    embedding: OllamaEmbeddingModel,
    vector_db: Qdrant,
    vector_store: QdrantVectorStore<OllamaEmbeddingModel>,
    rank_agent: Agent<OllamaCompletionModel>,
    simplify_agent: Agent<OllamaCompletionModel>,
    analyst_agent: Agent<OllamaCompletionModel>,
}

impl Model {
    pub async fn new(config: ModelConfig) -> Self {
        let client = Client::builder()
            .api_key("")
            .base_url(&config.curl)
            .build()
            .expect("Failed to build Ollama client");

        let entry_db = Database::builder()
            .set_cache_size(ENTRY_DB_CACHE_SIZE) // 5 MiB
            .create(ENTRY_DB_FILE)
            .expect("Failed to open Entry DB");

        let embedding = client.embedding_model_with_ndims(&config.embedding, config.ndims);

        let vector_db = Qdrant::new(QdrantConfig {
            uri: config.qdrant.clone(),
            timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(5),
            keep_alive_while_idle: true,
            api_key: None,
            compression: Some(qdrant_client::config::CompressionEncoding::Gzip),
            check_compatibility: true,
            pool_size: 1,
        })
        .expect("Failed to create Qdrant client");

        if !vector_db
            .collection_exists(QDRANT_COLLECTION)
            .await
            .expect("Failed to check collection existence")
        {
            vector_db
                .create_collection(
                    CreateCollectionBuilder::new(QDRANT_COLLECTION)
                        .vectors_config(VectorParamsBuilder::new(
                            config.ndims as u64,
                            qdrant_client::qdrant::Distance::Cosine,
                        ))
                        .build(),
                )
                .await
                .expect("Failed to create Qdrant collection");
        }

        let query_params = QueryPointsBuilder::new(QDRANT_COLLECTION)
            .limit(config.max_points)
            .with_payload(true)
            .with_vectors(true)
            .build();

        let vector_store =
            QdrantVectorStore::new(vector_db.clone(), embedding.clone(), query_params.clone());

        let rank_agent = client
            .agent(&config.rank_agent)
            .preamble(&config.rank_preamble)
            .temperature(config.rank_temperature)
            .max_tokens(config.rank_max_tokens)
            .build();

        let simplify_agent = client
            .agent(&config.simplify_agent)
            .preamble(&config.simplify_preamble)
            .temperature(config.simplify_temperature)
            .max_tokens(config.simplify_max_tokens)
            .build();

        let analyst_agent = client
            .agent(&config.analyst_agent)
            .preamble(&config.analyst_preamble)
            .temperature(config.analyst_temperature)
            .max_tokens(config.analyst_max_tokens)
            .dynamic_context(
                config.max_points as usize,
                QdrantVectorStore::new(vector_db.clone(), embedding.clone(), query_params.clone()),
            )
            .build();

        Self {
            config,
            entry_db,
            embedding,
            vector_db,
            vector_store,
            rank_agent,
            simplify_agent,
            analyst_agent,
        }
    }

    pub async fn fetch(&self, limit: Option<usize>) {
        let mut entries = Vec::with_capacity(self.config.sources.len());

        for source in &self.config.sources {
            tracing::info!("Fetching RSS feed from '{}'...", source);
            entries.extend(rss::fetch(source).await);
        }

        let write_tx = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry write");
        let mut table = write_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        tracing::info!("Deduplicating entries...");
        let old_len = entries.len();
        entries = entries
            .into_iter()
            .filter(|entry| {
                table
                    .get(&entry.title)
                    .expect("Failed to get entry from table")
                    .is_none()
            })
            .collect();

        if let Some(limit) = limit {
            tracing::info!("Truncating entries to {}...", limit);
            entries.truncate(limit);
        }

        tracing::info!(
            "Deduplicated entries. Using {}/{}...",
            entries.len(),
            old_len
        );

        tracing::info!("Ranking entries...");
        let mut progress = progress_bar(entries.len(), "Ranking");
        for entry in &mut entries {
            let rank = self.rank_entry(entry).await;
            entry.rank = rank;
            progress.update(1).expect("Failed to update progress bar");
        }
        eprintln!();

        tracing::info!("Filtering out irrelevant entries...");
        entries.retain(|e| e.rank > 0);

        tracing::info!("Simplifying entries...");
        let mut progress = progress_bar(entries.len(), "Simplifying");
        for entry in &mut entries {
            let simplified = self.simplify_entry(entry).await;
            entry.content = simplified;
            progress.update(1).expect("Failed to update progress bar");
        }
        eprintln!();

        tracing::info!(
            "Inserting entries {} into the entry database...",
            entries.len()
        );
        for entry in &entries {
            table
                .insert(entry.title.clone(), entry.display(true))
                .expect("Failed to insert entry");
        }
        drop(table);
        write_tx
            .commit()
            .expect("Failed to commit entry database transaction");

        let chunks = entries.chunks(self.config.embedding_chunks);

        tracing::info!(
            "Embedding {} chunks of {} entries...",
            chunks.len(),
            entries.len()
        );

        let mut progress = progress_bar(chunks.len(), "Embedding");
        for chunk in chunks {
            let embeddings = EmbeddingsBuilder::new(self.embedding.clone())
                .documents(chunk)
                .expect("Failed to embed entries")
                .build()
                .await
                .expect("Failed to build embeddings");

            self.vector_store
                .insert_documents(embeddings)
                .await
                .expect("Failed to insert ");

            progress.update(1).expect("Failed to update progress bar");
        }
        eprintln!();
    }

    pub async fn rank_entry(&self, entry: &FeedEntry) -> u8 {
        let mut rank = None;
        let mut retries = 0;

        while rank.is_none() && retries <= self.config.rank_retries {
            let resp = self
                .rank_agent
                .prompt(entry.display(false))
                .await
                .expect("Failed to prompt process AI");

            let rank_str = resp.trim().replace(|c: char| !c.is_numeric(), "");

            match rank_str.parse().ok() {
                Some(r) => {
                    if r > 3 {
                        tracing::warn!("Rank {} is out of bounds (0..3)", r);
                    } else {
                        rank = Some(r);
                    }
                }
                None => {
                    tracing::warn!(
                        "Failed to parse rank {}. Retrying ({}/{})...",
                        rank_str,
                        retries,
                        self.config.rank_retries,
                    );
                    retries += 1;
                }
            }
        }

        rank.unwrap_or_else(|| {
            tracing::error!("Ranking entry failed! Falling back to rank 1...");
            1
        })
    }

    pub async fn simplify_entry(&self, entry: &FeedEntry) -> String {
        self.simplify_agent
            .prompt(&entry.content)
            .await
            .expect("Failed to simplify entry")
    }

    pub async fn analyze(&self) -> String {
        let read_tx = self
            .entry_db
            .begin_read()
            .expect("Failed to begin entry read");

        if read_tx
            .list_tables()
            .expect("Failed to list tables")
            .count()
            == 0
        {
            panic!("No data in entry database. Please add some via `finalyst collect`!")
        }

        let table = read_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        tracing::info!("Collecting entries from database...");
        let mut entries = table
            .iter()
            .expect("Failed to get entry keys")
            .map(|res| res.expect("Failed to get entry"))
            .collect::<Vec<_>>();

        entries.sort_by(|a, b| b.1.value().cmp(&a.1.value()));
        entries.truncate(self.config.use_latest_entries);

        if entries.len() < self.config.use_latest_entries {
            tracing::warn!(
                "Only {} entries available, expected {}",
                entries.len(),
                self.config.use_latest_entries
            );
        }

        let entries = entries.into_iter().map(|e| e.1.value()).collect::<Vec<_>>();
        let feed = entries.join("\n\n");

        tracing::info!("Launching analyzer...");
        let result = self
            .analyst_agent
            .prompt(format!("The latest RSS feed entries are:\n\n{}", feed))
            .await
            .expect("Failed to prompt agent");

        result
    }

    pub async fn reset(&self) {
        if self
            .vector_db
            .collection_exists(QDRANT_COLLECTION)
            .await
            .expect("Failed to check collection existence")
        {
            self.vector_db
                .delete_collection(QDRANT_COLLECTION)
                .await
                .expect("Failed to delete Qdrant collection");
        }

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

#[derive(Serialize, Deserialize)]
pub struct ModelConfig {
    pub curl: String,

    pub rank_agent: String,
    pub rank_preamble: String,
    pub rank_temperature: f64,
    pub rank_max_tokens: u64,
    pub rank_retries: u64,

    pub simplify_agent: String,
    pub simplify_preamble: String,
    pub simplify_temperature: f64,
    pub simplify_max_tokens: u64,

    pub analyst_agent: String,
    pub analyst_preamble: String,
    pub analyst_temperature: f64,
    pub analyst_max_tokens: u64,

    pub embedding: String,
    pub embedding_chunks: usize,
    pub ndims: usize,
    pub qdrant: String,

    pub max_points: u64,
    pub use_latest_entries: usize,

    pub sources: Vec<String>,
}

impl ModelConfig {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if fs::exists(&path).expect("Failed to check config existence") {
            toml::from_slice(&fs::read(path).expect("Failed to read config file"))
                .expect("Failed to parse config file")
        } else {
            let def = Self::default();

            fs::write(path, toml::to_string(&def).unwrap())
                .expect("Failed to write default config file");

            def
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            curl: "http://localhost:11434".to_string(),

            rank_agent: "qwen2.5:1.5b".to_string(),
            rank_preamble: r#"
You are a high-speed financial data filter. Your sole purpose is to evaluate RSS news entries for market-moving potential.

SCORING CRITERIA:
- 0: Completely irrelevant (Weather, general politics, boilerplate updates).
- 1: General business news or market fluff.
- 2: Sector-specific news (Product launches, analyst upgrades).
- 3: Definitive market-moving news (Earnings, Mergers, Fed rate changes).

OUTPUT RULE:
Return ONLY a single integer between 0 and 3. Do not provide explanations, titles, or pleasantries.
                "#.to_string(),
            rank_temperature: 0.0,
            rank_max_tokens: 10,
            rank_retries: 3,

            simplify_agent: "phi4-mini:3.8b".to_string(),
            simplify_preamble: r#"
"You are a professional financial editor. Your task is to extract the core signal from messy RSS news text.
Simplify the provided text to a clean, concise summary.

RULES:
- Remove all marketing fluff, boilerplate text, and 'click for more' links.
- Retain all specific numbers, percentages, and ticker symbols.
- Summarize the event in 3-5 bullet points.
- If a specific company is the focus, put the TICKER symbol at the start.
- Output ONLY the clean summary. No conversational filler."
                "#.to_string(),
            simplify_temperature: 0.3,
            simplify_max_tokens: 200,

            analyst_agent: "deepseek-r1:14b".to_string(),
            analyst_preamble: r#"
You are a financial analyst with access to a vector store of past RSS feed entries.
Prompted with the latest entries and the context of the past ones,
you will predict moves of stock symbols mentioned and output a prediction,
that is either 'Up', 'Down', or 'Neutral'.
Every RSS entry contains a 'rank' field indicating its relevance to the market
(1: general, 2: sector-specific, 3: definitive).
If you cannot make a prediction, due to lack of relevant data, output 'Insufficient data'.
                "#
            .to_string(),
            analyst_temperature: 0.5,
            analyst_max_tokens: 1500,

            embedding: "bge-m3".to_string(),
            embedding_chunks: 10,
            ndims: 1024,
            qdrant: "http://127.0.0.1:6334".to_string(),
            max_points: 15,
            use_latest_entries: 5,
            sources: DEFAULT_SOURCES.iter().map(|src| src.to_string()).collect(),
        }
    }
}
