use chrono::{DateTime, NaiveDate, Utc};
use ledger_backup::{BackupError, BackupService};
use ledger_storage::sqlite::{ExchangeRateObservation, SqliteLedgerRepository};

#[test]
fn password_and_tamper_failure_leave_original_intact() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let before = repo.snapshot().unwrap().fingerprint().unwrap();
    let bytes = BackupService::create(&repo, "test-only-pass").unwrap();
    assert!(!bytes.windows(8).any(|w| w == b"accounts"));
    assert_ne!(
        bytes,
        BackupService::create(&repo, "test-only-pass").unwrap()
    );
    assert!(matches!(
        BackupService::inspect(&bytes, "wrong-password"),
        Err(BackupError::Authentication)
    ));
    let mut corrupted = bytes.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(BackupService::inspect(&corrupted, "test-only-pass").is_err());
    let inspected = BackupService::inspect(&bytes, "test-only-pass").unwrap();
    BackupService::restore(&repo, &inspected, before).unwrap();
    assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
    assert!(BackupService::inspect(b"broken", "test-only-pass").is_err());
    assert!(BackupService::create(&repo, "short").is_err());
}

#[test]
fn exchange_rate_history_survives_encrypted_backup_inspection_and_restore() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.upsert_exchange_rates(&[ExchangeRateObservation {
        effective_date: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
        base_currency: "CNY".into(),
        quote_currency: "USD".into(),
        rate: "7.12345678912345".into(),
        provider: "frankfurter".into(),
        provider_sources: vec!["ecb".into()],
        fetched_at_utc: DateTime::parse_from_rfc3339("2026-10-30T17:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    }])
    .unwrap();
    let bytes = BackupService::create(&repo, "test-only-pass").unwrap();
    let inspected = BackupService::inspect(&bytes, "test-only-pass").unwrap();
    assert_eq!(inspected.info.schema_version, 13);

    let restored = SqliteLedgerRepository::in_memory().unwrap();
    let expected = restored.snapshot().unwrap().fingerprint().unwrap();
    BackupService::restore(&restored, &inspected, expected).unwrap();
    let rows = restored
        .exchange_rates(
            "USD",
            NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
        )
        .unwrap();
    assert_eq!(rows[0].rate, "7.12345678912345");
}
