use chrono::{DateTime, Datelike, NaiveDate, Utc};
use ledger_storage::sqlite::{ExchangeRateObservation, SUPPORTED_EXCHANGE_RATE_QUOTES};
use serde::Deserialize;
use std::{collections::BTreeMap, io::Read, time::Duration};

const API_BASE: &str = "https://api.frankfurter.dev/v2";
pub(crate) const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateProviderError {
    Network,
    Timeout,
    Provider,
    InvalidData,
}
impl RateProviderError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Network => "network_error",
            Self::Timeout => "timeout",
            Self::Provider => "provider_error",
            Self::InvalidData => "invalid_data",
        }
    }
}

pub(crate) trait RateProvider {
    fn fetch(
        &self,
        from: Option<NaiveDate>,
        through: NaiveDate,
    ) -> Result<Vec<ExchangeRateObservation>, RateProviderError>;
}

pub(crate) struct HttpRateProvider {
    agent: ureq::Agent,
    base_url: String,
}
impl Default for HttpRateProvider {
    fn default() -> Self {
        Self::new(
            ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .proxy(preferred_proxy())
                .max_redirects(0)
                .build()
                .into(),
            API_BASE.into(),
        )
    }
}
fn preferred_proxy() -> Option<ureq::Proxy> {
    ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .and_then(|value| ureq::Proxy::new(&value).ok())
        })
        .or_else(ureq::Proxy::try_from_env)
}
impl HttpRateProvider {
    pub(crate) fn new(agent: ureq::Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }
    fn request(&self, suffix: &str) -> Result<Vec<u8>, RateProviderError> {
        let url = format!(
            "{}/rates?base=CNY&quotes={}&{suffix}",
            self.base_url,
            SUPPORTED_EXCHANGE_RATE_QUOTES.join(",")
        );
        let mut response = self.agent.get(&url).call().map_err(classify_error)?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    RateProviderError::Timeout
                } else {
                    RateProviderError::Network
                }
            })?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(RateProviderError::InvalidData);
        }
        Ok(bytes)
    }
}
impl RateProvider for HttpRateProvider {
    fn fetch(
        &self,
        from: Option<NaiveDate>,
        through: NaiveDate,
    ) -> Result<Vec<ExchangeRateObservation>, RateProviderError> {
        let fetched_at = Utc::now();
        let mut merged = BTreeMap::new();
        if let Some(from) = from {
            let body = self.request(&format!("from={from}&to={through}"))?;
            for row in parse_rates(&body, fetched_at)? {
                if row.effective_date < from || row.effective_date > through {
                    return Err(RateProviderError::InvalidData);
                }
                merged.insert((row.effective_date, row.quote_currency.clone()), row);
            }
        }
        // Expanded provider records are large; only the newest actual observation needs attribution.
        if let Some(latest_date) = merged.keys().map(|(date, _)| *date).max() {
            let latest = self.request(&format!("date={latest_date}&expand=providers"))?;
            for row in parse_rates(&latest, fetched_at)? {
                if row.effective_date != latest_date {
                    return Err(RateProviderError::InvalidData);
                }
                if from.is_none_or(|start| row.effective_date >= start) {
                    merged.insert((row.effective_date, row.quote_currency.clone()), row);
                }
            }
        }
        Ok(merged.into_values().collect())
    }
}

#[derive(Deserialize)]
struct RateRow {
    date: String,
    base: String,
    quote: String,
    rate: serde_json::Number,
    #[serde(default)]
    providers: Vec<ProviderRow>,
}
#[derive(Deserialize)]
struct ProviderRow {
    key: String,
    date: String,
    rate: serde_json::Number,
}
pub(crate) fn parse_rates(
    body: &[u8],
    fetched_at: DateTime<Utc>,
) -> Result<Vec<ExchangeRateObservation>, RateProviderError> {
    let rows: Vec<RateRow> =
        serde_json::from_slice(body).map_err(|_| RateProviderError::InvalidData)?;
    if rows.len() > 3660 {
        return Err(RateProviderError::InvalidData);
    }
    let mut observations = BTreeMap::new();
    for row in rows {
        let effective_date = parse_date(&row.date)?;
        let mut sources = Vec::new();
        for source in row.providers {
            if parse_date(&source.date)? > effective_date
                || source
                    .rate
                    .to_string()
                    .parse::<f64>()
                    .ok()
                    .is_none_or(|rate| !rate.is_finite() || rate <= 0.)
            {
                return Err(RateProviderError::InvalidData);
            }
            sources.push(source.key);
        }
        sources.sort();
        sources.dedup();
        let observation = ExchangeRateObservation {
            effective_date,
            base_currency: row.base,
            quote_currency: row.quote,
            rate: row.rate.to_string(),
            provider: "frankfurter".into(),
            provider_sources: sources,
            fetched_at_utc: fetched_at,
        };
        observation
            .validate()
            .map_err(|_| RateProviderError::InvalidData)?;
        if observations
            .insert(
                (effective_date, observation.quote_currency.clone()),
                observation,
            )
            .is_some()
        {
            return Err(RateProviderError::InvalidData);
        }
    }
    Ok(observations.into_values().collect())
}
fn parse_date(value: &str) -> Result<NaiveDate, RateProviderError> {
    let date =
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| RateProviderError::InvalidData)?;
    if value.len() != 10 || !(1..=9999).contains(&date.year()) || date.to_string() != value {
        return Err(RateProviderError::InvalidData);
    }
    Ok(date)
}
fn classify_error(error: ureq::Error) -> RateProviderError {
    match error {
        ureq::Error::Timeout(_) => RateProviderError::Timeout,
        ureq::Error::StatusCode(_) | ureq::Error::TooManyRedirects => RateProviderError::Provider,
        _ => RateProviderError::Network,
    }
}
