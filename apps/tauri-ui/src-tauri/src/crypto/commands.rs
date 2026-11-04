use crate::connectors::{
    credential_store::CredentialService,
    credentials::ConnectorState,
    markets::okx::{
        OkxClient, OkxCredentials,
        model::*,
        orders::{OkxOrderService, OrderActionResult, OrderSubmissionResult, TradeContextResponse},
    },
    model::{CredentialInput, ProviderErrorCode, ProviderId},
};
use crate::{LedgerState, access};
use ledger_domain::crypto::{CryptoReviewStatus, ReviewedCryptoRecord};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

fn ensure_generation(ledger: &crate::ledger::Ledger, expected: u64) -> Result<(), String> {
    ledger.require_unlocked()?;
    if ledger.import_generation() != expected {
        return Err("crypto_session_changed".into());
    }
    Ok(())
}

pub(super) fn complete_exchange_result<T>(
    result: Result<T, String>,
    session_check: Result<(), String>,
    reconciliation_error: Option<String>,
) -> Result<T, String> {
    if let Err(error) = session_check {
        return Err(reconciliation_error.unwrap_or(error));
    }
    result
}

fn provider_error(error: ProviderErrorCode) -> String {
    serde_json::to_value(error)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "provider_failure".into())
}
async fn background<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ProviderErrorCode> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| "provider_failure".to_string())?
        .map_err(provider_error)
}
fn private<T>(
    service: Arc<CredentialService>,
    read: impl FnOnce(&OkxClient, &OkxCredentials<'_>) -> Result<T, ProviderErrorCode>,
) -> Result<T, ProviderErrorCode> {
    let loaded = service
        .load(ProviderId::Okx)
        .map_err(|_| ProviderErrorCode::SecureStorageUnavailable)?;
    let Some(CredentialInput::Okx {
        api_key,
        secret_key,
        passphrase,
    }) = loaded
    else {
        return Err(ProviderErrorCode::InvalidCredentials);
    };
    read(
        &OkxClient::default(),
        &OkxCredentials {
            api_key: api_key.expose_secret(),
            secret_key: secret_key.expose_secret(),
            passphrase: passphrase.expose_secret(),
        },
    )
}

#[tauri::command]
pub(crate) async fn okx_tickers(ledger: State<'_, LedgerState>) -> Result<Vec<Ticker>, String> {
    access(ledger.clone(), |_| Ok(()))?;
    let result = background(|| OkxClient::default().read_tickers()).await?;
    access(ledger, |_| Ok(result))
}
#[tauri::command]
pub(crate) async fn okx_candles(
    ledger: State<'_, LedgerState>,
    instrument: String,
    bar: String,
    after: Option<String>,
) -> Result<Vec<Candle>, String> {
    access(ledger.clone(), |_| Ok(()))?;
    let result =
        background(move || OkxClient::default().read_candles(&instrument, &bar, after.as_deref()))
            .await?;
    access(ledger, |_| Ok(result))
}
#[tauri::command]
pub(crate) fn okx_watchlist(ledger: State<'_, LedgerState>) -> Result<Vec<String>, String> {
    access(ledger, |l| {
        l.repo()?
            .crypto_watchlist()
            .map_err(|_| "crypto_storage_error".into())
    })
}
#[tauri::command]
pub(crate) fn okx_save_watchlist(
    ledger: State<'_, LedgerState>,
    instruments: Vec<String>,
) -> Result<(), String> {
    access(ledger, |l| {
        l.repo()?
            .save_crypto_watchlist(&instruments)
            .map_err(|_| "invalid_request".into())
    })
}
#[tauri::command]
pub(crate) async fn okx_assets(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
) -> Result<AssetOverview, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let service = connectors.credentials();
    let result = background(move || {
        private(service, |client, credentials| {
            client.read_assets(credentials)
        })
    })
    .await?;
    access(ledger, |l| {
        ensure_generation(l, generation)?;
        Ok(result)
    })
}
#[tauri::command]
pub(crate) async fn okx_positions(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    instrument_type: String,
    after: Option<String>,
) -> Result<PositionOverview, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let service = connectors.credentials();
    let result = background(move || {
        private(service, |client, credentials| {
            client.read_positions(credentials, &instrument_type, after.as_deref())
        })
    })
    .await?;
    access(ledger, |l| {
        ensure_generation(l, generation)?;
        Ok(result)
    })
}

#[tauri::command]
pub(crate) async fn okx_trade_context(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    inst_type: OrderInstrumentType,
    inst_id: String,
    environment: OkxEnvironment,
) -> Result<TradeContextResponse, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let credentials = connectors.credentials();
    let result = background(move || {
        OkxOrderService::new(OkxClient::default(), credentials).trade_context(
            inst_type,
            &inst_id,
            environment,
        )
    })
    .await;
    let session_check = access(ledger, |l| ensure_generation(l, generation));
    complete_exchange_result(result, session_check, None)
}

