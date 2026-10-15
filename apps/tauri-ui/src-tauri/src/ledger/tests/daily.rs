use super::super::*;
#[test]
fn platform_management_persists_and_preserves_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let original = ledger
        .accounts()
        .unwrap()
        .into_iter()
        .find(|a| a.name == "微信")
        .unwrap();
    assert_eq!(original.category, "wallet");
    ledger
        .save_entry(&original.id, "2026-10-14", "12", "", true)
        .unwrap();
    ledger
        .update_platform(&original.id, "我的钱包", "wallet")
        .unwrap();
    let bank = ledger.create_platform("招商银行", "bank").unwrap();
    assert!(
        ledger
            .update_platform(&bank.id, "我的钱包", "bank")
            .is_err()
    );
    assert!(ledger.create_platform("无效", "invalid").is_err());
    assert!(ledger.update_platform(&bank.id, "", "bank").is_err());
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let accounts = ledger.accounts().unwrap();
    assert_eq!(
        accounts.iter().find(|a| a.id == bank.id).unwrap().category,
        "bank"
    );
    assert_eq!(
        accounts.iter().find(|a| a.id == original.id).unwrap().name,
        "我的钱包"
    );
    assert_eq!(
        ledger.day(&original.id, "2026-10-14").unwrap().income,
        "12.00"
    );
    assert_eq!(accounts.len(), 4);
}
#[test]
fn ipc_flow_preserves_daily_projection_and_data_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let id = ledger.accounts().unwrap()[0].id.clone();
    assert!(ledger.day(&id, "2026-01-04").unwrap().balance.is_none());
    ledger
        .save_entry(&id, "2026-01-04", "20", "工资", true)
        .unwrap();
    ledger
        .save_entry(&id, "2026-01-04", "5", "午饭", false)
        .unwrap();
    ledger.set_reference(&id, "2026-01-04", "100").unwrap();
    assert_eq!(
        ledger.day(&id, "2026-01-03").unwrap().balance.as_deref(),
        Some("85.00")
    );
    ledger
        .save_entry(&id, "2026-01-05", "30", "", true)
        .unwrap();
    ledger
        .save_entry(&id, "2026-01-05", "10", "", false)
        .unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let view = ledger.day(&id, "2026-01-05").unwrap();
    assert_eq!(view.balance.as_deref(), Some("120.00"));
    assert_eq!(view.entries.len(), 2);
    let other = ledger.create_account("银行卡").unwrap();
    assert!(
        ledger
            .day(&other.id, "2026-01-05")
            .unwrap()
            .entries
            .is_empty()
    );
    ledger.clear_reference(&id).unwrap();
    let view = ledger.day(&id, "2026-01-05").unwrap();
    assert!(view.balance.is_none());
    assert_eq!(view.net, "20.00");
}
#[test]
fn invalid_command_input_never_writes() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let id = ledger.accounts().unwrap()[0].id.clone();
    for amount in [
        "",
        "1.123",
        "-1",
        "0",
        "1e2",
        "NaN",
        "999999999999999999999",
    ] {
        assert!(
            ledger
                .save_entry(&id, "2026-01-04", amount, "", true)
                .is_err()
        );
    }
    assert!(
        ledger
            .save_entry("bad-id", "2026-01-04", "1", "", true)
            .is_err()
    );
    assert!(ledger.save_entry(&id, "2026-02-31", "1", "", true).is_err());
    assert!(ledger.day(&id, "2026-01-04").unwrap().entries.is_empty());
    ledger.set_reference(&id, "2026-01-04", "0").unwrap();
    assert_eq!(
        ledger.day(&id, "2026-01-04").unwrap().balance.as_deref(),
        Some("0.00")
    );
}
