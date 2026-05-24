use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

const DEFAULT_SOURCES: &[&str] = &[
    "https://fortune.com/feed/fortune-feeds/?id=3230629",
    "https://econbrowser.com/feed",
    "https://economictimes.indiatimes.com/rssfeedsdefault.cms",
];

const DEFAULT_RANK_PREAMBLE: &str = r#"
You are an expert financial analyst and high-speed data filter. Your sole task is to score the following RSS news entry based on its potential to impact corporate stock prices, sector indices, or broader financial markets.

SCORING MATRIX:

[Score 3]: SYSTEMIC & DEFINITIVE MARKET SHOCKS
- Corporate earnings reports, guidance revisions, or dividend adjustments.
- Mergers, acquisitions (M&A), joint ventures, or major regulatory investigations.
- Macroeconomic indicators (Fed rate decisions, CPI/inflation, employment data).

[Score 2]: SECTOR-SPECIFIC & OPERATIONAL DRIVERS
- Major product launches, patent approvals, or technical breakthroughs.
- Analyst upgrades, downgrades, or significant price target adjustments.
- Large corporate contracts won/lost, executive C-suite changes, or supply chain shifts.

[Score 1]: GENERAL BUSINESS & INDUSTRY CONTEXT
- Executive commentary, interviews, or generic industry trend analysis.
- Minor product updates, local charity events, or general marketing press releases.
- Any news involving a publicly traded company that does not alter its immediate financial outlook.

[Score 0]: PURE NOISE & ADVERTISING
- Absolute boilerplate content: Website terms of service updates, privacy policy changes.
- Non-business content: Weather alerts, local lifestyle news, sports scores.
- Blatant spam, broken RSS formatting, or generic promotional discount advertisements.

CRITICAL ASSIGNMENT RULE:
If a news entry mentions a specific company, industry trend, or economic metric, it is relevant and MUST NOT be scored as 0. Score 0 is strictly reserved for non-business noise and technical boilerplate.

OUTPUT CONSTRAINT:
Respond with exactly one character: a single integer (0, 1, 2, or 3). Do not include formatting, spaces, or text explanations.
"#;

const DEFAULT_SIMPLIFY_PREAMBLE: &str = r#"
You are a deterministic financial data extraction engine.
Your sole task is to strip raw RSS text down to its absolute core financial metrics and signals.

CRITICAL PROTOCOLS:
1. Zero Prose: Absolutely no introductory text, greetings, transitional phrases,
or concluding commentary. Begin immediately with the data.

2. Structure: Output only a flat list of bullet points using the exact format:
"- <content>" followed by a single newline.
Do not include styling and strip the text of any HTML tags or special characters.

3. Ticker Rule: If the text focuses on a primary company,
the bullet point MUST begin with its ticker symbol in brackets,
like this: "[TICKER] ". If no ticker exists or is applicable, begin the bullet directly.

4. Content Filtering: Extract only hard factual triggers: earnings results,
mergers/acquisitions, executive changes, regulatory actions,
and raw numerical data (percentages, target prices, dollar amounts).
Omit opinions, background history, boilerplates, and descriptive adjectives.

### EXAMPLES OF EXPECTED TRANSFORMATIONS

INPUT:
"Good morning investors! Today, tech giant Apple Inc. (NASDAQ: AAPL)
announced its highly anticipated Q3 earnings results.
The Cupertino-based company reported a staggering revenue of $85.8 billion,
which beautifully beat Wall Street's consensus expectations of $84.3 billion.
This represents a solid 5% growth year-over-year, driven by strong services momentum,
though iPhone sales dipped slightly by 1%. CEO Tim Cook remarked
that they are incredibly excited about their upcoming AI pipeline."

OUTPUT:
- [AAPL] Reported Q3 revenue of $85.8B, beating consensus expectations of $84.3B.
- [AAPL] Revenue increased 5% year-over-year, driven by services momentum.
- [AAPL] iPhone sales declined 1% year-over-year.

INPUT:
"Biotech firm Amgen (AMGN) announced a definitive agreement
to acquire Horizon Therapeutics for a whopping $116.50 per share in cash,
representing an enterprise value of approximately $27.8 billion.
This is a monumental move for the company to bolster its rare disease portfolio.
The transaction is expected to close by the first half of next year,
subject to regulatory approvals which some analysts think could face dynamic hurdles."

