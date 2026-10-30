use super::*;
use chrono::{Datelike, NaiveDate};

pub const SUPPORTED_EXCHANGE_RATE_QUOTES: [&str; 10] = [
    "USD", "EUR", "JPY", "HKD", "GBP", "KRW", "AUD", "CAD", "SGD", "CHF",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeRateObservation {
    pub effective_date: NaiveDate,
    pub base_currency: String,
    pub quote_currency: String,
    pub rate: String,
    pub provider: String,
    pub provider_sources: Vec<String>,
    pub fetched_at_utc: DateTime<Utc>,
}

impl ExchangeRateObservation {
    pub fn validate(&self) -> Result<(), RepositoryError> {
        validate_observation(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeRateSyncState {
    pub last_attempt_utc: Option<DateTime<Utc>>,
    pub last_success_utc: Option<DateTime<Utc>>,
    pub status: String,
    pub error_code: Option<String>,
}

impl SqliteLedgerRepository {
    pub fn upsert_exchange_rates(
        &self,
        observations: &[ExchangeRateObservation],
    ) -> Result<usize, RepositoryError> {
        for observation in observations {
            validate_observation(observation)?;
        }

        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut inserted = 0;
        for observation in observations {
            let sources = serde_json::to_string(&observation.provider_sources)
                .map_err(|_| invalid_exchange_rate())?;
            inserted += tx
                .execute(
                    "INSERT INTO exchange_rates
                     (effective_date, base_currency_code, quote_currency_code, rate, provider, provider_sources, fetched_at_utc_micros)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(effective_date, base_currency_code, quote_currency_code) DO UPDATE SET
                       rate = excluded.rate,
                       provider = excluded.provider,
                       provider_sources = excluded.provider_sources,
                       fetched_at_utc_micros = excluded.fetched_at_utc_micros
                     WHERE excluded.fetched_at_utc_micros > exchange_rates.fetched_at_utc_micros
                       AND (excluded.rate != exchange_rates.rate OR excluded.provider != exchange_rates.provider
                            OR excluded.provider_sources != exchange_rates.provider_sources)",
                    rusqlite::params![
                        observation.effective_date.format("%Y-%m-%d").to_string(),
                        observation.base_currency,
                        observation.quote_currency,
                        observation.rate,
                        observation.provider,
                        sources,
                        observation.fetched_at_utc.timestamp_micros(),
                    ],
                )
                .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        Ok(inserted)
    }

    pub fn exchange_rates(
        &self,
        quote_currency: &str,
        from_inclusive: NaiveDate,
        until_exclusive: NaiveDate,
    ) -> Result<Vec<ExchangeRateObservation>, RepositoryError> {
        if !is_supported_quote(quote_currency) || from_inclusive >= until_exclusive {
            return Err(invalid_exchange_rate());
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT effective_date, base_currency_code, quote_currency_code, rate, provider, provider_sources, fetched_at_utc_micros
                 FROM exchange_rates
                 WHERE quote_currency_code = ?1 AND effective_date >= ?2 AND effective_date < ?3
                 ORDER BY effective_date ASC",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(
                rusqlite::params![
                    quote_currency,
                    from_inclusive.format("%Y-%m-%d").to_string(),
                    until_exclusive.format("%Y-%m-%d").to_string(),
                ],
                map_observation,
            )
            .map_err(storage_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)
    }

    pub fn exchange_rate_at_or_before(
        &self,
        quote_currency: &str,
        date: NaiveDate,
    ) -> Result<Option<ExchangeRateObservation>, RepositoryError> {
        if !is_supported_quote(quote_currency) {
            return Err(invalid_exchange_rate());
        }
        self.connection.query_row(
            "SELECT effective_date, base_currency_code, quote_currency_code, rate, provider, provider_sources, fetched_at_utc_micros FROM exchange_rates WHERE base_currency_code='CNY' AND quote_currency_code=?1 AND effective_date<=?2 ORDER BY effective_date DESC LIMIT 1",
            rusqlite::params![quote_currency, date.to_string()], map_observation,
        ).optional().map_err(storage_error)
    }

    pub fn newest_exchange_rate_date(&self) -> Result<Option<NaiveDate>, RepositoryError> {
        let latest = self
            .connection
            .query_row(
                "SELECT MAX(effective_date) FROM exchange_rates",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(storage_error)?;
        latest.map(|value| parse_date(&value)).transpose()
    }

    pub fn exchange_rate_sync_state(
        &self,
    ) -> Result<Option<ExchangeRateSyncState>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT last_attempt_utc_micros, last_success_utc_micros, status, error_code
                 FROM exchange_rate_sync WHERE singleton = 1",
                [],
                |row| {
                    Ok(ExchangeRateSyncState {
                        last_attempt_utc: read_utc(row.get(0)?)?,
                        last_success_utc: read_utc(row.get(1)?)?,
                        status: row.get(2)?,
                        error_code: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn record_exchange_rate_sync(
        &self,
        state: &ExchangeRateSyncState,
    ) -> Result<(), RepositoryError> {
        validate_sync_state(state)?;
        self.connection
            .execute(
                "INSERT INTO exchange_rate_sync
                 (singleton, last_attempt_utc_micros, last_success_utc_micros, status, error_code)
                 VALUES (1, ?1, ?2, ?3, ?4)
                 ON CONFLICT(singleton) DO UPDATE SET
                   last_attempt_utc_micros = excluded.last_attempt_utc_micros,
                   last_success_utc_micros = excluded.last_success_utc_micros,
                   status = excluded.status,
                   error_code = excluded.error_code",
                rusqlite::params![
                    state.last_attempt_utc.map(|value| value.timestamp_micros()),
                    state.last_success_utc.map(|value| value.timestamp_micros()),
                    state.status,
                    state.error_code,
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub(super) fn validate_exchange_rate_snapshot(&self) -> Result<(), RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT effective_date, base_currency_code, quote_currency_code, rate, provider, provider_sources, fetched_at_utc_micros
                 FROM exchange_rates ORDER BY effective_date, quote_currency_code",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], map_observation)
            .map_err(storage_error)?;
        for row in rows {
            validate_observation(&row.map_err(storage_error)?)?;
        }
        if let Some(state) = self.exchange_rate_sync_state()? {
            validate_sync_state(&state)?;
        }
        Ok(())
    }
}

fn map_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExchangeRateObservation> {
    let effective_date = row.get::<_, String>(0)?;
    let sources = row.get::<_, String>(5)?;
    let fetched_at = row.get::<_, i64>(6)?;
    Ok(ExchangeRateObservation {
        effective_date: parse_date(&effective_date).map_err(to_sql_conversion_error)?,
        base_currency: row.get(1)?,
        quote_currency: row.get(2)?,
        rate: row.get(3)?,
        provider: row.get(4)?,
        provider_sources: serde_json::from_str(&sources)
            .map_err(|_| to_sql_conversion_error(invalid_exchange_rate()))?,
        fetched_at_utc: DateTime::from_timestamp_micros(fetched_at)
            .ok_or_else(|| to_sql_conversion_error(invalid_exchange_rate()))?,
    })
}

fn validate_observation(observation: &ExchangeRateObservation) -> Result<(), RepositoryError> {
    if observation.base_currency != "CNY"
        || !is_supported_quote(&observation.quote_currency)
        || observation.provider != "frankfurter"
        || !is_positive_decimal(&observation.rate)
        || observation.provider_sources.len() > 128
        || observation
            .provider_sources
            .iter()
            .any(|source| !is_provider_source(source))
    {
        return Err(invalid_exchange_rate());
    }
    Ok(())
}

fn validate_sync_state(state: &ExchangeRateSyncState) -> Result<(), RepositoryError> {
    let allowed_status = matches!(
        state.status.as_str(),
        "success" | "no_new_data" | "network_error" | "timeout" | "provider_error" | "invalid_data"
    );
    let safe_error = state.error_code.as_ref().is_none_or(|code| {
        !code.is_empty()
            && code.len() <= 64
            && code.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
    });
    if !allowed_status || !safe_error {
        return Err(invalid_exchange_rate());
    }
    Ok(())
}

fn is_supported_quote(value: &str) -> bool {
    SUPPORTED_EXCHANGE_RATE_QUOTES.contains(&value)
}

fn is_positive_decimal(value: &str) -> bool {
    if value.len() > 64 {
        return false;
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|digits| {
            digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return false;
    }
    whole
        .bytes()
        .chain(fraction.unwrap_or_default().bytes())
        .any(|byte| byte != b'0')
}

fn is_provider_source(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

fn parse_date(value: &str) -> Result<NaiveDate, RepositoryError> {
    if value.len() != 10 {
        return Err(invalid_exchange_rate());
    }
    let parsed =
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid_exchange_rate())?;
    if !(1..=9999).contains(&parsed.year()) || parsed.format("%Y-%m-%d").to_string() != value {
        return Err(invalid_exchange_rate());
    }
    Ok(parsed)
}

fn read_utc(value: Option<i64>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value
        .map(|micros| {
            DateTime::from_timestamp_micros(micros)
                .ok_or_else(|| to_sql_conversion_error(invalid_exchange_rate()))
        })
        .transpose()
}

fn to_sql_conversion_error(error: RepositoryError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error.to_string())),
    )
}

fn invalid_exchange_rate() -> RepositoryError {
    RepositoryError::InvalidData("invalid or unsupported exchange-rate data".into())
}
