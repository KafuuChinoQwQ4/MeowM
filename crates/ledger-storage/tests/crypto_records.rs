use ledger_app::{LedgerRepository, ports::PrivacyRepository};
use ledger_domain::crypto::{CryptoLeg, CryptoRecord, CryptoReviewStatus};
use ledger_storage::sqlite::SqliteLedgerRepository;

fn fill() -> CryptoRecord {
    CryptoRecord {
        key: "okx:123:fill:456".into(),
        account_uid: "123".into(),
        source: "fill".into(),
        source_id: "456".into(),
        occurred_at: 1708587373361,
        instrument: Some("BTC-USDT".into()),
        quote_asset: Some("USDT".into()),
        legs: vec![
            CryptoLeg {
                asset: "BTC".into(),
                quantity: "0.00192834".into(),
            },
            CryptoLeg {
                asset: "USDT".into(),
                quantity: "-99.99985572".into(),
            },
        ],
        fee: Some(CryptoLeg {
            asset: "BTC".into(),
            quantity: "-0.00000192834".into(),
        }),
        price: Some("51858".into()),
        platform_state: "filled".into(),
    }
}

#[test]
fn crypto_reimport_keeps_review_and_never_creates_fiat_transactions() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(repo.stage_crypto_records(&[fill()]).unwrap(), 1);
    repo.review_crypto_records(&[fill().key], CryptoReviewStatus::Confirmed)
        .unwrap();
    assert_eq!(repo.stage_crypto_records(&[fill()]).unwrap(), 0);
    let (records, total) = repo.crypto_records(None, 50, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(records[0].status, CryptoReviewStatus::Confirmed);
    assert_eq!(records[0].record, fill());
    assert!(repo.list_transactions().unwrap().is_empty());
    let mut invalid = fill();
    invalid.source_id = "789".into();
    invalid.key = "okx:123:fill:789".into();
    invalid.legs[1].quantity = "-100".into();
    let mut another = fill();
    another.key = "okx:123:fill:790".into();
    another.source_id = "790".into();
    assert!(repo.stage_crypto_records(&[another, invalid]).is_err());
    assert_eq!(repo.crypto_records(None, 50, 0).unwrap().1, 1);
}

#[test]
fn incomplete_deposits_cannot_confirm_and_reimport_refreshes_upstream_status() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut record = fill();
    record.source = "deposit".into();
    record.key = "okx:123:deposit:456".into();
    record.instrument = None;
    record.quote_asset = None;
    record.price = None;
    record.fee = None;
    record.legs.truncate(1);
    record.platform_state = "0".into();
    repo.stage_crypto_records(&[record.clone()]).unwrap();
    assert!(
        repo.review_crypto_records(&[record.key.clone()], CryptoReviewStatus::Confirmed)
            .is_err()
    );
    record.platform_state = "2".into();
    assert_eq!(repo.stage_crypto_records(&[record.clone()]).unwrap(), 0);
    repo.review_crypto_records(&[record.key], CryptoReviewStatus::Confirmed)
        .unwrap();
}

#[test]
fn crypto_data_round_trips_snapshot_and_is_erased_by_full_cleanup() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.save_crypto_watchlist(&["BTC-USDT".into(), "DOGE-USDT".into()])
        .unwrap();
    repo.stage_crypto_records(&[fill()]).unwrap();
    let snapshot = repo.snapshot().unwrap();
    let restored = SqliteLedgerRepository::from_snapshot(&snapshot).unwrap();
    assert_eq!(
        restored.crypto_watchlist().unwrap(),
        vec!["BTC-USDT", "DOGE-USDT"]
    );
    assert_eq!(
        restored.crypto_records(None, 50, 0).unwrap().0[0].record,
        fill()
    );
    restored
        .delete_all_data(restored.snapshot().unwrap().fingerprint().unwrap())
        .unwrap();
    assert_eq!(restored.crypto_records(None, 50, 0).unwrap().1, 0);
    assert!(restored.crypto_watchlist().unwrap().is_empty());
}
