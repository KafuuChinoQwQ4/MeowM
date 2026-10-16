//! Migration replay, atomic rollback and independent SQLite connections.
use chrono::{Duration, Utc};
use ledger_app::{LedgerRepository, reconciliation::ReconciliationService};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
use rusqlite::{Connection, params};
fn money(n: i64) -> Money {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap())
}
fn seed(path: &std::path::Path) -> Account {
    let repo = SqliteLedgerRepository::open(path).unwrap();
    let mut a = Account::new("钱包", AccountKind::Asset, money(0).currency().clone());
    a.opening_balance = Some(money(10000));
    a.opening_balance_effective_at = Some(Utc::now() - Duration::days(2));
    repo.insert_account(a.clone()).unwrap();
    a
}
#[test]
fn schema_eight_placeholders_are_preserved_but_never_presented_as_complete_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let db = Connection::open(&path).unwrap();
    for (i, sql) in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0002_classification_rules.sql"),
        include_str!("../migrations/0003_optional_balance.sql"),
        include_str!("../migrations/0004_nine_features.sql"),
        include_str!("../migrations/0005_pending_import.sql"),
        include_str!("../migrations/0006_notifications.sql"),
        include_str!("../migrations/0007_recurring.sql"),
        include_str!("../migrations/0008_search_rules.sql"),
    ]
    .iter()
    .enumerate()
    {
        db.execute_batch(sql).unwrap();
        db.execute("INSERT INTO schema_version VALUES(?1,0)", [i + 1])
            .unwrap();
    }
    let account = AccountId::new();
    let id = ReconciliationId::new();
    db.execute("INSERT INTO accounts(id,display_name,kind,currency_code,currency_exponent,opening_balance_minor,opening_balance_effective_at_utc_micros) VALUES(?1,'旧平台','asset','CNY',2,1000,0)",[account.as_uuid().to_string()]).unwrap();
    db.execute("INSERT INTO reconciliation_sessions(id,account_id,cutoff_utc_micros,expected_minor,actual_minor,status) VALUES(?1,?2,1,1000,999,'closed')",params![id.as_uuid().to_string(),account.as_uuid().to_string()]).unwrap();
    drop(db);
    for _ in 0..2 {
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        assert_eq!(repo.schema_version().unwrap(), 9);
        let service = ReconciliationService::new(&repo);
        let s = service.summary(id).unwrap();
        assert_eq!(s.record.session.actual_minor, 999);
        assert!(!s.record.snapshot_ready);
        assert!(service.reopen(id, 0).is_err());
        assert!(s.entries.is_empty());
    }
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        9
    );
}
#[test]
fn failed_adjustment_rolls_back_transaction_postings_receipt_version_and_audits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let a = seed(&path);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let service = ReconciliationService::new(&repo);
    let s = service.start(a.id, Utc::now(), money(9900)).unwrap();
    let id = s.record.session.id;
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER injected_audit_failure BEFORE INSERT ON reconciliation_events WHEN NEW.action='adjusted' BEGIN SELECT RAISE(ABORT,'simulated storage failure'); END;").unwrap();
    assert!(service.add_adjustment(id, 0, "核对补差").is_err());
    assert_eq!(service.summary(id).unwrap(), s);
    assert!(repo.list_transactions().unwrap().is_empty());
    for table in ["postings", "transaction_metadata", "audit_events"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    db.execute_batch("DROP TRIGGER injected_audit_failure")
        .unwrap();
    service.add_adjustment(id, 0, "核对补差").unwrap();
    assert_eq!(repo.list_transactions().unwrap().len(), 1);
    // The immutable SQL boundary protects historical numbers independently of the UI.
    assert!(
        db.execute(
            "UPDATE reconciliation_sessions SET actual_minor=0 WHERE id=?1",
            [id.as_uuid().to_string()]
        )
        .is_err()
    );
}
#[test]
fn simultaneous_adjustment_retries_have_one_receipt_and_one_posting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let a = seed(&path);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let s = ReconciliationService::new(&repo)
        .start(a.id, Utc::now(), money(9900))
        .unwrap();
    let id = s.record.session.id;
    drop(repo);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let repo = SqliteLedgerRepository::open(&path).unwrap();
                barrier.wait();
                ReconciliationService::new(&repo)
                    .add_adjustment(id, 0, "同一次明确确认")
                    .unwrap()
            })
        })
        .collect();
    let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(ids[0], ids[1]);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.list_transactions().unwrap().len(), 1);
    assert_eq!(
        repo.balance_as_of(a.id, s.record.session.cutoff_utc)
            .unwrap(),
        Some(money(9900))
    );
}
