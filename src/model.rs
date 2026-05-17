use std::time::Duration;

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
use tracing_indicatif::span_ext::IndicatifSpanExt;

use crate::{
    config::Config,
    finance::quotes::QuotesTool,
    rss::{self, FeedEntry},
    utils::{join_chunked, with_progress},
};

type OllamaEmbeddingModel = ollama::EmbeddingModel;
type OllamaCompletionModel = ollama::CompletionModel;

const ENTRY_DB_CACHE_SIZE: usize = 1024 * 1024 * 5; // 5 MiB
const ENTRY_DB_FILE: &str = "entry.db";
const QDRANT_COLLECTION: &str = "embed-entries";
const ENTRY_TABLE: TableDefinition<'static, String, FeedEntry> = TableDefinition::new("entries");

pub struct Model {
    config: Config,
    entry_db: Database,
    embedding: OllamaEmbeddingModel,
    vector_db: Qdrant,
    vector_store: QdrantVectorStore<OllamaEmbeddingModel>,
    rank_agent: Agent<OllamaCompletionModel>,
    simplify_agent: Agent<OllamaCompletionModel>,
    analyst_agent: Agent<OllamaCompletionModel>,
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
            custom_headers: Default::default(),
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
            .tool(QuotesTool)
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

    pub async fn collect(&self, limit: usize) {
        let mut entries = self.fetch().await;

        // TODO: don't use write here
        let write_tx = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry write");
        let table = write_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        tracing::info!("Deduplicating...");
        entries.retain(|e| !table.get(&e.title).expect("Failed to get entry").is_some());

        drop(table);
        write_tx.commit().expect("Failed to commit entry write");

        // Sort entries by timestamp in descending order
        tracing::info!("Sorting entries...");
        entries.sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));

        tracing::info!("Truncating entries...");
        entries.truncate(limit);

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
                        self.process(idx, e).await;

                        span.pb_inc(1);
                    }
                },
            )
            .await
        })
        .await;
    }

    #[tracing::instrument(skip(self, entry))]
    async fn process(&self, i: usize, mut entry: FeedEntry) {
        tracing::debug!("Processing entry: {entry:?}");

        tracing::info!("Ranking entry...");
        entry.rank = self.rank_entry(&entry).await;
        tracing::info!("Ranked entry with rank {}", entry.rank);

        if entry.rank == 0 {
            tracing::info!("Skipping irrelevant entry");
            tracing::debug!("Irrelevant entry: {}", entry.title);
            return;
        }

        tracing::info!("Simplifying entry...");
        entry.content = self.simplify_entry(&entry).await;
        tracing::debug!("Simplified entry with final content: {}", entry.content);

        let write_tx = self
            .entry_db
            .begin_write()
            .expect("Failed to begin entry write");
        let mut table = write_tx
            .open_table(ENTRY_TABLE)
            .expect("Failed to open entry table");

        tracing::info!("Embedding entry...");
        let docs = EmbeddingsBuilder::new(self.embedding.clone())
            .document(entry.clone())
            .expect("Failed to embed entry document")
            .build()
            .await
            .expect("Failed to build embedding");

        tracing::info!("Updating vector store...");
        self.vector_store
            .insert_documents(docs)
            .await
            .expect("Failed to insert embedded document");

        tracing::info!("Updating entry database...");
        table
            .insert(entry.title.clone(), entry)
            .expect("Failed to insert entry into database");
    }

    async fn fetch(&self) -> Vec<FeedEntry> {
        let mut entries = Vec::with_capacity(self.config.sources.len());

        for source in &self.config.sources {
            tracing::info!("Fetching RSS feed from '{}'...", source);
            entries.extend(rss::fetch(source).await);
        }

        entries
    }

    async fn rank_entry(&self, entry: &FeedEntry) -> u8 {
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
            tracing::error!("Ranked entry content: '{}'", entry.content);
            1
        })
    }

    async fn simplify_entry(&self, entry: &FeedEntry) -> String {
        self.simplify_agent
            .prompt(&entry.content)
            .await
            .expect("Failed to simplify entry")
    }

    pub async fn analyze(&self) -> String {
        crate::finance::try_init();

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

        tracing::info!("Collecting entries...");
        let mut entries = table
            .iter()
            .expect("Failed to get entry keys")
            .map(|res| res.expect("Failed to get entry"))
            .collect::<Vec<_>>();

        tracing::info!("Sorting entries...");
        entries
            .sort_unstable_by(|a, b| b.1.value().timestamp_unix.cmp(&a.1.value().timestamp_unix));
        entries.truncate(self.config.use_latest_entries);

        if entries.len() < self.config.use_latest_entries {
            tracing::warn!(
                "Only {} entries available, expected {}",
                entries.len(),
                self.config.use_latest_entries
            );
        }

        let entries = entries.into_iter().map(|e| e.1.value()).collect::<Vec<_>>();
        let feed = entries
            .into_iter()
            .map(|e| e.display(true))
            .collect::<Vec<String>>()
            .join("\n\n");

        // Drop database handles for performance
        drop((table, read_tx));

        tracing::info!("Running analyst agent...");

        let result = with_progress("Analyzing", 0, |_| async {
            self.analyst_agent
                .prompt(format!("The latest RSS feed entries are:\n\n{}", feed))
                .await
                .expect("Failed to prompt agent")
        })
        .await;

        tracing::info!("Finished analyzing.");

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
