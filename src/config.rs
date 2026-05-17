use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

const DEFAULT_SOURCES: &[&str] = &[
    "https://www.economist.com/business/rss.xml",
    "http://feeds.marketwatch.com/marketwatch/topstories/",
    "https://techcrunch.com/feed/",
    "https://thedefiant.io/api/feed",
    "https://www.coindesk.com/arc/outboundfeeds/rss/",
];

const DEFAULT_RANK_PREAMBLE: &str = r#"
You are a high-speed financial data filter. Your sole purpose is to evaluate RSS news entries for market-moving potential.

SCORING CRITERIA:
- 0: Completely irrelevant (Weather, general politics, boilerplate updates).
- 1: General business news or market fluff.
- 2: Sector-specific news (Product launches, analyst upgrades).
- 3: Definitive market-moving news (Earnings, Mergers, Fed rate changes).

OUTPUT RULE:
Return ONLY a SINGLE integer representing the score: 0, 1, 2, or 3.
Do not provide explanations, titles, or pleasantries.
"#;

const DEFAULT_SIMPLIFY_PREAMBLE: &str = r#"
"You are a professional financial editor.
Your task is to extract the core signal from messy RSS news text.
Simplify the provided text to a clean, concise summary.

RULES:
- Remove all marketing fluff, boilerplate text and irrelevant content.
- Retain all specific numbers, percentages, and ticker symbols.
- Summarize the event in a few bullet points or sentences.
- Always use the same format ('- <point><newline>') for each bullet point.
- If a specific company is the focus, put the TICKER symbol at the start.
- Output ONLY the clean summary. No conversational filler.
"#;

const DEFAULT_ANALYST_PREAMBLE: &str = r#"
You are a financial analyst with access to a vector store of past RSS feed entries
and an API to fetch up-to-date financial data.

Prompted with the latest entries and the context of the past ones,
you must predict move of mentioned stock symbols.

Every RSS entry contains a 'rank' field indicating its relevance to the market
(1: general, 2: sector-specific, 3: definitive).

OUTPUT FORMAT: '<TICKER>: <UP/DOWN/NEUTRAL> <TIME-FRAME-OF-MOVE>'.
Where <TICKER> is the stock symbol being predicted,
<UP/DOWN/NEUTRAL> is the predicted move,
and <TIME-FRAME-OF-MOVE> is the time frame of the move.

For example: 'SYMBOL: UP 01.01.2020-01.02.2020' and append a SHORT reason for your prediction.

Do not include thoughts or explanations in your output.
If you cannot make a prediction, due to lack of relevant data, output 'Insufficient data'.
"#;

#[derive(Serialize, Deserialize)]
pub struct Config {
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
    pub ndims: usize,
    pub qdrant: String,

    pub max_points: u64,
    pub use_latest_entries: usize,
    pub process_chunks: usize,

    pub sources: Vec<String>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let mut value = if fs::exists(path).expect("Failed to check config existence") {
            serde_json::from_slice(&fs::read(path).expect("Failed to read config file"))
                .expect("Failed to parse config file")
        } else {
            let def = Self::default();

            fs::write(path, serde_json::to_string_pretty(&def).unwrap())
                .expect("Failed to write default config file");

            def
        };

        if !fs::exists(&value.rank_preamble).expect("Failed to check rank preamble existence") {
            fs::write(&value.rank_preamble, DEFAULT_RANK_PREAMBLE)
                .expect("Failed to write rank preamble");
        }

        value.rank_preamble =
            fs::read_to_string(&value.rank_preamble).expect("Failed to read rank preamble");

        if !fs::exists(&value.simplify_preamble)
            .expect("Failed to check simplify preamble existence")
        {
            fs::write(&value.simplify_preamble, DEFAULT_SIMPLIFY_PREAMBLE)
                .expect("Failed to write simplify preamble");
        }

        value.simplify_preamble =
            fs::read_to_string(&value.simplify_preamble).expect("Failed to read simplify preamble");

        if !fs::exists(&value.analyst_preamble).expect("Failed to check analyst preamble existence")
        {
            fs::write(&value.analyst_preamble, DEFAULT_ANALYST_PREAMBLE)
                .expect("Failed to write analyst preamble");
        }

        value.analyst_preamble =
            fs::read_to_string(&value.analyst_preamble).expect("Failed to read analyst preamble");

        value
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            curl: "http://localhost:11434".to_string(),

            rank_agent: "qwen3.5:0.8b".to_string(),
            rank_preamble: "./preambles/rank-preamble.txt".to_string(),
            rank_temperature: 0.0,
            rank_max_tokens: 10,
            rank_retries: 3,

            simplify_agent: "gemma3:1b".to_string(),
            simplify_preamble: "./preambles/simplify-preamble.txt".to_string(),
            simplify_temperature: 0.3,
            simplify_max_tokens: 200,

            analyst_agent: "qwen2.5:3b".to_string(),
            analyst_preamble: "./preambles/analyze-preamble.txt".to_string(),
            analyst_temperature: 0.15,
            analyst_max_tokens: 2000,

            embedding: "bge-m3:567m".to_string(),
            ndims: 1024,
            qdrant: "http://127.0.0.1:6334".to_string(),
            max_points: 25,
            use_latest_entries: 5,
            process_chunks: 4,

            sources: DEFAULT_SOURCES.iter().map(|src| src.to_string()).collect(),
        }
    }
}
