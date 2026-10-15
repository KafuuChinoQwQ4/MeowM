use ledger_domain::TransactionKind;
use ledger_notifications::{
    NotificationEvent, NotificationSettings, ParseOutcome, parse_notification,
};

fn event(package: &str, body: &str) -> NotificationEvent {
    NotificationEvent {
        package_name: package.into(),
        title: "支付通知".into(),
        body: body.into(),
        notification_key: "key-1".into(),
        posted_at_ms: 1_760_000_000_000,
    }
}
fn settings(package: &str) -> NotificationSettings {
    NotificationSettings {
        package_name: package.into(),
        enabled: true,
        account_id: None,
    }
}
#[test]
fn parses_supported_payment_directions_and_chinese_amounts() {
    for package in ["com.tencent.mm", "com.eg.android.AlipayGphone"] {
        for (body, kind, amount) in [
            ("向星星咖啡付款12.30元", TransactionKind::Expense, 1230),
            ("收款到账￥20.01 商户：小店", TransactionKind::Income, 2001),
            ("退款到账8.50元", TransactionKind::Refund, 850),
            (
                "转账成功人民币一百二十三元四角五分",
                TransactionKind::Transfer,
                12345,
            ),
        ] {
            let candidate = match parse_notification(&event(package, body), &settings(package)) {
                ParseOutcome::Candidate(c) | ParseOutcome::NeedsConfirmation(c) => c,
                other => panic!("unexpected {other:?}"),
            };
            assert_eq!(candidate.kind, kind);
            assert_eq!(candidate.amount_minor, Some(amount));
        }
    }
}
#[test]
fn disabled_unknown_and_non_payment_notifications_do_not_create_candidates() {
    let mut disabled = settings("com.tencent.mm");
    disabled.enabled = false;
    assert_eq!(
        parse_notification(&event("com.tencent.mm", "付款12元"), &disabled),
        ParseOutcome::Ignored
    );
    assert_eq!(
        parse_notification(&event("unknown", "付款12元"), &settings("unknown")),
        ParseOutcome::Unsupported
    );
    assert_eq!(
        parse_notification(
            &event("com.tencent.mm", "张三：今晚吃饭吗"),
            &settings("com.tencent.mm")
        ),
        ParseOutcome::Ignored
    );
    assert_eq!(
        parse_notification(
            &event("com.tencent.mm", "支付优惠，满100减20元"),
            &settings("com.tencent.mm")
        ),
        ParseOutcome::Ignored
    );
}
#[test]
fn uncertain_fields_are_reviewable_without_inventing_amount_or_merchant() {
    for body in [
        "付款-12.30元",
        "付款-￥12.30",
        "付款- 12元",
        "付款负十二元",
        "付款1,234元",
        "付款12.3.4元",
        "付款12.345元",
        "付款12元，余额20元",
        "付款成功，内容已隐藏",
        "付款十二三元",
        "付款USD12.30",
        "付款9999999999999999999999元",
    ] {
        let ParseOutcome::NeedsConfirmation(c) =
            parse_notification(&event("com.tencent.mm", body), &settings("com.tencent.mm"))
        else {
            panic!("must review {body}")
        };
        assert_eq!(c.amount_minor, None, "{body}");
        assert_eq!(c.merchant, None);
    }
    let ParseOutcome::NeedsConfirmation(c) = parse_notification(
        &event("com.tencent.mm", "付款12.30元"),
        &settings("com.tencent.mm"),
    ) else {
        panic!("missing merchant")
    };
    assert_eq!(c.amount_minor, Some(1230));
    let ParseOutcome::Candidate(c) = parse_notification(
        &event("com.tencent.mm", "向星星咖啡付款12.30元"),
        &settings("com.tencent.mm"),
    ) else {
        panic!("known fields")
    };
    assert_eq!(c.merchant.as_deref(), Some("星星咖啡"));
}
