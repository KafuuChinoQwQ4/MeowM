use ledger_app::LedgerRepository;
use ledger_domain::{Account, AccountKind, CurrencyCode};
use ledger_storage::sqlite::SqliteLedgerRepository;

#[test]
fn restore_replaces_atomically_and_rejects_changes_since_preview() {
    let source = SqliteLedgerRepository::in_memory().unwrap();
    let account = Account::new(
        "备份账户",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    source.insert_account(account.clone()).unwrap();
    let snapshot = source.snapshot().unwrap();
    let restored = SqliteLedgerRepository::from_snapshot(&snapshot).unwrap();
    assert_eq!(
        restored.list_accounts().unwrap()[0].display_name,
        "备份账户"
    );
    let target = SqliteLedgerRepository::in_memory().unwrap();
    let old = target.snapshot().unwrap().fingerprint().unwrap();
    target
        .insert_account(Account::new(
            "稍后新增",
            AccountKind::Asset,
            CurrencyCode::new("CNY", 2).unwrap(),
        ))
        .unwrap();
    assert!(target.replace_snapshot(&snapshot, old).is_err());
    assert_eq!(target.list_accounts().unwrap()[0].display_name, "稍后新增");
    let fresh = target.snapshot().unwrap().fingerprint().unwrap();
    target.replace_snapshot(&snapshot, fresh).unwrap();
    assert_eq!(target.list_accounts().unwrap()[0].id, account.id);
    let fresh = target.snapshot().unwrap().fingerprint().unwrap();
    target.replace_snapshot(&snapshot, fresh).unwrap();
    assert_eq!(target.list_accounts().unwrap().len(), 1);
}
