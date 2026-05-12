use std::{error::Error, fmt::Display, sync::OnceLock};

use yahoo_finance_api::{
    YahooConnector, YahooConnectorBuilder,
    time::{Date, OffsetDateTime, Time},
};

pub mod quotes;

static CONNECTOR: OnceLock<YahooConnector> = OnceLock::new();

pub fn build_date(year: i32, ordinal: u16, hours: u8) -> Result<OffsetDateTime, FinanceError> {
    Ok(OffsetDateTime::new_utc(
        Date::from_ordinal_date(year, ordinal).map_err(|err| err.to_string())?,
        Time::from_hms(hours, 0, 0).map_err(|err| err.to_string())?,
    ))
}

pub fn yahoo<'a>() -> &'a YahooConnector {
    CONNECTOR.get().expect("Connector not initialized")
}

pub fn try_init() {
    let _ = CONNECTOR.set(
        YahooConnectorBuilder::new()
            .build()
            .expect("Failed to build yahoo connector"),
    );
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