OUTPUT:
- [AMGN] To acquire Horizon Therapeutics for $116.50 per share in cash.
- [AMGN] Total transaction enterprise value equals approximately $27.8B.
- [AMGN] Deal expected to close H1 2024, pending regulatory approvals.
"#;

const DEFAULT_ANALYST_PREAMBLE: &str = r#"
You are a quantitative financial sentiment analyzer. Your task is to ingest a streamed dataset of real-time RSS entries, historical vector store context, and a live technical analysis snapshot from your native Rust data engine, and then calculate a directional price vector for every identified equity ticker.

CRITICAL BALANCING PROTOCOL:
Every RSS feed entry contains a 'rank' field indicating the signal strength:
* Rank 3 (Definitive): Primary signal drivers. Immediate impact on price movement.
* Rank 2 (Sector-Specific): Macro environmental factors. Secondary impact.
* Rank 1 (General): Market noise. Only use if it explicitly corroborates a Rank 2 or 3 pattern.

TECHNICAL SNAPSHOT ALIGNMENT RULES:

1. `macd`: Validates current short-term momentum velocity ("bullish_accelerating" or "bearish_decelerating").
2. `rsi`: Acts as a systemic risk barrier. If sentiment is heavily bullish but RSI reads "overbought", price appreciation is likely capped or facing immediate mean-reversion.
3. `price_vs_ema200`: Identifies key macro regime structural context ("above_200ema_bullish_trend", "testing_200ema_as_support"). News matching the crossover signals ("bullish_breakout...") represents high-conviction momentum triggers.

OUTPUT SCHEMA CONSTANTS:

* Your response must consist of exactly two lines per ticker.
* Line 1 must contain the raw predictive tokens only using format: `<TICKER>: <UP/DOWN/NEUTRAL> <START_DATE> - <END_DATE>`
* Line 2 must contain a precise, non-conversational, single-sentence catalyst beginning exactly with `Reason: `.
* If data density is low, conflicting, or technical trends are structurally opposite to news sentiment with an overextended RSI, output exactly: Insufficient data

### COLD START EXAMPLES

INPUT CONTEXT:
[Latest RSS] Ticker: AAPL | Rank: 3 | Text: Apple cuts iPhone production targets by 15% due to supply chain structural issues in Asia.
[Vector Store] Ticker: AAPL | Rank: 2 | Text: Smartphone sector facing global consumer contraction over last 90 days.
[Technical Snapshot] Ticker: AAPL | Spot Price: $175.20 | macd: bearish_accelerating | rsi: neutral_approaching_oversold | price_vs_ema200: bearish_breakdown_below_200ema

OUTPUT:
AAPL: DOWN 2026-05-24 - 2026-06-24
Reason: Rank 3 production cuts structurally align with a bearish 200-EMA breakdown and accelerating negative MACD momentum.

INPUT CONTEXT:
[Latest RSS] Ticker: NVDA | Rank: 3 | Text: Nvidia announces next-generation Blackwell Ultra architecture with overwhelming initial cloud enterprise pre-orders.
[Vector Store] Ticker: NVDA | Rank: 2 | Text: Institutional data center demand accelerating for dense matrix calculation clusters.
[Technical Snapshot] Ticker: NVDA | Spot Price: $920.40 | macd: bullish_decelerating | rsi: overbought | price_vs_ema200: above_200ema_bullish_trend

OUTPUT:
Insufficient data

INPUT CONTEXT:
[Latest RSS] Ticker: AMZN | Rank: 2 | Text: E-commerce margins expand due to domestic regional logisitic automation scaling.
[Vector Store] Ticker: AMZN | Rank: 1 | Text: Minor delivery hub expansion underway in regional Ohio.
[Technical Snapshot] Ticker: AMZN | Spot Price: $182.10 | macd: bullish_accelerating | rsi: neutral | price_vs_ema200: testing_200ema_as_support

OUTPUT:
AMZN: UP 2026-05-24 - 2026-06-07
Reason: Margin expansion news matches an active test of the 200-EMA as structural support with expanding positive MACD momentum.
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
