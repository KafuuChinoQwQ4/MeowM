//! Real-file migration and restart receipts; no simulated repository.
use chrono::{NaiveDate, Utc};
use ledger_app::{FeatureRepository, LedgerRepository, recurring::RecurringService};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
#[test]
fn schema_six_recurring_rows_migrate_without_losing_ids_and_skips_survive_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    for migration in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0002_classification_rules.sql"),
        include_str!("../migrations/0003_optional_balance.sql"),
        include_str!("../migrations/0004_nine_features.sql"),
        include_str!("../migrations/0005_pending_import.sql"),
        include_str!("../migrations/0006_notifications.sql"),
    ] {
        db.execute_batch(migration).unwrap();
    }
    for v in 1..=6 {
        db.execute("INSERT INTO schema_version VALUES(?1,0)", [v])
            .unwrap();
    }
    let legacy_account = AccountId::new();
    let legacy_template = RecurringTemplateId::new();
    let legacy_tx = TransactionId::new();
    let batch = uuid::Uuid::new_v4();
    db.execute(
        "INSERT INTO accounts VALUES(?1,'旧平台','asset','CNY',2,NULL,NULL)",
        [legacy_account.as_uuid().to_string()],
    )
    .unwrap();
    db.execute("INSERT INTO recurring_templates(id,account_id,kind,amount_cny_minor,next_date,interval_months,enabled) VALUES(?1,?2,'expense',900,'2026-10-31',1,1)",rusqlite::params![legacy_template.as_uuid().to_string(),legacy_account.as_uuid().to_string()]).unwrap();
    db.execute("INSERT INTO transactions(id,occurred_at_utc_micros,kind,status,report_amount_minor,report_currency_code,report_currency_exponent) VALUES(?1,0,'expense','pending',900,'CNY',2)",[legacy_tx.as_uuid().to_string()]).unwrap();
    db.execute(
        "INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros) VALUES(?1,0)",
        [legacy_tx.as_uuid().to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO postings VALUES(?1,0,?2,-900,'CNY',2)",
        rusqlite::params![
            legacy_tx.as_uuid().to_string(),
            legacy_account.as_uuid().to_string()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO import_batches(id) VALUES(?1)",
        [batch.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO source_evidence VALUES(42,?1,'manual','legacy-recurring',?2,zeroblob(32))",
        rusqlite::params![legacy_tx.as_uuid().to_string(), batch.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO recurring_instances VALUES(?1,'2026-09-30',?2)",
        rusqlite::params![
            legacy_template.as_uuid().to_string(),
            legacy_tx.as_uuid().to_string()
        ],
    )
    .unwrap();
    drop(db);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.schema_version().unwrap(), 9);
    let migrated = RecurringService::new(&repo).list().unwrap().remove(0);
    assert_eq!(migrated.id, legacy_template);
    assert_eq!(migrated.anchor_date, date("2026-10-31"));
    assert_eq!(
        RecurringService::new(&repo)
            .instances(legacy_template)
            .unwrap()[0]
            .transaction_id,
        Some(legacy_tx)
    );
    assert_eq!(
        repo.source_evidence(legacy_tx).unwrap()[0]
            .external_transaction_id
            .as_deref(),
        Some("legacy-recurring")
    );
    let inspect = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        inspect
            .query_row(
                "SELECT id FROM source_evidence WHERE transaction_id=?1",
                [legacy_tx.as_uuid().to_string()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        42
    );
    assert_eq!(
        inspect
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
    drop(inspect);
    let account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    let template = RecurringTemplate {
        id: RecurringTemplateId::new(),
        account_id: account.id,
        kind: TransactionKind::Income,
        amount_cny_minor: 500,
        category_id: None,
        merchant: None,
        description: None,
        cadence: RecurringCadence::Weekly,
        interval: 1,
        anchor_date: date("2026-10-16"),
        next_due_date: date("2026-10-16"),
        end_date: None,
        enabled: true,
        version: 0,
    };
    let s = RecurringService::new(&repo);
    s.create(template.clone()).unwrap();
    let ids = s.generate_due(date("2026-10-16"), &Utc).unwrap();
    let current = s
        .list()
        .unwrap()
        .into_iter()
        .find(|t| t.id == template.id)
        .unwrap();
    s.skip_instance(template.id, date("2026-10-23"), current.version)
        .unwrap();
    drop(repo);
    let reopened = SqliteLedgerRepository::open(&path).unwrap();
    let s = RecurringService::new(&reopened);
    assert!(s.generate_due(date("2026-10-23"), &Utc).unwrap().is_empty());
    assert_eq!(
        s.list()
            .unwrap()
            .into_iter()
            .find(|t| t.id == template.id)
            .unwrap()
            .next_due_date,
        date("2026-10-30")
    );
    assert_eq!(s.instances(template.id).unwrap().len(), 2);
    assert_eq!(reopened.list_transactions().unwrap().len(), 2);
    reopened.ignore_bulk(&ids).unwrap();
    assert_eq!(
        s.instances(template.id).unwrap()[0].status,
        RecurringInstanceStatus::Ignored
    );
    reopened.restore_pending_atomic(ids[0]).unwrap();
    assert_eq!(
        s.instances(template.id).unwrap()[0].status,
        RecurringInstanceStatus::Pending
    );
}
#[test]
fn generation_rolls_back_all_templates_when_any_local_day_is_unavailable() {
    use chrono::FixedOffset;
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = Account::new(
        "wallet",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    let mut t = RecurringTemplate {
        id: RecurringTemplateId::new(),
        account_id: account.id,
        kind: TransactionKind::Expense,
        amount_cny_minor: 100,
        category_id: None,
        merchant: None,
        description: None,
        cadence: RecurringCadence::Weekly,
        interval: 1,
        anchor_date: date("2011-12-23"),
        next_due_date: date("2011-12-23"),
        end_date: Some(date("2011-12-23")),
        enabled: true,
        version: 0,
    };
    let s = RecurringService::new(&repo);
    s.create(t.clone()).unwrap();
    t.id = RecurringTemplateId::new();
    t.end_date = None;
    t.anchor_date = date("2011-12-30");
    t.next_due_date = t.anchor_date;
    s.create(t).unwrap();
    // Pacific/Apia skipped 2011-12-30 entirely. No partial batch or advanced cursor is allowed.
    assert!(
        s.generate_due(date("2026-10-16"), &chrono_tz::Pacific::Apia)
            .is_err()
    );
    assert!(repo.list_transactions().unwrap().is_empty());
    assert_eq!(s.list().unwrap().iter().map(|t| t.version).sum::<u32>(), 0);
    assert_eq!(
        s.generate_due(date("2011-12-30"), &FixedOffset::east_opt(0).unwrap())
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn two_connections_generating_the_same_due_date_commit_only_one_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.sqlite3");
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    let t = RecurringTemplate {
        id: RecurringTemplateId::new(),
        account_id: account.id,
        kind: TransactionKind::Expense,
        amount_cny_minor: 100,
        category_id: None,
        merchant: None,
        description: None,
        cadence: RecurringCadence::Monthly,
        interval: 1,
        anchor_date: date("2026-10-16"),
        next_due_date: date("2026-10-16"),
        end_date: None,
        enabled: true,
        version: 0,
    };
    RecurringService::new(&repo).create(t.clone()).unwrap();
    drop(repo);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let repo = SqliteLedgerRepository::open(&path).unwrap();
                barrier.wait();
                RecurringService::new(&repo)
                    .generate_due(date("2026-10-16"), &Utc)
                    .unwrap()
                    .len()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .sum::<usize>(),
        1
    );
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.list_transactions().unwrap().len(), 1);
    assert_eq!(
        RecurringService::new(&repo).instances(t.id).unwrap().len(),
        1
    );
}
