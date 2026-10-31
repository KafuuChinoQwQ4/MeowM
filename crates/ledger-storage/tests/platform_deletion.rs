use chrono::{DateTime, Utc};
use ledger_app::{LedgerRepository, RepositoryError};
use ledger_domain::{
    AccountId, CurrencyCode, Money, Posting, Transaction, TransactionId, TransactionKind,
    TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use rusqlite::{Connection, params};

#[test]
fn deleting_unused_platform_removes_metadata_and_survives_snapshot_restore() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let removed = AccountId::new();
    let kept = AccountId::new();
    let cny = CurrencyCode::new("CNY", 2).unwrap();
    repo.save_platform(removed, "临时钱包", "wallet", true, &cny)
        .unwrap();
    repo.save_platform(kept, "银行卡", "bank", true, &cny)
        .unwrap();
    repo.delete_platform(removed).unwrap();
    assert!(repo.get_account(removed).unwrap().is_none());
    assert!(repo.platform_category(removed).unwrap().is_none());
    assert_eq!(
        repo.get_account(kept).unwrap().unwrap().display_name,
        "银行卡"
    );
    assert!(matches!(
        repo.delete_platform(removed),
        Err(RepositoryError::NotFound)
    ));
    repo.delete_platform(kept).unwrap();
    let restored = SqliteLedgerRepository::from_snapshot(&repo.snapshot().unwrap()).unwrap();
    assert!(restored.list_accounts().unwrap().is_empty());
    assert!(!restored.needs_default_platforms().unwrap());
}

#[test]
fn platform_deletion_rejects_postings_in_every_transaction_status_without_changes() {
    for status in [
        TransactionStatus::Pending,
        TransactionStatus::Confirmed,
        TransactionStatus::Ignored,
    ] {
        let repo = SqliteLedgerRepository::in_memory().unwrap();
        let id = AccountId::new();
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        repo.save_platform(id, "钱包", "wallet", true, &cny)
            .unwrap();
        let transaction = Transaction {
            id: TransactionId::new(),
            occurred_at: DateTime::parse_from_rfc3339("2026-10-14T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            kind: TransactionKind::Expense,
            status,
            postings: vec![Posting {
                account_id: id,
                delta: Money::new(-1200, cny.clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(Money::new(1200, cny)),
            fx_snapshot: None,
            merchant: None,
            description: None,
        };
        repo.insert_transaction(transaction).unwrap();
        let before = repo.snapshot().unwrap().fingerprint().unwrap();
        assert!(
            matches!(repo.delete_platform(id), Err(RepositoryError::InvalidData(message)) if message.contains("流水"))
        );
        assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
    }
}

#[test]
fn platform_deletion_rejects_each_related_feature_without_changes() {
    let cases = [
        (
            "INSERT INTO budgets(id,month,account_id,amount_cny_minor) VALUES(?1,'2026-10',?2,10000)",
            "预算",
        ),
        (
            "INSERT INTO recurring_templates(id,account_id,kind,amount_cny_minor,next_date,interval_months,enabled) VALUES(?1,?2,'expense',1000,'2026-11-01',1,0)",
            "周期账单",
        ),
        (
            "INSERT INTO reconciliation_sessions(id,account_id,cutoff_utc_micros,expected_minor,actual_minor) VALUES(?1,?2,0,0,0)",
            "对账",
        ),
        (
            "INSERT INTO notification_settings(package_name,enabled,account_id) VALUES(?1,0,?2)",
            "采集",
        ),
        (
            "INSERT INTO screen_settings(package_name,enabled,account_id) VALUES(?1,0,?2)",
            "采集",
        ),
    ];
    for (sql, reason) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        let id = AccountId::new();
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        repo.save_platform(id, "钱包", "wallet", true, &cny)
            .unwrap();
        Connection::open(&path)
            .unwrap()
            .execute(
                sql,
                params![uuid::Uuid::new_v4().to_string(), id.as_uuid().to_string()],
            )
            .unwrap();
        let before = repo.snapshot().unwrap().fingerprint().unwrap();
        assert!(
            matches!(repo.delete_platform(id), Err(RepositoryError::InvalidData(message)) if message.contains(reason)),
            "{reason}"
        );
        assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
    }
}
