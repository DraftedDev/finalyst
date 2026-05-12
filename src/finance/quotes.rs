use rig_core::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use yahoo_finance_api::Quote;

use crate::finance::{FinanceError, build_date, yahoo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotesTool;

impl Tool for QuotesTool {
    const NAME: &'static str = "quotes";

    type Error = FinanceError;

    type Args = QuotesArgs;

    type Output = Vec<Quote>;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "quotes".to_string(),
            description: "Get stock quotes for a given symbol".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
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
                "required": ["symbol"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::debug!("Calling QuotesTool with {args:?}");
        let yahoo = yahoo();

        let resp = yahoo
            .get_quote_history(
                &args.symbol,
                build_date(args.start_year, args.start_ordinal, args.start_hours)?,
                build_date(args.end_year, args.end_ordinal, args.end_hours)?,
            )
            .await
            .map_err(|err| err.to_string())?;

        Ok(resp.quotes().map_err(|err| err.to_string())?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotesArgs {
    symbol: String,
    start_year: i32,
    start_ordinal: u16,
    start_hours: u8,
    end_year: i32,
    end_ordinal: u16,
    end_hours: u8,
}
