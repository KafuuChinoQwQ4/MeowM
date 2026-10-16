use ledger_app::{
    FeatureRepository, LedgerRepository,
    privacy::{PrivacyError, PrivacyService},
};
use ledger_domain::{Account, AccountKind, CurrencyCode};
use ledger_storage::sqlite::SqliteLedgerRepository;
use std::time::{Duration, Instant};

#[test]
fn app_lock_restarts_locked_and_rejects_wrong_password() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "test-only-lock-pass").unwrap();
    assert!(!privacy.is_locked());
    assert!(
        repo.privacy_settings()
            .unwrap()
            .password_verifier
            .unwrap()
            .starts_with("$argon2id$")
    );
    privacy.lock_now();
    assert!(privacy.require_unlocked().is_err());
    assert!(privacy.unlock("wrong-password").is_err());
    assert!(privacy.require_unlocked().is_err());
    privacy.unlock("test-only-lock-pass").unwrap();
    assert!(privacy.require_unlocked().is_ok());
    let restarted = PrivacyService::load(&repo).unwrap();
    assert!(restarted.is_locked());
}

#[test]
fn idle_timeout_and_hide_amounts_only_change_the_view() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "test-only-lock-pass").unwrap();
    privacy.set_auto_lock_minutes(&repo, 1).unwrap();
    privacy.set_hide_amounts(&repo, true).unwrap();
    assert_eq!(privacy.redact_view("123.45"), "••••");
    assert!(repo.privacy_settings().unwrap().hide_amounts);
    let now = Instant::now();
    privacy.touch_at(now).unwrap();
    assert!(
        privacy
            .require_unlocked_at(now + Duration::from_secs(59))
            .is_ok()
    );
    assert!(
        privacy
            .require_unlocked_at(now + Duration::from_secs(60))
            .is_err()
    );
    assert!(privacy.touch_at(now + Duration::from_secs(61)).is_err());
    privacy.unlock("test-only-lock-pass").unwrap();
    privacy.disable_lock(&repo, "test-only-lock-pass").unwrap();
    assert!(!PrivacyService::load(&repo).unwrap().is_locked());
}

#[test]
fn stale_delete_gets_a_specific_retry_error() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    let old = repo.snapshot().unwrap().fingerprint().unwrap();
    repo.insert_account(Account::new(
        "later",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    ))
    .unwrap();
    assert_eq!(
        privacy.delete_all_data(&repo, old).unwrap_err(),
        PrivacyError::Changed
    );
    assert_eq!(repo.list_accounts().unwrap().len(), 1);
}

#[test]
fn repeated_wrong_password_is_temporarily_limited() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "test-only-lock-pass").unwrap();
    privacy.lock_now();
    for _ in 0..5 {
        assert_eq!(
            privacy.unlock("incorrect").unwrap_err(),
            PrivacyError::WrongPassword
        );
    }
    assert_eq!(
        privacy.unlock("test-only-lock-pass").unwrap_err(),
        PrivacyError::RateLimited
    );
}

#[test]
fn auto_lock_without_user_activity_survives_status_reads() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "test-only-lock-pass").unwrap();
    privacy.set_auto_lock_minutes(&repo, 1).unwrap();
    let now = Instant::now();
    privacy.touch_at(now).unwrap();
    assert!(
        privacy
            .require_unlocked_at(now + Duration::from_secs(59))
            .is_ok()
    );
    assert!(
        privacy
            .require_unlocked_at(now + Duration::from_secs(60))
            .is_err()
    );
}

#[test]
fn restoring_display_preference_refreshes_process_state_without_unlocking() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "test-only-lock-pass").unwrap();
    privacy.lock_now();
    let mut restored = repo.privacy_settings().unwrap();
    restored.hide_amounts = true;
    repo.save_privacy_settings(restored).unwrap();
    privacy.reload_settings(&repo).unwrap();
    assert_eq!(privacy.redact_view("123.45"), "••••");
    assert!(privacy.is_locked());
}
