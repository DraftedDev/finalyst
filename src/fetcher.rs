use std::{error::Error, fmt::Display};

use rig_core::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use yahoo_finance_api::{
    YahooConnectorBuilder,
    time::{Date, OffsetDateTime, Time},
};
use yata::{
    core::{Candle, IndicatorConfig, IndicatorInstance, IndicatorResult, Method},
    indicators::{MACD, RSI},
    methods::EMA,
};

pub fn build_date(year: i32, ordinal: u16, hours: u8) -> Result<OffsetDateTime, FinanceError> {
    Ok(OffsetDateTime::new_utc(
        Date::from_ordinal_date(year, ordinal).map_err(|err| err.to_string())?,
        Time::from_hms(hours, 0, 0).map_err(|err| err.to_string())?,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceFetcher;

impl Tool for FinanceFetcher {
    const NAME: &'static str = "finance_fetcher";

    type Error = FinanceError;

    type Args = FinanceArgs;

    type Output = FinanceData;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "quotes".to_string(),
            description: "Get important indicators (in text form) for a given stock symbol."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ticker": {
                        "type": "string",
                        "description": "The stock symbol to get quotes for"
                    },
                    "start_year": {
                        "type": "integer",
                        "description": "The start year to get quotes for"
                    },
                    "start_ordinal": {
                        "type": "integer",
                        "description": "The start ordinal days to get quotes for"
                    },
                    "start_hours": {
                        "type": "integer",
                        "description": "The start hours to get quotes for"
                    },
                    "end_year": {
                        "type": "integer",
                        "description": "The end year to get quotes for"
                    },
                    "end_ordinal": {
                        "type": "integer",
                        "description": "The end ordinal days to get quotes for"
                    },
                    "end_hours": {
                        "type": "integer",
                        "description": "The end hours to get quotes for"
                    }

                },
                "required": ["ticker", "interval", "start_year", "start_ordinal", "start_hours", "end_year", "end_ordinal", "end_hours"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::debug!("Calling FinanceFetcher with {args:?}");
        let yahoo = YahooConnectorBuilder::new()
            .build()
            .expect("Failed to build yahoo connector");

        let resp = yahoo
            .get_quote_history_interval(
                &args.ticker,
                build_date(args.start_year, args.start_ordinal, args.start_hours)?,
                build_date(args.end_year, args.end_ordinal, args.end_hours)?,
                "1h",
            )
            .await
            .map_err(|err| err.to_string())?;

        let candles = resp
            .quotes()
            .map_err(|err| err.to_string())?
            .into_iter()
            .map(|q| Candle {
                open: q.open,
                high: q.high,
                low: q.adjclose,
                close: q.close,
                volume: q.volume as f64,
            })
            .collect::<Vec<_>>();

        let macd = macd(&candles);
        let rsi = rsi(&candles);
        let price_vs_ema200 = price_vs_ema200(&candles);

        let data = FinanceData {
            macd,
            rsi,
            price_vs_ema200,
        };

        tracing::debug!("Fetched Finance Data: {data:?}");

        Ok(data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceData {
    macd: String,
    rsi: String,
    price_vs_ema200: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceArgs {
    ticker: String,
    start_year: i32,
    start_ordinal: u16,
    start_hours: u8,
    end_year: i32,
    end_ordinal: u16,
    end_hours: u8,
}

#[derive(Debug, Clone)]
pub struct FinanceError(String);

impl Display for FinanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for FinanceError {}

impl From<String> for FinanceError {
    fn from(value: String) -> Self {
        FinanceError(value)
    }
}

fn macd(historical_data: &[Candle]) -> String {
    if historical_data.is_empty() {
        return "insufficient_data".to_string();
    }

    let macd_config = MACD::default();
    let mut macd_engine = macd_config.init(&historical_data[0]).unwrap();

    let mut prev_histogram = 0.0f64;
    let mut current_result: Option<IndicatorResult> = None;

    for candle in historical_data.iter() {
        if let Some(res) = &current_result {
            // Save the old histogram value before calculating the new one
            prev_histogram = res.values()[0] - res.values()[1]; // MACD line - Signal line
        }
        current_result = Some(macd_engine.next(candle));
    }

    if let Some(res) = current_result {
        let macd_line = res.values()[0];
        let signal_line = res.values()[1];
        let histogram = macd_line - signal_line;

        let crossover = if macd_line > signal_line {
            "bullish"
        } else {
            "bearish"
        };

        let momentum = if histogram.abs() > prev_histogram.abs() {
            "accelerating"
        } else {
            "decelerating"
        };

        format!("{}_{}", crossover, momentum)
    } else {
        "insufficient_data".to_string()
    }
}

fn rsi(historical_data: &[Candle]) -> String {
    if historical_data.is_empty() {
        return "insufficient_data".to_string();
    }

    let rsi_config = RSI::default();
    let mut rsi_engine = rsi_config.init(&historical_data[0]).unwrap();

    let mut prev_rsi = 50.0f64;
    let mut current_result: Option<IndicatorResult> = None;

    for candle in historical_data.iter() {
        if let Some(res) = &current_result {
            prev_rsi = res.value(0);
        }

        let new_res = rsi_engine.next(candle);

        if current_result.is_none() {
            prev_rsi = new_res.value(0);
        }

        current_result = Some(new_res);
    }

    if let Some(res) = current_result {
        let current_rsi = res.value(0);

        let zone = if current_rsi >= 70.0 {
            "overbought"
        } else if current_rsi <= 30.0 {
            "oversold"
        } else if current_rsi > 50.0 {
            "neutral_bullish"
        } else {
            "neutral_bearish"
        };

        let trajectory = if current_rsi > prev_rsi {
            "rising"
        } else if current_rsi < prev_rsi {
            "falling"
        } else {
            "flat"
        };

        format!("{}_{}", zone, trajectory)
    } else {
        "insufficient_data".to_string()
    }
}

fn price_vs_ema200(historical_data: &[Candle]) -> String {
    if historical_data.len() < 200 {
        return "insufficient_data".to_string();
    }

    let first_close = historical_data[0].close;
    let mut ema_engine = EMA::new(200, &first_close).unwrap();

    let mut current_ema = first_close;
    let mut prev_ema = first_close;
    let mut prev_close = first_close;

    for candle in historical_data.iter() {
        prev_close = candle.close;
        prev_ema = current_ema;
        current_ema = ema_engine.next(&candle.close);
    }

    let latest_close = historical_data.last().unwrap().close;

    let distance_pct = (latest_close - current_ema) / current_ema;
    let is_testing_zone = distance_pct.abs() <= 0.005;

    if prev_close <= prev_ema && latest_close > current_ema {
        // Price crossed cleanly from underneath to above the 200-EMA
        "bullish_breakout_above_200ema".to_string()
    } else if prev_close >= prev_ema && latest_close < current_ema {
        // Price collapsed from above to underneath the 200-EMA
        "bearish_breakdown_below_200ema".to_string()
    } else if latest_close > current_ema {
        // Price is safely holding above the moving average line
        if is_testing_zone {
            "testing_200ema_as_support".to_string()
        } else {
            "above_200ema_bullish_trend".to_string()
        }
    } else {
        // Price is safely tracking underneath the moving average line
        if is_testing_zone {
            "testing_200ema_as_resistance".to_string()
        } else {
            "below_200ema_bearish_trend".to_string()
        }
    }
}
