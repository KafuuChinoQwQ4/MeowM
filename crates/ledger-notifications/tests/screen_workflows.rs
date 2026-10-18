use ledger_app::{
    FeatureRepository, LedgerRepository,
    ports::{NotificationRepository, ScreenCaptureRepository},
};
use ledger_domain::*;
use ledger_notifications::{
    NotificationEvent, NotificationSettings, ScreenCaptureEvent, capture_notification,
    capture_screen,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
fn event(id: &str) -> ScreenCaptureEvent {
    ScreenCaptureEvent {
        package_name: "com.tencent.mm".into(),
        recognized_text: "支付成功\n商户：咖啡店\n¥12.30".into(),
        captured_at_ms: 1760000000000,
        event_id: id.into(),
    }
}
fn enabled(repo: &SqliteLedgerRepository) -> Account {
    let a = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(a.clone()).unwrap();
    repo.save_screen_settings(NotificationSettings {
        package_name: "com.tencent.mm".into(),
        enabled: true,
        account_id: Some(a.id),
    })
    .unwrap();
    a
}
#[test]
fn disabled_settings_block_capture_and_repeated_frames_keep_one_receipt() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(
        capture_screen(&repo, &event("page1")).unwrap().code,
        "ignored"
    );
    let a = enabled(&repo);
    let first = capture_screen(&repo, &event("page1")).unwrap();
    let mut repeat = event("page1");
    repeat.captured_at_ms += 1000;
    assert_eq!(
        capture_screen(&repo, &repeat).unwrap().transaction_id,
        first.transaction_id
    );
    let tx = repo
        .get_transaction(first.transaction_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(tx.status, TransactionStatus::Pending);
    assert_eq!(tx.postings[0].account_id, a.id);
    assert_eq!(tx.postings[0].delta.minor_units(), -1230);
    let distinct = capture_screen(&repo, &event("page2")).unwrap();
    assert_ne!(
        distinct.transaction_id, first.transaction_id,
        "different payments must survive even if same amount and merchant"
    );
}
#[test]
fn same_amount_across_sources_stays_separate_for_review_in_both_delivery_orders() {
    for screen_first in [false, true] {
        let repo = SqliteLedgerRepository::in_memory().unwrap();
        let a = enabled(&repo);
        repo.save_notification_settings(NotificationSettings {
            package_name: "com.tencent.mm".into(),
            enabled: true,
            account_id: Some(a.id),
        })
        .unwrap();
        let notification = NotificationEvent {
            package_name: "com.tencent.mm".into(),
            title: "微信支付".into(),
            body: "向咖啡店付款12.30元".into(),
            notification_key: "n1".into(),
            posted_at_ms: 1760000005000,
        };
        let (one, two) = if screen_first {
            (
                capture_screen(&repo, &event("page1")).unwrap(),
                capture_notification(&repo, &notification).unwrap(),
            )
        } else {
            (
                capture_notification(&repo, &notification).unwrap(),
                capture_screen(&repo, &event("page1")).unwrap(),
            )
        };
        assert_eq!(two.code, "needs_confirmation");
        assert_ne!(one.transaction_id, two.transaction_id);
        let second = repo
            .get_transaction(two.transaction_id.unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(second.status, TransactionStatus::Pending);
        assert!(
            second
                .description
                .unwrap()
                .contains(&one.transaction_id.unwrap().as_uuid().to_string())
        );
        assert_eq!(
            repo.source_evidence(one.transaction_id.unwrap())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(repo.list_transactions().unwrap().len(), 2);
        assert_ne!(
            capture_screen(&repo, &event("page2"))
                .unwrap()
                .transaction_id,
            one.transaction_id
        );
        let restored = SqliteLedgerRepository::from_snapshot(&repo.snapshot().unwrap()).unwrap();
        assert_eq!(restored.list_transactions().unwrap().len(), 3);
    }
}
#[test]
fn no_cross_merge_without_same_package_account_kind_and_unique_candidate() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let a = enabled(&repo);
    repo.save_notification_settings(NotificationSettings {
        package_name: "com.eg.android.AlipayGphone".into(),
        enabled: true,
        account_id: Some(a.id),
    })
    .unwrap();
    let notification = NotificationEvent {
        package_name: "com.eg.android.AlipayGphone".into(),
        title: "支付宝".into(),
        body: "向咖啡店付款12.30元".into(),
        notification_key: "n1".into(),
        posted_at_ms: 1760000005000,
    };
    let first = capture_notification(&repo, &notification).unwrap();
    assert_ne!(
        first.transaction_id,
        capture_screen(&repo, &event("page1"))
            .unwrap()
            .transaction_id
    );
}

#[test]
fn screen_snapshot_without_account_and_source_cleanup_keep_only_confirmed_postings() {
    use ledger_app::ports::PrivacyRepository;
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteLedgerRepository::open(&dir.path().join("ledger.sqlite3")).unwrap();
    repo.save_screen_settings(NotificationSettings {
        package_name: "com.tencent.mm".into(),
        enabled: true,
        account_id: None,
    })
    .unwrap();
    let mut private = event("incomplete");
    private
        .recognized_text
        .push_str("\nSCREEN_PRIVATE_CANARY_123");
    let pending = capture_screen(&repo, &private)
        .unwrap()
        .transaction_id
        .unwrap();
    for path in std::fs::read_dir(dir.path()).unwrap() {
        let content = std::fs::read(path.unwrap().path()).unwrap();
        assert!(
            !content
                .windows(b"SCREEN_PRIVATE_CANARY_123".len())
                .any(|w| w == b"SCREEN_PRIVATE_CANARY_123")
        );
    }
    let restored = SqliteLedgerRepository::from_snapshot(&repo.snapshot().unwrap()).unwrap();
    assert_eq!(restored.screen_settings().unwrap()[0].account_id, None);
    assert!(
        restored
            .get_transaction(pending)
            .unwrap()
            .unwrap()
            .postings
            .is_empty()
    );
    enabled(&repo);
    let confirmed = capture_screen(&repo, &event("mapped"))
        .unwrap()
        .transaction_id
        .unwrap();
    let mut tx = repo.get_transaction(confirmed).unwrap().unwrap();
    tx.status = TransactionStatus::Confirmed;
    repo.update_transaction(tx).unwrap();
    let preview = repo.preview_notification_delete().unwrap();
    assert_eq!(
        (preview.pending, preview.confirmed, preview.receipts),
        (1, 1, 2)
    );
    repo.delete_notification_data(repo.snapshot().unwrap().fingerprint().unwrap())
        .unwrap();
    assert!(repo.get_transaction(pending).unwrap().is_none());
    let tx = repo.get_transaction(confirmed).unwrap().unwrap();
    assert_eq!(tx.postings[0].delta.minor_units(), -1230);
    assert_eq!(tx.status, TransactionStatus::Confirmed);
    assert!(repo.source_evidence(confirmed).unwrap().is_empty());
    assert!(repo.screen_settings().unwrap().iter().all(|s| !s.enabled));
    assert!(repo.screen_status().unwrap().is_empty());
    assert_eq!(repo.preview_notification_delete().unwrap().receipts, 0);
    assert!(
        repo.snapshot()
            .unwrap()
            .counts()
            .iter()
            .filter(
                |(name, _)| ["screen_receipts", "screen_status", "capture_matches"]
                    .contains(&name.as_str())
            )
            .all(|(_, n)| *n == 0)
    );
    drop(repo);
    for path in std::fs::read_dir(dir.path()).unwrap() {
        let content = std::fs::read(path.unwrap().path()).unwrap();
        assert!(
            !content
                .windows(b"SCREEN_PRIVATE_CANARY_123".len())
                .any(|w| w == b"SCREEN_PRIVATE_CANARY_123")
        );
    }
}
