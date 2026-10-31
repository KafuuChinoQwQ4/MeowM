use super::{CredentialService, CredentialStore, CredentialStoreError, MemoryCredentialStore};
use crate::connectors::model::{CredentialField, CredentialInput, ProviderId};
use std::sync::Arc;

fn binance_credentials(api_key: &str, secret_key: &str) -> CredentialInput {
    serde_json::from_value(serde_json::json!({
        "provider": "binance",
        "api_key": api_key,
        "secret_key": secret_key,
    }))
    .unwrap()
}

#[test]
fn credentials_are_isolated_and_remove_clears_every_field() {
    let store = Arc::new(MemoryCredentialStore::default());
    let service = CredentialService::new(store.clone());

    service
        .save(binance_credentials("binance-api-key", "binance-secret"))
        .unwrap();

    assert!(service.status(ProviderId::Binance).unwrap().configured);
    assert!(!service.status(ProviderId::Okx).unwrap().configured);
    service.remove(ProviderId::Binance).unwrap();
    assert!(!service.status(ProviderId::Binance).unwrap().configured);
    assert!(
        store
            .get(ProviderId::Binance, CredentialField::ApiKey)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .get(ProviderId::Binance, CredentialField::SecretKey)
            .unwrap()
            .is_none()
    );
}

#[test]
fn failed_group_save_restores_the_previous_credentials() {
    let store = Arc::new(MemoryCredentialStore::default());
    let service = CredentialService::new(store.clone());
    service
        .save(binance_credentials("old-api-key", "old-secret"))
        .unwrap();

    store.fail_next_write_after(1);
    assert!(matches!(
        service.save(binance_credentials("new-api-key", "new-secret")),
        Err(CredentialStoreError::Unavailable)
    ));

    assert_eq!(
        store
            .get(ProviderId::Binance, CredentialField::ApiKey)
            .unwrap()
            .unwrap()
            .as_str(),
        "old-api-key"
    );
    assert_eq!(
        store
            .get(ProviderId::Binance, CredentialField::SecretKey)
            .unwrap()
            .unwrap()
            .as_str(),
        "old-secret"
    );
}

#[test]
fn failed_group_remove_restores_credentials_deleted_before_the_failure() {
    let store = Arc::new(MemoryCredentialStore::default());
    let service = CredentialService::new(store.clone());
    service
        .save(binance_credentials("api-key", "secret-key"))
        .unwrap();

    store.fail_next_delete_after(1);
    assert!(matches!(
        service.remove(ProviderId::Binance),
        Err(CredentialStoreError::Unavailable)
    ));

    assert_eq!(
        store
            .get(ProviderId::Binance, CredentialField::ApiKey)
            .unwrap()
            .unwrap()
            .as_str(),
        "api-key"
    );
    assert_eq!(
        store
            .get(ProviderId::Binance, CredentialField::SecretKey)
            .unwrap()
            .unwrap()
            .as_str(),
        "secret-key"
    );
}

#[test]
fn loading_credentials_rejects_partial_groups_and_returns_zeroizing_inputs() {
    let store = Arc::new(MemoryCredentialStore::default());
    let service = CredentialService::new(store.clone());
    store
        .set(
            ProviderId::Binance,
            CredentialField::ApiKey,
            "partial-api-key",
        )
        .unwrap();
    assert!(service.load(ProviderId::Binance).unwrap().is_none());

    service
        .save(binance_credentials("complete-api-key", "complete-secret"))
        .unwrap();
    let loaded = service.load(ProviderId::Binance).unwrap().unwrap();
    match loaded {
        CredentialInput::Binance {
            api_key,
            secret_key,
        } => {
            assert_eq!(api_key.expose_secret(), "complete-api-key");
            assert_eq!(secret_key.expose_secret(), "complete-secret");
        }
        _ => panic!("loaded provider credentials must retain their type"),
    }
}
