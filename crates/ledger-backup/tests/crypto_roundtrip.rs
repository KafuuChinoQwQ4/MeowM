use ledger_backup::BackupService;
use ledger_domain::crypto::{CryptoLeg, CryptoRecord, CryptoReviewStatus};
use ledger_storage::sqlite::SqliteLedgerRepository;

#[test]
fn encrypted_backup_preserves_crypto_quantities_and_review_without_credentials() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let record = CryptoRecord {
        key: "okx:123:deposit:456".into(),
        account_uid: "123".into(),
        source: "deposit".into(),
        source_id: "456".into(),
        occurred_at: 1708587373361,
        instrument: None,
        quote_asset: None,
        legs: vec![CryptoLeg {
            asset: "USDT".into(),
            quantity: "12.000000000000001".into(),
        }],
        fee: None,
        price: None,
        platform_state: "2".into(),
    };
    repo.stage_crypto_records(&[record.clone()]).unwrap();
    repo.review_crypto_records(&[record.key.clone()], CryptoReviewStatus::Confirmed)
        .unwrap();
    repo.save_crypto_watchlist(&["BTC-USDT".into()]).unwrap();
    let bytes = BackupService::create(&repo, "test-only-password").unwrap();
    let inspected = BackupService::inspect(&bytes, "test-only-password").unwrap();
    let restored = SqliteLedgerRepository::in_memory().unwrap();
    BackupService::restore(
        &restored,
        &inspected,
        restored.snapshot().unwrap().fingerprint().unwrap(),
    )
    .unwrap();
    let (rows, total) = restored.crypto_records(None, 50, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].record, record);
    assert_eq!(rows[0].status, CryptoReviewStatus::Confirmed);
    assert_eq!(restored.crypto_watchlist().unwrap(), vec!["BTC-USDT"]);
}