#[tauri::command]
pub(crate) async fn okx_place_order(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    draft: OrderDraft,
) -> Result<OrderSubmissionResult, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let client_order_id = draft.cl_ord_id.clone();
    let credentials = connectors.credentials();
    let result = background(move || {
        OkxOrderService::new(OkxClient::default(), credentials).place_order(draft)
    })
    .await;
    let session_check = access(ledger, |l| ensure_generation(l, generation));
    // A write may already have reached OKX. Preserve its only reconciliation
    // key if the local session was locked or invalidated while in flight.
    complete_exchange_result(
        result,
        session_check,
        Some(format!(
            "order_result_requires_reconciliation:{client_order_id}"
        )),
    )
}

#[tauri::command]
pub(crate) async fn okx_order_detail(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    inst_id: String,
    cl_ord_id: String,
    environment: OkxEnvironment,
) -> Result<Option<OrderSnapshot>, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let credentials = connectors.credentials();
    let result = background(move || {
        OkxOrderService::new(OkxClient::default(), credentials).order_detail(
            &inst_id,
            &cl_ord_id,
            environment,
        )
    })
    .await;
    let session_check = access(ledger, |l| ensure_generation(l, generation));
    complete_exchange_result(result, session_check, None)
}

#[tauri::command]
pub(crate) async fn okx_order_history(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    inst_type: OrderInstrumentType,
    environment: OkxEnvironment,
) -> Result<Vec<OrderSnapshot>, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let credentials = connectors.credentials();
    let result = background(move || {
        OkxOrderService::new(OkxClient::default(), credentials)
            .order_history(inst_type, environment)
    })
    .await;
    let session_check = access(ledger, |l| ensure_generation(l, generation));
    complete_exchange_result(result, session_check, None)
}

#[tauri::command]
pub(crate) async fn okx_cancel_order(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    inst_id: String,
    ord_id: String,
    environment: OkxEnvironment,
) -> Result<OrderActionResult, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let order_id = ord_id.clone();
    let credentials = connectors.credentials();
    let result = background(move || {
        OkxOrderService::new(OkxClient::default(), credentials).cancel_order(
            &inst_id,
            &ord_id,
            environment,
        )
    })
    .await;
    let session_check = access(ledger, |l| ensure_generation(l, generation));
    complete_exchange_result(
        result,
        session_check,
        Some(format!("order_cancel_requires_reconciliation:{order_id}")),
    )
}

#[derive(Serialize)]
pub(crate) struct ImportResult {
    inserted: u64,
    duplicates: u64,
    fetched: usize,
    next_after: Option<String>,
    account_uid: String,
}
#[tauri::command]
pub(crate) async fn okx_import_history(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: HistoryRequest,
) -> Result<ImportResult, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    request
        .validate(chrono::Utc::now())
        .map_err(provider_error)?;
    let service = connectors.credentials();
    let (uid, page) = background(move || {
        private(service, |client, credentials| {
            let uid = client.account_uid(credentials)?;
            if request
                .account_uid
                .as_deref()
                .is_some_and(|expected| expected != uid)
            {
                return Err(ProviderErrorCode::InvalidRequest);
            }
            let page = client.read_history(credentials, &uid, &request)?;
            Ok((uid, page))
        })
    })
    .await?;
    access(ledger, |l| commit_import(l, generation, uid, page))
}

pub(super) fn commit_import(
    l: &crate::ledger::Ledger,
    generation: u64,
    uid: String,
    page: HistoryPage,
) -> Result<ImportResult, String> {
    ensure_generation(l, generation)?;
    let inserted = l
        .repo()?
        .stage_crypto_records(&page.records)
        .map_err(|_| "crypto_storage_error".to_string())?;
    Ok(ImportResult {
        inserted,
        duplicates: page.records.len() as u64 - inserted,
        fetched: page.fetched,
        next_after: page.next_after,
        account_uid: uid,
    })
}

#[derive(Serialize)]
pub(crate) struct RecordPage {
    rows: Vec<ReviewedCryptoRecord>,
    total: u64,
}
#[tauri::command]
pub(crate) fn okx_records(
    ledger: State<'_, LedgerState>,
    status: Option<CryptoReviewStatus>,
    offset: u32,
) -> Result<RecordPage, String> {
    access(ledger, |l| {
        let (rows, total) = l
            .repo()?
            .crypto_records(status, 50, offset)
            .map_err(|_| "crypto_storage_error".to_string())?;
        Ok(RecordPage { rows, total })
    })
}
#[tauri::command]
pub(crate) fn okx_review_records(
    ledger: State<'_, LedgerState>,
    keys: Vec<String>,
    status: CryptoReviewStatus,
) -> Result<(), String> {
    access(ledger, |l| {
        l.repo()?
            .review_crypto_records(&keys, status)
            .map_err(|_| "crypto_review_invalid".into())
    })
}
