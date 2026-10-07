use ledger_android_bridge::{CaptureResult, NotificationProcessor};
use ledger_app::ImportRepository;
use ledger_domain::TransactionStatus;
use ledger_notifications::{CaptureSettings, NotificationEvent, NotificationSettings};
use ledger_storage::sqlite::SqliteLedgerRepository;

fn payment() -> NotificationEvent {
    NotificationEvent {
        package_name: "com.tencent.mm".into(),
        title: "微信支付".into(),
        body: "微信支付凭证\n支付金额：¥12.34\n商户名称：测试咖啡\n交易时间：2026-10-07 10:30:00"
            .into(),
        notification_id: "key|tag|17".into(),
        posted_at_epoch_ms: 1_791_361_800_000,
        capture_settings: CaptureSettings {
            capture_enabled: true,
        },
    }
}

#[test]
fn first_capture_persists_a_pending_record_and_repeated_callback_is_idempotent() {
    let repository = SqliteLedgerRepository::in_memory().unwrap();
    let processor = NotificationProcessor::new(repository);
    let settings = NotificationSettings {
        enabled: true,
        wechat_enabled: true,
        alipay_enabled: true,
    };

    assert_eq!(
        processor.capture(payment(), settings).unwrap(),
        CaptureResult::Pending
    );
    assert_eq!(
        processor.capture(payment(), settings).unwrap(),
        CaptureResult::Pending
    );
    let records = processor.repository().list_import_records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].transaction.status, TransactionStatus::Pending);
    assert!(records[0].transaction.postings.is_empty());
}

#[test]
fn disabled_source_is_ignored_without_a_ledger_write() {
    let repository = SqliteLedgerRepository::in_memory().unwrap();
    let processor = NotificationProcessor::new(repository);
    let settings = NotificationSettings {
        enabled: true,
        wechat_enabled: false,
        alipay_enabled: true,
    };

    assert_eq!(
        processor.capture(payment(), settings).unwrap(),
        CaptureResult::Ignored
    );
    assert!(
        processor
            .repository()
            .list_import_records()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn retry_after_process_restart_cannot_duplicate_the_notification_batch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let settings = NotificationSettings {
        enabled: true,
        wechat_enabled: true,
        alipay_enabled: true,
    };
    let first = NotificationProcessor::new(SqliteLedgerRepository::open(&path).unwrap());
    assert_eq!(
        first.capture(payment(), settings).unwrap(),
        CaptureResult::Pending
    );
    drop(first);

    let restarted = NotificationProcessor::new(SqliteLedgerRepository::open(&path).unwrap());
    assert_eq!(
        restarted.capture(payment(), settings).unwrap(),
        CaptureResult::Pending
    );
    assert_eq!(
        restarted.repository().list_import_records().unwrap().len(),
        1
    );
}

#[test]
fn refund_notification_is_saved_for_review_before_an_original_is_selected() {
    use ledger_app::MobileRepository;
    let processor = NotificationProcessor::new(SqliteLedgerRepository::in_memory().unwrap());
    let mut event = payment();
    event.body = "退款成功\n退款金额：¥5.00\n原交易单号：original-1\n商户名称：测试咖啡\n交易时间：2026-10-07 10:30:00".into();
    assert_eq!(
        processor
            .capture(
                event,
                NotificationSettings {
                    enabled: true,
                    wechat_enabled: true,
                    alipay_enabled: true
                }
            )
            .unwrap(),
        CaptureResult::Pending
    );
    let records = processor.repository().list_import_records().unwrap();
    assert_eq!(
        records[0].transaction.kind,
        ledger_domain::TransactionKind::Refund
    );
    let details = processor
        .repository()
        .pending_details(records[0].transaction.id)
        .unwrap()
        .unwrap();
    assert_eq!(details.amount.unwrap().minor_units(), 500);
    assert_eq!(details.source, "wechat");
}

#[test]
fn reused_notification_slot_keeps_later_payments_and_exact_replays_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let settings = NotificationSettings {
        enabled: true,
        wechat_enabled: true,
        alipay_enabled: true,
    };
    let first = NotificationProcessor::new(SqliteLedgerRepository::open(&path).unwrap());
    first.capture(payment(), settings).unwrap();
    let mut later = payment();
    later.posted_at_epoch_ms += 60_000;
    later.body = later.body.replace("12.34", "56.78");
    first.capture(later.clone(), settings).unwrap();
    assert_eq!(first.repository().list_import_records().unwrap().len(), 2);
    drop(first);
    let restarted = NotificationProcessor::new(SqliteLedgerRepository::open(&path).unwrap());
    restarted.capture(payment(), settings).unwrap();
    restarted.capture(later, settings).unwrap();
    let records = restarted.repository().list_import_records().unwrap();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|r| r.transaction.status == TransactionStatus::Pending)
    );
}

#[test]
fn provider_transaction_id_deduplicates_across_notification_metadata_updates() {
    for package in ["com.tencent.mm", "com.eg.android.AlipayGphone"] {
        let processor = NotificationProcessor::new(SqliteLedgerRepository::in_memory().unwrap());
        let settings = NotificationSettings {
            enabled: true,
            wechat_enabled: true,
            alipay_enabled: true,
        };
        let mut original = payment();
        original.package_name = package.into();
        original.body.push_str("\n交易号：PROVIDER_PRIVATE_ID");
        processor.capture(original.clone(), settings).unwrap();
        original.notification_id = "another-slot".into();
        original.posted_at_epoch_ms += 60_000;
        original.title = "更新的支付通知".into();
        processor.capture(original, settings).unwrap();
        let records = processor.repository().list_import_records().unwrap();
        assert_eq!(records.len(), 1, "{package}");
        let identity = records[0]
            .evidence
            .external_transaction_id
            .as_ref()
            .unwrap();
        assert_eq!(identity.len(), 64);
        assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!identity.contains("PROVIDER_PRIVATE_ID"));
    }
}
