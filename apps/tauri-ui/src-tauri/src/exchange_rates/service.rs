use super::provider::RateProvider;
use chrono::{DateTime, Duration, Local, Utc};
use ledger_app::RepositoryError;
use ledger_storage::sqlite::{ExchangeRateSyncState, SqliteLedgerRepository};

pub(crate) fn sync(
    repo: &SqliteLedgerRepository,
    provider: &dyn RateProvider,
    now_utc: DateTime<Utc>,
    force: bool,
) -> Result<ExchangeRateSyncState, RepositoryError> {
    let today = now_utc.with_timezone(&Local).date_naive();
    let previous = repo.exchange_rate_sync_state()?;
    if !force
        && let Some(previous) = &previous
        && matches!(previous.status.as_str(), "success" | "no_new_data")
        && previous
            .last_attempt_utc
            .is_some_and(|value| value.with_timezone(&Local).date_naive() == today)
    {
        return Ok(previous.clone());
    }
    let newest = repo.newest_exchange_rate_date()?;
    let oldest = today - Duration::days(364);
    // Re-fetch the newest observation so the provider can finalize a rate first seen earlier that day.
    let from = newest.unwrap_or(oldest).max(oldest);
    let mut state = ExchangeRateSyncState {
        last_attempt_utc: Some(now_utc),
        last_success_utc: previous.and_then(|state| state.last_success_utc),
        status: "no_new_data".into(),
        error_code: None,
    };
    if from <= today {
        match provider.fetch(Some(from), today) {
            Ok(rows) => {
                if rows.iter().any(|row| {
                    row.effective_date < from
                        || row.effective_date > today
                        || row.validate().is_err()
                }) {
                    state.status = "invalid_data".into();
                    state.error_code = Some("invalid_data".into());
                } else {
                    let inserted = repo.upsert_exchange_rates(&rows)?;
                    state.status = if inserted == 0 {
                        "no_new_data"
                    } else {
                        "success"
                    }
                    .into();
                    state.last_success_utc = Some(now_utc);
                }
            }
            Err(error) => {
                state.status = error.code().into();
                state.error_code = Some(error.code().into());
            }
        }
    } else {
        state.last_success_utc = Some(now_utc);
    }
    repo.record_exchange_rate_sync(&state)?;
    Ok(state)
}
