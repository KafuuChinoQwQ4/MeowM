use chrono::{TimeZone, Utc};
use ledger_domain::{TransactionKind, TransactionStatus};
use ledger_notifications::{
    CaptureSettings, NotificationEvent, NotificationSettings, ParseOutcome, parse_notification,
};

fn event(package_name: &str, body: &str, notification_id: &str) -> NotificationEvent {
    NotificationEvent {
        package_name: package_name.into(),
        title: "支付通知".into(),
        body: body.into(),
        notification_id: notification_id.into(),
        posted_at_epoch_ms: Utc
            .with_ymd_and_hms(2026, 10, 1, 4, 30, 0)
            .unwrap()
            .timestamp_millis(),
        capture_settings: CaptureSettings {
            capture_enabled: true,
        },
    }
}

fn settings() -> NotificationSettings {
    NotificationSettings {
        enabled: true,
        wechat_enabled: true,
        alipay_enabled: true,
    }
}

#[test]
fn wechat_expense_preserves_minor_units_and_never_confirms_candidate() {
    let input = event(
        "com.tencent.mm",
        "微信支付凭证\n支付金额：¥12.34\n商户：晨光便利店\n支付时间：2026-10-01 12:30:00\n交易单号：wx-1",
        "notification-1",
    );
    let ParseOutcome::Candidate(candidate) = parse_notification(&input, &settings()) else {
        panic!("complete WeChat expense should be a candidate");
    };
    assert_eq!(candidate.amount.as_ref().unwrap().minor_units(), 1234);
    assert_eq!(candidate.currency.as_deref(), Some("CNY"));
    assert_eq!(candidate.direction, Some(TransactionKind::Expense));
    assert_eq!(candidate.merchant.as_deref(), Some("晨光便利店"));
    assert_eq!(candidate.status, TransactionStatus::Pending);
}

#[test]
fn chinese_currency_markers_and_grouped_amounts_use_integer_minor_units() {
    let input = event(
        "com.tencent.mm",
        "微信支付
支付金额：人民币1,234.56元
商户：杂货店
支付时间：2026-10-01 12:30:00",
        "chinese-currency",
    );
    let ParseOutcome::Candidate(candidate) = parse_notification(&input, &settings()) else {
        panic!("well-formed Chinese currency amount should be a candidate");
    };
    assert_eq!(candidate.amount.as_ref().unwrap().minor_units(), 123456);
}

#[test]
fn alipay_income_and_refund_are_distinguished() {
    let income = event(
        "com.eg.android.AlipayGphone",
        "收款成功\n金额：￥30.00\n付款方：小王\n交易时间：2026-10-01 12:30:00\n交易号：ali-1",
        "a",
    );
    let ParseOutcome::Candidate(income) = parse_notification(&income, &settings()) else {
        panic!("expected income candidate");
    };
    assert_eq!(income.direction, Some(TransactionKind::Income));

    let refund = event(
        "com.eg.android.AlipayGphone",
        "退款成功\n退款金额：￥8.20\n商户：书店\n交易时间：2026-10-01 12:30:00\n交易号：ali-2",
        "b",
    );
    let ParseOutcome::NeedsConfirmation(refund) = parse_notification(&refund, &settings()) else {
        panic!("refund without source mapping needs confirmation");
    };
    assert_eq!(refund.amount.as_ref().unwrap().minor_units(), 820);
    assert_eq!(refund.direction, Some(TransactionKind::Refund));
    assert!(refund.warnings.iter().any(
        |warning| warning.code == ledger_notifications::WarningCode::MissingOriginalTransaction
    ));
}

#[test]
fn transfer_notice_is_pending_and_account_mapping_is_uncertain() {
    let input = event(
        "com.tencent.mm",
        "转账到账通知\n金额：￥20.00\n转账时间：2026-10-01 12:30:00\n交易单号：wx-transfer",
        "t",
    );
    let ParseOutcome::NeedsConfirmation(candidate) = parse_notification(&input, &settings()) else {
        panic!("transfer needs account mapping confirmation");
    };
    assert_eq!(candidate.direction, Some(TransactionKind::Transfer));
    assert_eq!(candidate.status, TransactionStatus::Pending);
}

