use crate::{
    LedgerState, access,
    connectors::{
        credentials::ConnectorState,
        markets::okx::advanced::{self, AdvancedClient, ApiRequest, ApiResponse, Execution},
    },
};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub(crate) fn okx_api_catalogue(
    ledger: State<'_, LedgerState>,
) -> Result<serde_json::Value, String> {
    access(ledger, |_| Ok(advanced::catalogue().clone()))
}

// This check also runs inside the blocking worker immediately before the HTTP send.
fn session_current(app: &AppHandle, generation: u64) -> bool {
    let Some(state) = app.try_state::<LedgerState>() else {
        return false;
    };
    let Ok(ledger) = state.0.lock() else {
        return false;
    };
    ledger.require_unlocked().is_ok() && ledger.import_generation() == generation
}

fn complete(execution: Execution, session_is_current: bool) -> ApiResponse {
    if session_is_current {
        execution.response
    } else {
        ApiResponse::session_changed(
            execution.response.request_id,
            execution.sent && execution.mutating,
        )
    }
}

#[tauri::command]
pub(crate) async fn okx_api_request(
    app: AppHandle,
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: ApiRequest,
) -> Result<ApiResponse, String> {
    let generation = access(ledger.clone(), |l| Ok(l.import_generation()))?;
    let service = connectors.credentials();
    let client = AdvancedClient::default();
    let mutating = client.is_mutating(&request.endpoint_id) && request.confirmed;
    let request_id = request.request_id.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        client.execute_guarded(request, &service, || session_current(&app, generation))
    })
    .await;
    let still_current = access(ledger, |l| {
        if l.import_generation() == generation {
            Ok(())
        } else {
            Err("crypto_session_changed".into())
        }
    })
    .is_ok();
    match outcome {
        Ok(execution) => Ok(complete(execution, still_current)),
        // The worker could have sent a mutation before it failed, so retain the reconciliation ID.
        Err(_) => Ok(ApiResponse::session_changed(request_id, mutating)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_during_sent_mutation_preserves_only_unknown_reconciliation_id() {
        let result = complete(
            Execution {
                response: ApiResponse {
                    status: "success",
                    code: Some("0".into()),
                    message: None,
                    data: serde_json::json!({"account":"private"}),
                    request_id: "req-123".into(),
                    http_status: Some(200),
                },
                sent: true,
                mutating: true,
            },
            false,
        );
        assert_eq!(result.status, "unknown");
        assert_eq!(result.request_id, "req-123");
        assert!(result.data.is_null());
        assert!(result.code.is_none());
    }
}
