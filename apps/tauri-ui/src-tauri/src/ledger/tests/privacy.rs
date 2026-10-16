use crate::ledger::Ledger;

#[test]
fn locked_ledger_denies_every_normal_access_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    ledger
        .privacy
        .borrow_mut()
        .enable_lock(&ledger.repo().unwrap(), "test-only-password")
        .unwrap();
    ledger.require_unlocked().unwrap();
    ledger.privacy.borrow_mut().lock_now();
    assert!(ledger.require_unlocked().is_err());
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert!(ledger.require_unlocked().is_err());
    ledger
        .privacy
        .borrow_mut()
        .unlock("test-only-password")
        .unwrap();
    ledger.require_unlocked().unwrap();
}
