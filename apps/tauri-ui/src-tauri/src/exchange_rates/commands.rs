use super::{provider::HttpRateProvider, service};
use crate::{LedgerState, Result, access, ledger::Ledger};
use chrono::{NaiveDate, Utc};
use ledger_storage::sqlite::{
    ExchangeRateObservation, ExchangeRateSyncState, SqliteLedgerRepository,
};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub(crate) struct HistoryRow {
    effective_date: String,
    quote_currency: String,
    rate: String,
    provider: String,
    provider_sources: Vec<String>,
}
impl From<ExchangeRateObservation> for HistoryRow {
    fn from(row: ExchangeRateObservation) -> Self {
        Self {
            effective_date: row.effective_date.to_string(),
            quote_currency: row.quote_currency,
            rate: row.rate,
            provider: row.provider,
            provider_sources: row.provider_sources,
        }
    }
}
#[derive(Serialize)]
pub(crate) struct SyncStateView {
    pub(crate) status: String,
    latest_date: Option<String>,
    last_attempt_utc: Option<String>,
    last_success_utc: Option<String>,
    error_code: Option<String>,
}
impl SyncStateView {
    pub(crate) fn load(repo: &SqliteLedgerRepository) -> Result<Self> {
        let state = repo
            .exchange_rate_sync_state()
            .map_err(|_| "无法读取汇率同步状态")?;
        let latest_date = repo
            .newest_exchange_rate_date()
            .map_err(|_| "无法读取汇率日期")?;
        Ok(Self::from_state(state, latest_date))
    }
    fn from_state(state: Option<ExchangeRateSyncState>, latest_date: Option<NaiveDate>) -> Self {
        Self {
            latest_date: latest_date.map(|date| date.to_string()),
            last_attempt_utc: state
                .as_ref()
                .and_then(|state| state.last_attempt_utc)
                .map(|time| time.to_rfc3339()),
            last_success_utc: state
                .as_ref()
                .and_then(|state| state.last_success_utc)
                .map(|time| time.to_rfc3339()),
            error_code: state.as_ref().and_then(|state| state.error_code.clone()),
            status: state
                .map(|state| state.status)
                .unwrap_or_else(|| "never".into()),
        }
    }
}
impl Ledger {
    pub(crate) fn exchange_rates_history(
        &self,
        quote: &str,
        from: &str,
        until: &str,
    ) -> Result<Vec<HistoryRow>> {
        let parse = |value: &str| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .filter(|date| value.len() == 10 && date.to_string() == value)
                .ok_or_else(|| "汇率日期无效".to_string())
        };
        let (from, until) = (parse(from)?, parse(until)?);
        if until.signed_duration_since(from).num_days() > 3660 {
            return Err("请选择十年以内的汇率范围".into());
        }
        self.repo()?
            .exchange_rates(quote, from, until)
            .map(|rows| rows.into_iter().map(HistoryRow::from).collect())
            .map_err(|_| "汇率币种或日期范围无效".into())
    }
}

#[tauri::command]
pub(crate) fn exchange_rates_history(
    state: State<'_, LedgerState>,
    quote_currency: String,
    from_date: String,
    until_date: String,
) -> Result<Vec<HistoryRow>> {
    access(state, |ledger| {
        ledger.exchange_rates_history(&quote_currency, &from_date, &until_date)
    })
}
#[tauri::command]
pub(crate) fn exchange_rate_for_date(
    state: State<'_, LedgerState>,
    quote_currency: String,
    date: String,
) -> Result<Option<HistoryRow>> {
    access(state, |ledger| {
        ledger
            .repo()?
            .exchange_rate_at_or_before(&quote_currency, crate::ledger::date(&date)?)
            .map(|row| row.map(HistoryRow::from))
            .map_err(|e| e.to_string())
    })
}
#[tauri::command]
pub(crate) fn exchange_rates_sync_state(state: State<'_, LedgerState>) -> Result<SyncStateView> {
    access(state, |ledger| SyncStateView::load(&ledger.repo()?))
}
#[tauri::command]
pub(crate) async fn exchange_rates_refresh(state: State<'_, LedgerState>) -> Result<SyncStateView> {
    let path = access(state, |ledger| Ok(ledger.database_path().to_path_buf()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = SqliteLedgerRepository::open(&path).map_err(|_| "无法打开本地汇率缓存")?;
        service::sync(&repo, &HttpRateProvider::default(), Utc::now(), true)
            .map_err(|_| "无法保存汇率，请稍后重试")?;
        SyncStateView::load(&repo)
    })
    .await
    .map_err(|_| "汇率更新任务未完成，请重试")?
}
