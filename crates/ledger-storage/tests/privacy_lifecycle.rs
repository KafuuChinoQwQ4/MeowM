use chrono::Utc;
use ledger_app::{
    FeatureRepository, LedgerRepository, ports::PrivacyRepository, privacy::PrivacyService,
};
use ledger_domain::{
    Account, AccountKind, CurrencyCode, Money, Posting, SourceEvidence, SourceKind, Transaction,
    TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use rusqlite::Connection;
use uuid::Uuid;

fn seed() -> (
    tempfile::TempDir,
    SqliteLedgerRepository,
    TransactionId,
    TransactionId,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    let add = |status, n: i64| {
        let id = TransactionId::new();
        let tx = Transaction {
            id,
            occurred_at: Utc::now(),
            kind: TransactionKind::Expense,
            status,
            postings: vec![Posting {
                account_id: account.id,
                delta: Money::new(-n, account.currency.clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(Money::new(n, account.currency.clone())),
            merchant: Some(format!("merchant-{n}")),
            description: None,
        };
        repo.insert_transaction_with_evidence(
            tx,
            Some(SourceEvidence {
                source_kind: SourceKind::Notification,
                external_transaction_id: Some(format!("{n:064x}")),
                original_external_transaction_id: None,
                import_batch_id: Uuid::new_v4(),
                source_fields_digest: [n as u8; 32],
            }),
        )
        .unwrap();
        id
    };
    let pending = add(TransactionStatus::Pending, 123);
    let confirmed = add(TransactionStatus::Confirmed, 456);
    let c = Connection::open(&path).unwrap();
    for (id, n) in [(pending, 123), (confirmed, 456)] {
        c.execute(
            "INSERT INTO notification_receipts(identity,digest,transaction_id) VALUES(?1,?2,?3)",
            rusqlite::params![
                format!("{n:064x}"),
                vec![n as u8; 32],
                id.as_uuid().to_string()
            ],
        )
        .unwrap();
    }
    (dir, repo, pending, confirmed)
}

#[test]
fn notification_cleanup_previews_rolls_back_on_failure_and_keeps_confirmed_postings() {
    let (dir, repo, pending, confirmed) = seed();
    let preview = repo.preview_notification_delete().unwrap();
    assert_eq!(
        (preview.pending, preview.confirmed, preview.receipts),
        (1, 1, 2)
    );
    let before = repo.snapshot().unwrap().fingerprint().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let c = Connection::open(&path).unwrap();
    c.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON notification_receipts BEGIN SELECT RAISE(ABORT,'simulated failure'); END;").unwrap();
    assert!(repo.delete_notification_data(before).is_err());
    assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
    c.execute_batch("DROP TRIGGER fail_cleanup").unwrap();
    repo.delete_notification_data(before).unwrap();
    assert!(repo.get_transaction(pending).unwrap().is_none());
    assert_eq!(
        repo.get_transaction(confirmed).unwrap().unwrap().postings[0]
            .delta
            .minor_units(),
        -456
    );
    assert!(repo.source_evidence(confirmed).unwrap().is_empty());
    assert_eq!(repo.preview_notification_delete().unwrap().receipts, 0);
}

#[test]
fn full_delete_requires_fresh_preview_and_reseeds_default_accounts() {
    let (_dir, repo, _, _) = seed();
    let mut privacy = PrivacyService::load(&repo).unwrap();
    privacy.enable_lock(&repo, "local-only-password").unwrap();
    privacy.set_hide_amounts(&repo, true).unwrap();
    let old = repo.snapshot().unwrap().fingerprint().unwrap();
    repo.insert_account(Account::new(
        "后来新增",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    ))
    .unwrap();
    assert!(repo.delete_all_data(old).is_err());
    assert_eq!(repo.list_transactions().unwrap().len(), 2);
    let fresh = repo.snapshot().unwrap().fingerprint().unwrap();
    repo.delete_all_data(fresh).unwrap();
    assert!(repo.list_transactions().unwrap().is_empty());
    let names: Vec<_> = repo
        .list_accounts()
        .unwrap()
        .into_iter()
        .map(|a| a.display_name)
        .collect();
    assert_eq!(names, vec!["微信", "支付宝", "现金"]);
    assert!(repo.privacy_settings().unwrap().password_verifier.is_none());
    assert!(!repo.privacy_settings().unwrap().lock_enabled);
    assert!(!repo.privacy_settings().unwrap().hide_amounts);
}

#[test]
fn historical_reconciliation_blocks_single_source_cleanup() {
    let (dir, repo, pending, _) = seed();
    let c = Connection::open(dir.path().join("ledger.sqlite3")).unwrap();
    let account_id: String = c
        .query_row("SELECT id FROM accounts LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let session = Uuid::new_v4().to_string();
    c.execute("INSERT INTO reconciliation_sessions(id,account_id,cutoff_utc_micros,expected_minor,actual_minor) VALUES(?1,?2,0,0,0)",rusqlite::params![session,account_id]).unwrap();
    c.execute("INSERT INTO reconciliation_entries(session_id,transaction_id,occurred_at_utc_micros,kind,status,delta_minor) VALUES(?1,?2,0,'expense','pending',-123)",rusqlite::params![session,pending.as_uuid().to_string()]).unwrap();
    assert_eq!(repo.preview_notification_delete().unwrap().blocked, 1);
    let before = repo.snapshot().unwrap().fingerprint().unwrap();
    assert!(repo.delete_notification_data(before).is_err());
    assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
}