#[test]
fn repeated_notification_identity_is_deterministic_and_distinguishes_notification_ids() {
    let a = event("com.tencent.mm", "支付成功 金额 ¥12.34", "same");
    let b = event("com.tencent.mm", "支付成功 金额 ¥12.34", "other");
    let identity = |event: &NotificationEvent| match parse_notification(event, &settings()) {
        ParseOutcome::Candidate(candidate) | ParseOutcome::NeedsConfirmation(candidate) => {
            candidate.notification_identity.clone()
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(identity(&a), identity(&a));
    assert_ne!(identity(&a), identity(&b));
}

#[test]
fn unknown_package_and_disabled_sources_are_ignored() {
    assert!(matches!(
        parse_notification(&event("evil.app", "支付金额 ¥1.00", "x"), &settings()),
        ParseOutcome::Unsupported
    ));
    let mut prefs = settings();
    prefs.wechat_enabled = false;
    assert!(matches!(
        parse_notification(&event("com.tencent.mm", "支付金额 ¥1.00", "x"), &prefs),
        ParseOutcome::Ignored
    ));
    let mut prefs = settings();
    prefs.enabled = false;
    assert!(matches!(
        parse_notification(
            &event("com.eg.android.AlipayGphone", "金额 ￥1.00", "x"),
            &prefs
        ),
        ParseOutcome::Ignored
    ));
}

#[test]
fn missing_fields_and_ambiguous_negative_amount_require_confirmation() {
    let incomplete = event("com.tencent.mm", "支付成功\n金额：￥2.00", "missing");
    let ParseOutcome::NeedsConfirmation(candidate) = parse_notification(&incomplete, &settings())
    else {
        panic!("missing date requires confirmation");
    };
    assert!(candidate.occurred_at.is_none());

    let negative = event(
        "com.eg.android.AlipayGphone",
        "支付成功\n金额：-￥2.00\n交易时间：2026-10-01 12:30:00",
        "negative",
    );
    let ParseOutcome::NeedsConfirmation(candidate) = parse_notification(&negative, &settings())
    else {
        panic!("negative notice amount is ambiguous");
    };
    assert!(candidate.amount.is_none());
}

#[test]
fn event_debug_redacts_body_and_digest_is_stable() {
    let input = event("com.tencent.mm", "SECRET raw notification body", "safe-id");
    assert!(!format!("{input:?}").contains("SECRET"));
    assert!(!format!("{input:?}").contains("支付通知"));
    let ParseOutcome::NeedsConfirmation(candidate) = parse_notification(&input, &settings()) else {
        panic!("missing fields need confirmation");
    };
    let same_input = event("com.tencent.mm", "SECRET raw notification body", "safe-id");
    let ParseOutcome::NeedsConfirmation(same_candidate) =
        parse_notification(&same_input, &settings())
    else {
        panic!("identical event should normalize the same way");
    };
    assert_eq!(candidate.content_digest, same_candidate.content_digest);
    assert!(!format!("{candidate:?}").contains("SECRET"));
}

#[test]
fn outcome_debug_redacts_merchant_account_and_external_token_fields() {
    let input = event(
        "com.tencent.mm",
        "支付成功\n支付金额：￥12.34\n商户：MARKER_MERCHANT\n支付方式：MARKER_ACCOUNT\n支付时间：2026-10-01 12:30:00\n交易单号：TOKEN_SECRET_MARKER",
        "debug-redaction",
    );
    let outcome = parse_notification(&input, &settings());
    let debug = format!("{outcome:?}");
    for sensitive in ["MARKER_MERCHANT", "MARKER_ACCOUNT", "TOKEN_SECRET_MARKER"] {
        assert!(
            !debug.contains(sensitive),
            "debug leaked {sensitive}: {debug}"
        );
    }
    assert!(debug.contains("Candidate"));
    assert!(debug.contains("digest"));
}

#[test]
fn reused_slot_identity_uses_posted_time_and_content_without_an_external_id() {
    let input = event("com.tencent.mm", "支付成功\n金额：￥2.00", "slot");
    let identity = |input: &NotificationEvent| match parse_notification(input, &settings()) {
        ParseOutcome::Candidate(candidate) | ParseOutcome::NeedsConfirmation(candidate) => {
            candidate.notification_identity
        }
        other => panic!("unexpected outcome: {other:?}"),
    };
    let mut later = input.clone();
    later.posted_at_epoch_ms += 60_000;
    assert_ne!(identity(&input), identity(&later));
    let mut changed = input.clone();
    changed.body = "支付成功\n金额：￥3.00".into();
    assert_ne!(identity(&input), identity(&changed));
}

#[test]
fn original_transaction_reference_is_not_a_refund_transaction_identity() {
    for package in ["com.tencent.mm", "com.eg.android.AlipayGphone"] {
        let input = event(
            package,
            "退款成功\n金额：￥2.00\n原交易单号：same-parent",
            "slot",
        );
        let ParseOutcome::NeedsConfirmation(first) = parse_notification(&input, &settings()) else {
            panic!("refund requires confirmation")
        };
        assert!(first.external_transaction_id.is_none());
        assert_eq!(
            first.original_external_transaction_id.as_deref(),
            Some("same-parent")
        );
        let mut later = input.clone();
        later.posted_at_epoch_ms += 60_000;
        let ParseOutcome::NeedsConfirmation(second) = parse_notification(&later, &settings())
        else {
            panic!("refund requires confirmation")
        };
        assert_ne!(first.notification_identity, second.notification_identity);
    }
}
