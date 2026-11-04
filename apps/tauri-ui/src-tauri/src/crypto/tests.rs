use super::commands::commit_import;
use crate::{connectors::markets::okx::model::HistoryPage, ledger::Ledger};
use ledger_domain::crypto::{CryptoLeg, CryptoRecord};

fn page() -> HistoryPage {
    HistoryPage {
        records: vec![CryptoRecord {
            key: "okx:123:deposit:456".into(),
            account_uid: "123".into(),
            source: "deposit".into(),
            source_id: "456".into(),
            occurred_at: 1708587373361,
            instrument: None,
            quote_asset: None,
            legs: vec![CryptoLeg {
                asset: "USDT".into(),
                quantity: "1".into(),
            }],
            fee: None,
            price: None,
            platform_state: "2".into(),
        }],
        next_after: None,
        fetched: 1,
    }
}

#[test]
fn outstanding_import_cannot_write_after_its_session_is_invalidated() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let generation = ledger.import_generation();
    // This invalidation is shared by lock, restore, deletion and OKX credential changes.
    ledger.clear_import_preview();
    assert!(commit_import(&ledger, generation, "123".into(), page()).is_err());
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .crypto_records(None, 50, 0)
            .unwrap()
            .1,
        0
    );
    commit_import(&ledger, ledger.import_generation(), "123".into(), page()).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .crypto_records(None, 50, 0)
            .unwrap()
            .1,
        1
    );
}
