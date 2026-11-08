use crate::connectors::{
    credentials::ConnectorState,
    markets::api::{self, ApiRequest, ApiResponse},
};
use crate::{LedgerState, access};
use tauri::{AppHandle, Manager, State};

fn session_current(app: &AppHandle, generation: u64) -> bool {
    let Some(state) = app.try_state::<LedgerState>() else {
        return false;
    };
    let Ok(ledger) = state.0.lock() else {
        return false;
    };
    ledger.require_unlocked().is_ok() && ledger.import_generation() == generation
}

fn ifind_mutating(endpoint_id: &str) -> bool {
    crate::connectors::markets::ifind_api::catalogue()
        .as_array()
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("id").and_then(serde_json::Value::as_str) == Some(endpoint_id)
            })
        })
        .and_then(|entry| entry.get("mutating"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

#[tauri::command]
pub(crate) fn binance_api_catalogue(
    ledger: State<'_, LedgerState>,
) -> Result<serde_json::Value, String> {
    access(ledger, |_| Ok(api::binance_catalogue().clone()))
}

#[tauri::command]
pub(crate) fn ifind_api_catalogue(
    ledger: State<'_, LedgerState>,
) -> Result<serde_json::Value, String> {
    access(ledger, |_| {
        Ok(crate::connectors::markets::ifind_api::catalogue().clone())
    })
}

#[tauri::command]
pub(crate) async fn binance_api_request(
    app: AppHandle,
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: ApiRequest,
) -> Result<ApiResponse, String> {
    let generation = access(ledger, |l| Ok(l.import_generation()))?;
    let credentials = connectors.credentials();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(api::execute_binance_guarded(
            request,
            &credentials,
            &app,
            generation,
        ))
    })
    .await
    .map_err(|_| "provider_request_failed".to_string())?
}

#[tauri::command]
pub(crate) async fn binance_socket_request(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: ApiRequest,
) -> Result<ApiResponse, String> {
    access(ledger, |_| Ok(()))?;
    let credentials = connectors.credentials();
    Ok(api::execute_binance_socket(request, &credentials).await)
}

#[tauri::command]
pub(crate) async fn ifind_api_request(
    app: AppHandle,
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: ApiRequest,
) -> Result<ApiResponse, String> {
    let generation = access(ledger, |l| Ok(l.import_generation()))?;
    let request_id = request.request_id.clone();
    let mutating = ifind_mutating(&request.endpoint_id);
    let credentials = connectors.credentials();
    let tokens = connectors.ifind_tokens();
    let response = tauri::async_runtime::spawn_blocking(move || {
        Ok::<ApiResponse, String>(crate::connectors::markets::ifind_api::execute_ifind(
            request,
            &credentials,
            &tokens,
        ))
    })
    .await
    .map_err(|_| "provider_request_failed".to_string())??;
    if session_current(&app, generation) {
        Ok(response)
    } else if mutating {
        Ok(ApiResponse {
            status: "unknown",
            code: response.code,
            message: response.message,
            data: serde_json::Value::Null,
            request_id,
            http_status: response.http_status,
        })
    } else {
        Err("crypto_session_changed".to_string())
    }
}
