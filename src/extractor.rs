use std::sync::{Arc, Mutex};

use rig_core::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use yahoo_finance_api::time::UtcDateTime;

#[derive(Clone)]
pub struct SubmitTool {
    out: Arc<Mutex<Vec<MarketExtraction>>>,
}

impl SubmitTool {
    pub fn new() -> Self {
        Self {
            out: Arc::new(Mutex::new(Vec::with_capacity(2))),
        }
    }

    pub fn flush(&self) -> Vec<MarketExtraction> {
        let mut out = self.out.lock().expect("Failed to lock mutex");
        let result = out.drain(..).collect::<Vec<_>>();

        result
    }
}

impl Tool for SubmitTool {
    const NAME: &'static str = "submit_tool";

    type Error = serde_json::Error;

    type Args = MarketExtraction;

    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "Submit Tool".to_string(),
            description: "Submits a model extraction. May be called multiple times.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "ticker": {
                        "type": "string",
                        "description": "The ticker symbol of the stock."
                    },
                    "action": {
                        "type": "string",
                        "description": "The market action to take (either 'long', 'short', or 'neutral')."
                    },
                    "start_time": {
                        "type": "string",
                        "description": "The start time of the market action in YYYY-MM-DD format."
                    },
                    "end_time": {
                        "type": "string",
                        "description": "The end time of the market action in YYYY-MM-DD format."
                    },
                    "confidence": {
                        "type": "number",
                        "description": "The confidence level of the model extraction from 0 to 100."
                    }
                },
                "required": ["ticker", "action", "start_time", "end_time", "confidence"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.out.lock().expect("Failed to lock mutex").push(args);

        Ok("Extraction successfully submitted.".to_string())
    }
}

#[derive(Clone, Debug)]
pub enum MarketAction {
    Long,
    Short,
    Neutral,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketExtraction {
    pub ticker: String,
    #[serde(with = "market_action")]
    pub action: MarketAction,
    #[serde(with = "date_format")]
    pub start_time: UtcDateTime,
    #[serde(with = "date_format")]
    pub end_time: UtcDateTime,
    pub confidence: f32,
}

mod market_action {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    use crate::extractor::MarketAction;

    pub fn serialize<S>(action: &MarketAction, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match action {
            MarketAction::Long => "long",
            MarketAction::Short => "short",
            MarketAction::Neutral => "neutral",
        };
        serializer.serialize_str(s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<MarketAction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        match s.as_str() {
            "long" => Ok(MarketAction::Long),
            "short" => Ok(MarketAction::Short),
            "neutral" => Ok(MarketAction::Neutral),
            _ => Err(Error::custom(format!("Invalid market action: {}", s))),
        }
    }
}

mod date_format {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    use time::{UtcDateTime, format_description::BorrowedFormatItem, macros::format_description};

    const FORMAT: &[BorrowedFormatItem<'static>] = format_description!("[year]-[month]-[day]");

    pub fn serialize<S>(date: &UtcDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(&FORMAT).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<UtcDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;

        Ok(UtcDateTime::parse(&s, &FORMAT)
            .map_err(|err| Error::custom(format!("Invalid date: {err}")))?)
    }
}
