use super::*;
use ledger_app::{LedgerRepository, ports::ScreenCaptureRepository};
#[test]
fn native_boundary_requires_live_session_and_real_privacy_lock() {
    let dir = tempfile::tempdir().unwrap();
    let l = crate::ledger::Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    l.repo()
        .unwrap()
        .save_screen_settings(ledger_app::notifications::NotificationSettings {
            package_name: "com.tencent.mm".into(),
            enabled: true,
            account_id: None,
        })
        .unwrap();
    let payload = r#"{"event_id":"session:page","package_name":"com.tencent.mm","recognized_text":"支付成功\n商户：小店\n¥12.30","captured_at_ms":1760000000000}"#;
    assert!(capture(&l, "session", payload).contains("stopped"));
    l.capture_session.replace(Some("session".into()));
    assert!(capture(&l, "session", payload).contains("needs_confirmation"));
    assert!(capture(&l, "session", payload).contains("duplicate"));
    l.privacy
        .borrow_mut()
        .enable_lock(&l.repo().unwrap(), "test-only-pass")
        .unwrap();
    l.privacy.borrow_mut().lock_now();
    assert!(capture(&l, "session", payload).contains("stopped"));
    l.privacy.borrow_mut().unlock("test-only-pass").unwrap();
    assert!(capture(&l, "session", payload).contains("stopped"));
    assert_eq!(l.repo().unwrap().list_transactions().unwrap().len(), 1);
}
