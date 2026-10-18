use ledger_domain::TransactionKind;
use ledger_notifications::{
    ScreenCaptureEvent, ScreenCaptureOutcome, ScreenCaptureParserSettings, parse_screen_capture,
};

fn event(package_name: &str, text: &str) -> ScreenCaptureEvent {
    ScreenCaptureEvent {
        event_id: "page1".into(),
        package_name: package_name.into(),
        recognized_text: text.into(),
        captured_at_ms: 1_760_000_000_000,
    }
}

#[test]
fn recognizes_successful_expense_from_screen_text() {
    let result = parse_screen_capture(
        &event("com.tencent.mm", "支付成功\n商户：星星咖啡\n¥12.30"),
        &ScreenCaptureParserSettings::enabled(),
    );
    let ScreenCaptureOutcome::Candidate(candidate) = result else {
        panic!("expected candidate");
    };
    assert_eq!(candidate.kind, TransactionKind::Expense);
    assert_eq!(candidate.amount_minor, Some(1230));
    assert_eq!(candidate.merchant.as_deref(), Some("星星咖啡"));
}

#[test]
fn incomplete_screen_text_requires_confirmation_without_inventing_fields() {
    let result = parse_screen_capture(
        &event("com.eg.android.AlipayGphone", "付款成功\n金额已隐藏"),
        &ScreenCaptureParserSettings::enabled(),
    );
    let ScreenCaptureOutcome::NeedsConfirmation(candidate) = result else {
        panic!("expected reviewable candidate");
    };
    assert_eq!(candidate.amount_minor, None);
    assert_eq!(candidate.merchant, None);
}

#[test]
fn disabled_or_unsupported_capture_is_ignored() {
    assert_eq!(
        parse_screen_capture(
            &event("com.tencent.mm", "支付成功 ¥12.30"),
            &ScreenCaptureParserSettings::disabled(),
        ),
        ScreenCaptureOutcome::Ignored
    );
    assert_eq!(
        parse_screen_capture(
            &event("com.example.other", "支付成功 ¥12.30"),
            &ScreenCaptureParserSettings::enabled(),
        ),
        ScreenCaptureOutcome::Unsupported
    );
}
