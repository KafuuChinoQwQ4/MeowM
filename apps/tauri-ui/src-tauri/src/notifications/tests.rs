use super::*;
use ledger_app::{LedgerRepository, ports::NotificationRepository};
use ledger_domain::{Account, AccountKind, CurrencyCode, TransactionStatus};
#[test]
fn json_boundary_is_private_bounded_and_restart_safe() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    let repo = ledger_storage::sqlite::SqliteLedgerRepository::open(&path).unwrap();
    let account = Account::new(
        "微信",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    repo.save_notification_settings(ledger_app::notifications::NotificationSettings {
        package_name: "com.tencent.mm".into(),
        enabled: true,
        account_id: Some(account.id),
    })
    .unwrap();
    let event = r#"{"package_name":"com.tencent.mm","title":"微信支付","body":"向店铺付款12元，敏感正文不可返回","notification_key":"secret-key","posted_at_ms":1760000000000}"#;
    let first = capture_json(&path, event);
    assert!(first.contains("pending"));
    assert!(!first.contains("secret-key"));
    assert!(!first.contains("敏感正文"));
    assert!(capture_json(&path, event).contains("duplicate"));
    assert_eq!(
        capture_json(&path, "not-json"),
        r#"{"code":"error","transaction_id":null}"#
    );
    assert_eq!(
        capture_json(&path, &"x".repeat(65537)),
        r#"{"code":"error","transaction_id":null}"#
    );
    assert_eq!(
        repo.list_transactions().unwrap()[0].status,
        TransactionStatus::Pending
    );
}
#[test]
fn pending_transfer_can_be_completed_without_becoming_income_or_expense() {
    use crate::ledger::PendingEdit;
    let temp = tempfile::tempdir().unwrap();
    let ledger = crate::ledger::Ledger::open(&temp.path().join("ledger.sqlite3")).unwrap();
    let repo = ledger.repo().unwrap();
    let accounts = repo.list_accounts().unwrap();
    repo.save_notification_settings(ledger_app::notifications::NotificationSettings {
        package_name: "com.tencent.mm".into(),
        enabled: true,
        account_id: Some(accounts[0].id),
    })
    .unwrap();
    let event = ledger_notifications::NotificationEvent {
        package_name: "com.tencent.mm".into(),
        title: "微信".into(),
        body: "转账成功12元".into(),
        notification_key: "transfer".into(),
        posted_at_ms: 1760000000000,
    };
    let result = ledger_notifications::capture_notification(&repo, &event).unwrap();
    let id = result.transaction_id.unwrap();
    let edit = PendingEdit {
        id: id.as_uuid().to_string(),
        expected_version: 0,
        account_id: accounts[0].id.as_uuid().to_string(),
        destination_id: Some(accounts[1].id.as_uuid().to_string()),
        date: "2025-10-09".into(),
        amount: "12.00".into(),
        kind: "transfer".into(),
        merchant: "".into(),
        description: "核对后转账".into(),
        category_id: None,
        original_transaction_id: None,
    };
    ledger.edit_pending(edit.clone()).unwrap();
    let page = ledger
        .page(
            serde_json::from_value(serde_json::json!({
                "status": "pending", "offset": 0, "limit": 20
            }))
            .unwrap(),
        )
        .unwrap_or_else(|e| panic!("{}", e.message));
    let page = serde_json::to_value(page).unwrap();
    assert_eq!(
        page["items"][0]["destination_id"],
        accounts[1].id.as_uuid().to_string()
    );
    ledger
        .confirm_pending(
            vec![id.as_uuid().to_string()],
            vec![PendingEdit {
                expected_version: 1,
                ..edit
            }],
        )
        .unwrap();
    let tx = repo.get_transaction(id).unwrap().unwrap();
    assert_eq!(tx.status, TransactionStatus::Confirmed);
    assert_eq!(tx.postings.len(), 2);
    assert!(ledger_domain::report_effect(&tx).is_none());
}
