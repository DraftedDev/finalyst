use std::{
    fs,
    path::{Path, PathBuf},
};

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
Return ONLY a single integer between 0 and 3. Do not provide explanations, titles, or pleasantries.
"#;

const DEFAULT_SIMPLIFY_PREAMBLE: &str = r#"
"You are a professional financial editor. Your task is to extract the core signal from messy RSS news text.
Simplify the provided text to a clean, concise summary.

RULES:
- Remove all marketing fluff, boilerplate text, and 'click for more' links.
- Retain all specific numbers, percentages, and ticker symbols.
- Summarize the event in a few bullet points or sentences.
- Always use the same format ('- <point><newline>') for each bullet point.
- If a specific company is the focus, put the TICKER symbol at the start.
- Output ONLY the clean summary. No conversational filler.
"#;

const DEFAULT_ANALYST_PREAMBLE: &str = r#"
You are a financial analyst with access to a vector store of past RSS feed entries.
Prompted with the latest entries and the context of the past ones,
and <TIME-FRAME> is the time period in '<day>/<month>/<year>'.
Every RSS entry contains a 'rank' field indicating its relevance to the market
(1: general, 2: sector-specific, 3: definitive).
You also have access to a 'finance-api' tool that provides you with up-to-date financial data
and can even calculate indicators like EMA and RSI.
Use the provided tools to make a prediction about the stock symbol's move.
IMPORTANT: Output in the format '<TICKER>: <UP/DOWN/NEUTRAL> <TIME-FRAME-OF-MOVE>'.
For example: 'SYMBOL: UP 01.01.20XX-01.02.20XX' and append a short reason for your prediction.
Do not include thoughts or explanations in your output.
If you cannot make a prediction, due to lack of relevant data, output 'Insufficient data'.
"#;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub curl: String,

    pub rank_agent: String,
    pub rank_preamble: PathBuf,
    pub rank_temperature: f64,
    pub rank_max_tokens: u64,
    pub rank_retries: u64,

    pub simplify_agent: String,
    pub simplify_preamble: PathBuf,
    pub simplify_temperature: f64,
    pub simplify_max_tokens: u64,

    pub analyst_agent: String,
    pub analyst_preamble: PathBuf,
    pub analyst_temperature: f64,
    pub analyst_max_tokens: u64,

    pub embedding: String,
    pub ndims: usize,
    pub qdrant: String,

    pub max_points: u64,
    pub use_latest_entries: usize,
    pub parallel_chunks: usize,

    pub sources: Vec<String>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if fs::exists(path).expect("Failed to check config existence") {
            serde_json::from_slice(&fs::read(path).expect("Failed to read config file"))
                .expect("Failed to parse config file")
        } else {
            let def = Self::default();

            fs::write(path, serde_json::to_string_pretty(&def).unwrap())
                .expect("Failed to write default config file");

            def
        }
    }

    pub fn read_rank_preamble(&self) -> String {
        fs::read_to_string(&self.rank_preamble)
            .expect("Failed to read rank preamble")
            .replace(|c: char| c == '\t' || c == '\n', " ")
            .trim()
            .to_string()
    }

    pub fn read_simplify_preamble(&self) -> String {
        fs::read_to_string(&self.simplify_preamble)
            .expect("Failed to read simplify preamble")
            .replace(|c: char| c == '\t' || c == '\n', " ")
            .trim()
            .to_string()
    }

    pub fn read_analyst_preamble(&self) -> String {
        fs::read_to_string(&self.analyst_preamble)
            .expect("Failed to read analyst preamble")
            .replace(|c: char| c == '\t' || c == '\n', " ")
            .trim()
            .to_string()
    }
}

impl Default for Config {
    fn default() -> Self {
        let rank_preamble = "rank-preamble.txt";
        let simplify_preamble = "simplify-preamble.txt";
        let analyst_preamble = "analyze-preamble.txt";

        fs::write(rank_preamble, DEFAULT_RANK_PREAMBLE).expect("Failed to write rank preamble");
        fs::write(simplify_preamble, DEFAULT_SIMPLIFY_PREAMBLE)
            .expect("Failed to write simplify preamble");
        fs::write(analyst_preamble, DEFAULT_ANALYST_PREAMBLE)
            .expect("Failed to write analyze preamble");

        Self {
            curl: "http://localhost:11434".to_string(),

            rank_agent: "qwen3.5:0.8b".to_string(),
            rank_preamble: Path::new(&rank_preamble).to_path_buf(),
            rank_temperature: 0.0,
            rank_max_tokens: 10,
            rank_retries: 3,

            simplify_agent: "gemma3:1b".to_string(),
            simplify_preamble: Path::new(&simplify_preamble).to_path_buf(),
            simplify_temperature: 0.3,
            simplify_max_tokens: 200,

            analyst_agent: "qwen2.5:3b".to_string(),
            analyst_preamble: Path::new(&analyst_preamble).to_path_buf(),
            analyst_temperature: 0.15,
            analyst_max_tokens: 2000,

            embedding: "bge-m3:567m".to_string(),
            ndims: 1024,
            qdrant: "http://127.0.0.1:6334".to_string(),
            max_points: 25,
            use_latest_entries: 5,
            parallel_chunks: 4,

            sources: DEFAULT_SOURCES.iter().map(|src| src.to_string()).collect(),
        }
    }
}
