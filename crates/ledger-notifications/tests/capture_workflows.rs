use ledger_app::{FeatureRepository, LedgerRepository, ports::NotificationRepository};
use ledger_domain::{Account, AccountKind, CurrencyCode, SourceKind, TransactionStatus};
use ledger_notifications::{NotificationEvent, NotificationSettings, capture_notification};
use ledger_storage::sqlite::SqliteLedgerRepository;

fn event() -> NotificationEvent {
    NotificationEvent {
        package_name: "com.tencent.mm".into(),
        title: "微信支付".into(),
        body: "向咖啡店付款12.30元。原始正文秘密标记".into(),
        notification_key: "private-notification-key".into(),
        posted_at_ms: 1_760_000_000_000,
    }
}
#[test]
fn persists_only_pending_normalized_candidates_and_is_idempotent_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    assert!(
        repo.notification_settings()
            .unwrap()
            .iter()
            .all(|s| !s.enabled)
    );
    assert_eq!(
        capture_notification(&repo, &event()).unwrap().code,
        "ignored"
    );
    assert!(repo.list_transactions().unwrap().is_empty());
    repo.save_notification_settings(NotificationSettings {
        package_name: event().package_name,
        enabled: true,
        account_id: Some(account.id),
    })
    .unwrap();
    let first = capture_notification(&repo, &event()).unwrap();
    assert_eq!(first.code, "pending");
    let id = first.transaction_id.unwrap();
    let tx = repo.get_transaction(id).unwrap().unwrap();
    assert_eq!(tx.status, TransactionStatus::Pending);
    assert_eq!(tx.postings[0].delta.minor_units(), -1230);
    assert_eq!(tx.merchant.as_deref(), Some("咖啡店"));
    let evidence = repo.source_evidence(id).unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].source_kind, SourceKind::Notification);
    repo.ignore_atomic(id).unwrap();
    drop(repo);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let repeated = capture_notification(&repo, &event()).unwrap();
    assert_eq!(repeated.code, "duplicate");
    assert_eq!(repeated.transaction_id, Some(id));
    assert_eq!(
        repo.get_transaction(id).unwrap().unwrap().status,
        TransactionStatus::Ignored
    );
    let mut changed_key = event();
    changed_key.notification_key = "another-key".into();
    assert_eq!(
        capture_notification(&repo, &changed_key)
            .unwrap()
            .transaction_id,
        Some(id)
    );
    let mut reused_key = event();
    reused_key.posted_at_ms += 10_000;
    assert_ne!(
        capture_notification(&repo, &reused_key)
            .unwrap()
            .transaction_id,
        Some(id)
    );
    assert_eq!(repo.list_transactions().unwrap().len(), 2);
    assert_eq!(repo.notification_status().unwrap()[0].code, "pending");
    drop(repo);
    let bytes = std::fs::read(path).unwrap();
    let data = String::from_utf8_lossy(&bytes);
    assert!(!data.contains("原始正文秘密标记"));
    assert!(!data.contains("private-notification-key"));
}
#[test]
fn captures_unknown_amount_and_transfer_without_invalid_postings_and_rejects_invalid_settings() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.save_notification_settings(NotificationSettings {
        package_name: "com.tencent.mm".into(),
        enabled: true,
        account_id: None,
    })
    .unwrap();
    for (n, body) in ["付款内容已隐藏", "转账成功12元", "退款到账12元"]
        .iter()
        .enumerate()
    {
        let mut e = event();
        e.body = (*body).into();
        e.posted_at_ms += n as i64;
        let result = capture_notification(&repo, &e).unwrap();
        assert_eq!(result.code, "needs_confirmation");
        let tx = repo
            .get_transaction(result.transaction_id.unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(tx.status, TransactionStatus::Pending);
        assert!(tx.postings.is_empty());
    }
    assert!(
        repo.save_notification_settings(NotificationSettings {
            package_name: "unknown".into(),
            enabled: true,
            account_id: None
        })
        .is_err()
    );
    let foreign = Account::new(
        "USD",
        AccountKind::Asset,
        CurrencyCode::new("USD", 2).unwrap(),
    );
    repo.insert_account(foreign.clone()).unwrap();
    assert!(
        repo.save_notification_settings(NotificationSettings {
            package_name: "com.tencent.mm".into(),
            enabled: true,
            account_id: Some(foreign.id)
        })
        .is_err()
    );
    let mut invalid = event();
    invalid.notification_key.clear();
    assert!(capture_notification(&repo, &invalid).is_err());
    assert_eq!(repo.list_transactions().unwrap().len(), 3);
}
