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
    assert_eq!(inspected.info.schema_version, 15);

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

#[test]
fn transaction_fx_snapshot_survives_encrypted_backup() {
    use ledger_app::LedgerRepository;
    use ledger_domain::{
        Account, AccountId, AccountKind, CurrencyCode, FxSnapshotSource, Money, Posting,
        Transaction, TransactionFxSnapshot, TransactionId, TransactionKind, TransactionStatus,
    };
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let usd = CurrencyCode::new("USD", 2).unwrap();
    let account = Account {
        id: AccountId::new(),
        display_name: "USD wallet".into(),
        kind: AccountKind::Asset,
        currency: usd.clone(),
        opening_balance: None,
        opening_balance_effective_at: None,
    };
    let transaction = Transaction {
        id: TransactionId::new(),
        occurred_at: DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Confirmed,
        postings: vec![Posting {
            account_id: account.id,
            delta: Money::new(-10000, usd.clone()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(5000, CurrencyCode::new("CNY", 2).unwrap())),
        fx_snapshot: Some(TransactionFxSnapshot {
            quote_currency: usd,
            rate: "2.00000000000000".into(),
            effective_date: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            source: FxSnapshotSource::Reference,
            provider: Some("frankfurter".into()),
            provider_sources: vec!["ecb".into(), "bank".into()],
        }),
        merchant: None,
        description: None,
    };
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction.clone()).unwrap();
    let encrypted = BackupService::create(&repo, "test-only-pass").unwrap();
    let inspected = BackupService::inspect(&encrypted, "test-only-pass").unwrap();
    assert_eq!(inspected.info.schema_version, 15);
    let restored = SqliteLedgerRepository::in_memory().unwrap();
    let expected = restored.snapshot().unwrap().fingerprint().unwrap();
    BackupService::restore(&restored, &inspected, expected).unwrap();
    assert_eq!(
        restored.get_transaction(transaction.id).unwrap().unwrap(),
        transaction
    );
}
