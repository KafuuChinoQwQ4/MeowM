use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::{CryptoRecord, valid_asset, valid_decimal, valid_external_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct Ticker {
    pub instrument: String,
    pub last: String,
    pub open_24h: String,
    pub high_24h: String,
    pub low_24h: String,
    pub volume_24h: String,
    pub timestamp: i64,
}
#[derive(Debug, Serialize)]
pub(crate) struct Candle {
    pub timestamp: i64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub confirmed: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct Balance {
    pub account: &'static str,
    pub asset: String,
    pub quantity: Option<String>,
    pub available: Option<String>,
    pub frozen: Option<String>,
    pub equity: Option<String>,
    pub equity_usd: Option<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Valuation {
    pub total_usd: String,
    pub funding_usd: String,
    pub trading_usd: String,
    pub earn_usd: Option<String>,
    pub timestamp: i64,
}
#[derive(Debug, Serialize)]
pub(crate) struct AssetOverview {
    pub balances: Vec<Balance>,
    pub valuation: Valuation,
    pub fetched_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistorySource {
    Bills,
    BillsArchive,
    Fills,
    Deposits,
    Withdrawals,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OkxEnvironment {
    Demo,
    Production,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct OrderAcknowledgement {
    /// Exchange order identifier assigned after request acceptance.
    #[serde(rename = "ordId")]
    pub order_id: String,
    #[serde(rename = "clOrdId")]
    pub client_order_id: String,
    /// Per-order request result code; zero means accepted, not filled.
    #[serde(rename = "sCode")]
    pub code: String,
    #[serde(rename = "sMsg")]
    pub message: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryRequest {
    pub source: HistorySource,
    pub begin: i64,
    pub end: i64,
    pub after: Option<String>,
    #[serde(default)]
    pub account_uid: Option<String>,
}
impl HistoryRequest {
    pub fn validate(&self, now: chrono::DateTime<chrono::Utc>) -> Result<(), ProviderErrorCode> {
        let oldest = if self.source == HistorySource::Bills {
            now - chrono::Duration::days(7)
        } else {
            now.checked_sub_months(chrono::Months::new(3))
                .ok_or(ProviderErrorCode::InvalidRequest)?
        };
        // A small tolerance avoids rejecting a valid form while the request is in flight.
        if self.begin <= 0
            || self.begin < oldest.timestamp_millis() - 60_000
            || self.end < self.begin
            || self.end > now.timestamp_millis() + 60_000
            || self.after.as_deref().is_some_and(|v| !valid_external_id(v))
            || self
                .account_uid
                .as_deref()
                .is_some_and(|v| !valid_external_id(v))
            || (self.after.is_some() && self.account_uid.is_none())
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct HistoryPage {
    pub records: Vec<CryptoRecord>,
    pub next_after: Option<String>,
    pub fetched: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Position {
    pub position_id: String,
    pub instrument: String,
    pub side: String,
    pub margin_mode: String,
    pub currency: String,
    pub quantity: Option<String>,
    pub open_price: Option<String>,
    pub close_price: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub realized_pnl: Option<String>,
    pub pnl: Option<String>,
    pub fee: Option<String>,
    pub funding_fee: Option<String>,
    pub liquidation_penalty: Option<String>,
    pub updated_at: i64,
}
#[derive(Debug, Serialize)]
pub(crate) struct PositionOverview {
    pub current: Vec<Position>,
    pub history: Vec<Position>,
    pub next_after: Option<String>,
    pub fetched_at: i64,
}

pub(super) fn text(row: &Value, key: &str) -> Result<String, ProviderErrorCode> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 128 && !s.chars().any(char::is_control))
        .map(String::from)
        .ok_or(ProviderErrorCode::InvalidResponse)
}
pub(super) fn decimal(row: &Value, key: &str) -> Result<String, ProviderErrorCode> {
    let value = text(row, key)?;
    if !valid_decimal(&value) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(value)
}
pub(super) fn optional_decimal(
    row: &Value,
    key: &str,
) -> Result<Option<String>, ProviderErrorCode> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) if v.is_empty() => Ok(None),
        _ => decimal(row, key).map(Some),
    }
}
pub(super) fn asset(row: &Value, key: &str) -> Result<String, ProviderErrorCode> {
    let value = text(row, key)?;
    if !valid_asset(&value) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(value)
}
pub(super) fn id(row: &Value, key: &str) -> Result<String, ProviderErrorCode> {
    let value = text(row, key)?;
    if !valid_external_id(&value) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(value)
}
pub(super) fn timestamp(row: &Value, key: &str) -> Result<i64, ProviderErrorCode> {
    let value = id(row, key)?
        .parse::<i64>()
        .map_err(|_| ProviderErrorCode::InvalidResponse)?;
    if value <= 0 || chrono::DateTime::from_timestamp_millis(value).is_none() {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(value)
}
