use ledger_backup::{BackupError, BackupService};
use ledger_storage::sqlite::SqliteLedgerRepository;
#[test]
fn password_and_tamper_failure_leave_original_intact() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let before = repo.snapshot().unwrap().fingerprint().unwrap();
    let bytes = BackupService::create(&repo, "test-only-pass").unwrap();
    assert!(!bytes.windows(8).any(|w| w == b"accounts"));
    assert_ne!(
        bytes,
        BackupService::create(&repo, "test-only-pass").unwrap()
    );
    assert!(matches!(
        BackupService::inspect(&bytes, "wrong-password"),
        Err(BackupError::Authentication)
    ));
    let mut corrupted = bytes.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(BackupService::inspect(&corrupted, "test-only-pass").is_err());
    let inspected = BackupService::inspect(&bytes, "test-only-pass").unwrap();
    BackupService::restore(&repo, &inspected, before).unwrap();
    assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
    assert!(BackupService::inspect(b"broken", "test-only-pass").is_err());
    assert!(BackupService::create(&repo, "short").is_err());
}
