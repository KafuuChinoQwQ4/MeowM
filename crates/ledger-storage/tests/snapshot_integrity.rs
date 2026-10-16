use chrono::{Duration, Utc};
use ledger_app::{LedgerRepository, reconciliation::ReconciliationService};
use ledger_domain::*;
use ledger_storage::sqlite::{LedgerSnapshot, SqliteLedgerRepository};
use rusqlite::{Connection, params};
fn money(n: i64) -> Money {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap())
}
fn seeded(path: &std::path::Path) -> (SqliteLedgerRepository, Account) {
    let repo = SqliteLedgerRepository::open(path).unwrap();
    let mut a = Account::new("钱包", AccountKind::Asset, money(0).currency().clone());
    a.opening_balance = Some(money(10000));
    a.opening_balance_effective_at = Some(Utc::now() - Duration::days(5));
    repo.insert_account(a.clone()).unwrap();
    (repo, a)
}
#[test]
fn snapshot_preserves_reconciliation_adjustment_and_rollback_on_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (repo, a) = seeded(&path);
    let service = ReconciliationService::new(&repo);
    let s = service.start(a.id, Utc::now(), money(9900)).unwrap();
    let id = s.record.session.id;
    service.add_adjustment(id, 0, "现金盘点").unwrap();
    let expected = service.summary(id).unwrap();
    let snapshot = repo.snapshot().unwrap();
    let restored = SqliteLedgerRepository::from_snapshot(&snapshot).unwrap();
    assert_eq!(
        ReconciliationService::new(&restored).summary(id).unwrap(),
        expected
    );
    let targetpath = dir.path().join("target.db");
    let (target, _) = seeded(&targetpath);
    let before = target.snapshot().unwrap().fingerprint().unwrap();
    let c = Connection::open(&targetpath).unwrap();
    c.execute_batch("CREATE TRIGGER fail_restore BEFORE INSERT ON reconciliation_events BEGIN SELECT RAISE(ABORT,'disk write simulation'); END;").unwrap();
    assert!(target.replace_snapshot(&snapshot, before).is_err());
    assert_eq!(target.snapshot().unwrap().fingerprint().unwrap(), before);
    c.execute_batch("DROP TRIGGER fail_restore").unwrap();
    target.replace_snapshot(&snapshot, before).unwrap();
    // Restoring over an existing immutable adjustment must also work repeatedly.
    target
        .replace_snapshot(&snapshot, target.snapshot().unwrap().fingerprint().unwrap())
        .unwrap();
    assert_eq!(
        ReconciliationService::new(&target).summary(id).unwrap(),
        expected
    );
}
#[test]
fn credentials_unknown_tables_or_broken_foreign_keys_never_enter_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (repo, a) = seeded(&path);
    let c = Connection::open(path).unwrap();
    c.execute(
        "INSERT INTO app_settings VALUES('password_verifier','test-sensitive-value')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO app_settings VALUES('notification_consent','1')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO app_settings VALUES('hide_amounts','1')", [])
        .unwrap();
    let snap = repo.snapshot().unwrap();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(!json.contains("test-sensitive-value"));
    assert!(!json.contains("notification_consent"));
    let mut json = serde_json::to_value(&snap).unwrap();
    json["tables"][0]["name"] = "accounts; DROP TABLE transactions".into();
    let invalid: LedgerSnapshot = serde_json::from_value(json).unwrap();
    assert!(SqliteLedgerRepository::from_snapshot(&invalid).is_err());
    let mut json = serde_json::to_value(&snap).unwrap();
    let table = json["tables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["name"] == "platform_metadata")
        .unwrap();
    table["rows"] =
        serde_json::json!([[{"Text":AccountId::new().as_uuid().to_string()},{"Text":"cash"}]]);
    assert!(SqliteLedgerRepository::from_snapshot(&serde_json::from_value(json).unwrap()).is_err());
    let restored = SqliteLedgerRepository::from_snapshot(&snap).unwrap();
    assert_eq!(restored.list_accounts().unwrap()[0].id, a.id);
    // Local safety settings survive replacement rather than being copied from another device.
    repo.replace_snapshot(&snap, snap.fingerprint().unwrap())
        .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT value FROM app_settings WHERE key='password_verifier'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "test-sensitive-value"
    );
}
#[test]
fn many_rows_roundtrip_without_losing_metadata_or_allowing_invalid_postings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (repo, a) = seeded(&path);
    let c = Connection::open(path).unwrap();
    let tx = c.unchecked_transaction().unwrap();
    for _ in 0..1200 {
        let id = TransactionId::new().as_uuid().to_string();
        tx.execute("INSERT INTO transactions(id,occurred_at_utc_micros,kind,status) VALUES(?1,0,'expense','pending')",[&id]).unwrap();
        tx.execute(
            "INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros) VALUES(?1,0)",
            [&id],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let snapshot = repo.snapshot().unwrap();
    assert_eq!(
        SqliteLedgerRepository::from_snapshot(&snapshot)
            .unwrap()
            .list_transactions()
            .unwrap()
            .len(),
        1200
    );
    let id = repo.list_transactions().unwrap()[0]
        .id
        .as_uuid()
        .to_string();
    c.execute(
        "UPDATE transactions SET status='confirmed' WHERE id=?1",
        [&id],
    )
    .unwrap();
    c.execute(
        "INSERT INTO postings VALUES(?1,0,?2,1,'CNY',2)",
        params![id, a.id.as_uuid().to_string()],
    )
    .unwrap();
    assert!(SqliteLedgerRepository::from_snapshot(&repo.snapshot().unwrap()).is_err());
}
