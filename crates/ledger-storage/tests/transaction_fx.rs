use chrono::{DateTime, NaiveDate, Utc};
use ledger_app::LedgerRepository;
use ledger_domain::{
    Account, AccountId, AccountKind, CurrencyCode, FxSnapshotSource, Money, Posting, Transaction,
    TransactionFxSnapshot, TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn fixture() -> (Account, Transaction) {
    let usd = CurrencyCode::new("USD", 2).unwrap();
    let cny = CurrencyCode::new("CNY", 2).unwrap();
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
        occurred_at: utc("2026-10-30T12:00:00Z"),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Confirmed,
        postings: vec![Posting {
            account_id: account.id,
            delta: Money::new(-10000, usd.clone()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(5000, cny)),
        fx_snapshot: Some(TransactionFxSnapshot {
            quote_currency: usd,
            rate: "2".into(),
            effective_date: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            source: FxSnapshotSource::Reference,
            provider: Some("frankfurter".into()),
            provider_sources: vec!["ecb".into(), "bank".into()],
        }),
        merchant: None,
        description: None,
    };
    (account, transaction)
}
#[test]
fn snapshot_survives_reopen_edit_and_removal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let (account, original) = fixture();
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(original.clone()).unwrap();
    drop(repo);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(
        repo.get_transaction(original.id).unwrap().unwrap(),
        original
    );
    let mut edited = original.clone();
    edited.fx_snapshot.as_mut().unwrap().source = FxSnapshotSource::Manual;
    edited.fx_snapshot.as_mut().unwrap().provider = None;
    edited
        .fx_snapshot
        .as_mut()
        .unwrap()
        .provider_sources
        .clear();
    repo.update_transaction(edited.clone()).unwrap();
    assert_eq!(repo.get_transaction(original.id).unwrap().unwrap(), edited);
    let cny = CurrencyCode::new("CNY", 2).unwrap();
    let cny_account = Account {
        id: AccountId::new(),
        display_name: "CNY wallet".into(),
        kind: AccountKind::Asset,
        currency: cny.clone(),
        opening_balance: None,
        opening_balance_effective_at: None,
    };
    repo.insert_account(cny_account.clone()).unwrap();
    edited.postings[0] = Posting {
        account_id: cny_account.id,
        delta: Money::new(-5000, cny),
    };
    edited.fx_snapshot = None;
    repo.update_transaction(edited.clone()).unwrap();
    assert_eq!(repo.get_transaction(original.id).unwrap().unwrap(), edited);
}
#[test]
fn failed_snapshot_write_rolls_back_transaction_and_postings() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, mut transaction) = fixture();
    repo.insert_account(account).unwrap();
    transaction.fx_snapshot.as_mut().unwrap().provider_sources = vec!["bad\u{0000}source".into()];
    assert!(repo.insert_transaction(transaction.clone()).is_err());
    assert!(repo.get_transaction(transaction.id).unwrap().is_none());
}
#[test]
fn failed_snapshot_edit_preserves_previous_postings_and_evidence() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, original) = fixture();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(original.clone()).unwrap();
    let mut edited = original.clone();
    edited.postings[0].delta = Money::new(-20000, edited.postings[0].delta.currency().clone());
    edited.fx_snapshot.as_mut().unwrap().provider_sources = vec!["not a key".into()];
    assert!(repo.update_transaction(edited).is_err());
    assert_eq!(
        repo.get_transaction(original.id).unwrap().unwrap(),
        original
    );
}
#[test]
fn snapshot_export_and_restore_keep_fx_evidence() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, transaction) = fixture();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction.clone()).unwrap();
    let snapshot = repo.snapshot().unwrap();
    let restored = SqliteLedgerRepository::from_snapshot(&snapshot).unwrap();
    assert_eq!(
        restored.get_transaction(transaction.id).unwrap().unwrap(),
        transaction
    );
}
#[test]
fn fresh_database_uses_schema_fifteen() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(repo.schema_version().unwrap(), 15);
}
#[test]
fn schema_thirteen_snapshot_upgrades_without_inventing_fx() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (mut account, mut transaction) = fixture();
    account.currency = CurrencyCode::new("CNY", 2).unwrap();
    transaction.postings[0].delta = Money::new(-5000, account.currency.clone());
    transaction.fx_snapshot = None;
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction.clone()).unwrap();
    let mut raw = serde_json::to_value(repo.snapshot().unwrap()).unwrap();
    raw["schema_version"] = 13.into();
    raw["tables"].as_array_mut().unwrap().retain(|table| {
        ![
            "transaction_fx_snapshots",
            "crypto_watchlist",
            "crypto_records",
        ]
        .contains(&table["name"].as_str().unwrap())
    });
    let old = serde_json::from_value(raw).unwrap();
    let upgraded = SqliteLedgerRepository::from_snapshot(&old).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), 15);
    assert_eq!(
        upgraded.get_transaction(transaction.id).unwrap().unwrap(),
        transaction
    );
}
#[test]
fn restore_rejects_fx_evidence_with_wrong_source_relationship() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, transaction) = fixture();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction).unwrap();
    let mut raw = serde_json::to_value(repo.snapshot().unwrap()).unwrap();
    let table = raw["tables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|table| table["name"] == "transaction_fx_snapshots")
        .unwrap();
    let source_index = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|column| column == "source_kind")
        .unwrap();
    table["rows"][0][source_index] = serde_json::json!({"Text":"manual"});
    let corrupt = serde_json::from_value(raw).unwrap();
    assert!(SqliteLedgerRepository::from_snapshot(&corrupt).is_err());
}
#[test]
fn schema_thirteen_database_upgrade_keeps_cny_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let (mut account, mut transaction) = fixture();
    account.currency = CurrencyCode::new("CNY", 2).unwrap();
    transaction.postings[0].delta = Money::new(-5000, account.currency.clone());
    transaction.fx_snapshot = None;
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction.clone()).unwrap();
    drop(repo);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE transaction_fx_snapshots; DROP TABLE crypto_records; DROP TABLE crypto_watchlist; DELETE FROM schema_version WHERE version>=14",
    )
    .unwrap();
    drop(db);
    let upgraded = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), 15);
    assert_eq!(
        upgraded.get_transaction(transaction.id).unwrap().unwrap(),
        transaction
    );
}
#[test]
fn invalid_provider_source_key_cannot_be_persisted() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, mut transaction) = fixture();
    repo.insert_account(account).unwrap();
    transaction.fx_snapshot.as_mut().unwrap().provider_sources = vec!["not a key".into()];
    assert!(repo.insert_transaction(transaction.clone()).is_err());
    assert!(repo.get_transaction(transaction.id).unwrap().is_none());
}
#[test]
fn malformed_rate_cannot_be_stored_even_when_sqlite_casts_its_prefix() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, mut transaction) = fixture();
    repo.insert_account(account).unwrap();
    transaction.fx_snapshot.as_mut().unwrap().rate = "2.0.0".into();
    assert!(repo.insert_transaction(transaction.clone()).is_err());
    assert!(repo.get_transaction(transaction.id).unwrap().is_none());
}
#[test]
fn deleting_transaction_cascades_to_fx_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let (account, transaction) = fixture();
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    repo.insert_account(account).unwrap();
    repo.insert_transaction(transaction.clone()).unwrap();
    drop(repo);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    db.execute(
        "DELETE FROM audit_events WHERE transaction_id=?1",
        [transaction.id.as_uuid().to_string()],
    )
    .unwrap();
    db.execute(
        "DELETE FROM transaction_metadata WHERE transaction_id=?1",
        [transaction.id.as_uuid().to_string()],
    )
    .unwrap();
    db.execute(
        "DELETE FROM transactions WHERE id=?1",
        [transaction.id.as_uuid().to_string()],
    )
    .unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM transaction_fx_snapshots", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn snapshot_with_mismatched_cny_amount_is_rejected_before_persisting() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let (account, mut transaction) = fixture();
    repo.insert_account(account).unwrap();
    transaction.report_amount_cny = Some(Money::new(6000, CurrencyCode::new("CNY", 2).unwrap()));
    assert!(repo.insert_transaction(transaction.clone()).is_err());
    assert!(repo.get_transaction(transaction.id).unwrap().is_none());
}
