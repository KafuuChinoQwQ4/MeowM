use super::credential_store::{
    CredentialService, CredentialStore, CredentialStoreError, NativeCredentialStore,
};
use super::model::{
    ConnectionStatus, ConnectionTestOutcome, ConnectionTestRequest, ConnectionTestResult,
    CredentialInput, ProviderErrorCode, ProviderId,
};
use crate::{LedgerState, access};
use std::sync::Arc;
use tauri::State;

pub struct ConnectorState {
    credentials: Arc<CredentialService>,
    ifind_tokens: Arc<super::markets::IfindTokenCache>,
}

impl ConnectorState {
    pub fn initialize() -> Self {
        let store: Arc<dyn CredentialStore> = Arc::new(NativeCredentialStore::initialize());
        Self {
            credentials: Arc::new(CredentialService::new(store)),
            ifind_tokens: Arc::new(super::markets::IfindTokenCache::default()),
        }
    }

    pub(crate) fn credentials(&self) -> Arc<CredentialService> {
        self.credentials.clone()
    }
}

fn storage_error(_: CredentialStoreError) -> String {
    "secure_storage_unavailable".to_string()
}

#[tauri::command]
pub fn provider_connections(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
) -> Result<Vec<ConnectionStatus>, String> {
    access(ledger, |_| Ok(()))?;
    connectors.credentials.statuses().map_err(storage_error)
}

#[tauri::command]
pub fn save_provider_credentials(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    input: CredentialInput,
) -> Result<ConnectionStatus, String> {
    access(ledger.clone(), |_| Ok(()))?;
    let provider = input.provider();
    let status = connectors.credentials.save(input).map_err(storage_error)?;
    if provider == ProviderId::Okx {
        access(ledger, |l| {
            l.clear_import_preview();
            Ok(())
        })?;
    }
    if provider == ProviderId::Ifind {
        connectors.ifind_tokens.invalidate();
    }
    Ok(status)
}

#[tauri::command]
pub fn remove_provider_credentials(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    provider: ProviderId,
) -> Result<ConnectionStatus, String> {
    access(ledger.clone(), |_| Ok(()))?;
    if provider == ProviderId::Ifind {
        connectors.ifind_tokens.invalidate();
    }
    let status = connectors
        .credentials
        .remove(provider)
        .map_err(storage_error)?;
    if provider == ProviderId::Okx {
        access(ledger, |l| {
            l.clear_import_preview();
            Ok(())
        })?;
    }
    Ok(status)
}

#[tauri::command]
pub async fn connections_test(
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    request: ConnectionTestRequest,
) -> Result<ConnectionTestResult, String> {
    access(ledger, |_| Ok(()))?;
    let credentials = connectors.credentials();
    let ifind_tokens = connectors.ifind_tokens.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let provider = request.provider;
        let loaded = match credentials.load(provider) {
            Ok(Some(value)) => value,
            Ok(None) => {
                return Ok(ConnectionTestResult {
                    provider,
                    outcome: ConnectionTestOutcome::Failed,
                    summary: None,
                    error_code: Some(ProviderErrorCode::InvalidCredentials),
                });
            }
            Err(_) => {
                return Ok(ConnectionTestResult {
                    provider,
                    outcome: ConnectionTestOutcome::Failed,
                    summary: None,
                    error_code: Some(ProviderErrorCode::SecureStorageUnavailable),
                });
            }
        };
        let result = match provider {
            ProviderId::WechatPay | ProviderId::Alipay => super::payments::test(request, loaded),
            ProviderId::Binance | ProviderId::Okx | ProviderId::Ifind => {
                super::markets::test(request, loaded, &ifind_tokens)
            }
        };
        match result {
            Ok(summary) => Ok(ConnectionTestResult {
                provider,
                outcome: ConnectionTestOutcome::Connected,
                summary: Some(summary),
                error_code: None,
            }),
            Err(error_code) => Ok(ConnectionTestResult {
                provider,
                outcome: ConnectionTestOutcome::Failed,
                summary: None,
                error_code: Some(error_code),
            }),
        }
    })
    .await
    .map_err(|_| "provider_test_failed".to_string())?
}
