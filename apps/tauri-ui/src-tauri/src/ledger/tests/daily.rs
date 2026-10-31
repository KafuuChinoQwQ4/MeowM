use super::super::*;
#[test]
fn deleted_categories_stay_deleted_after_reopening_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let categories = ledger.classification_state().unwrap().categories;
    assert!(
        !categories.is_empty(),
        "a new ledger should have default categories"
    );
    assert!(ledger.delete_category("invalid").is_err());
    assert!(
        ledger
            .delete_category(&uuid::Uuid::nil().to_string())
            .is_err()
    );
    assert_eq!(
        ledger.classification_state().unwrap().categories.len(),
        categories.len()
    );
    for category in categories {
        ledger.delete_category(&category.id).unwrap();
    }
    assert!(ledger.classification_state().unwrap().categories.is_empty());
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert!(ledger.classification_state().unwrap().categories.is_empty());
    assert_eq!(ledger.accounts().unwrap().len(), 3);
}

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
fn platform_deletion_persists_and_preserves_other_platforms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let platform = ledger.create_platform("临时钱包", "wallet").unwrap();
    let other = ledger.accounts().unwrap()[0].id.clone();
    ledger
        .save_entry(&other, "2026-10-14", "12", "", true)
        .unwrap();
    ledger
        .set_reference(&platform.id, "2026-10-14", "100")
        .unwrap();
    assert!(ledger.delete_platform("invalid").is_err());
    assert!(
        ledger
            .delete_platform(&uuid::Uuid::nil().to_string())
            .is_err()
    );
    assert!(ledger.delete_platform(&other).unwrap_err().contains("流水"));
    ledger.delete_platform(&platform.id).unwrap();
    assert!(ledger.delete_platform(&platform.id).is_err());
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.accounts().unwrap().len(), 3);
    assert!(
        !ledger
            .accounts()
            .unwrap()
            .iter()
            .any(|a| a.id == platform.id)
    );
    assert_eq!(ledger.day(&other, "2026-10-14").unwrap().income, "12.00");
    ledger.create_platform("临时钱包", "wallet").unwrap();
}

#[test]
fn deleting_all_platforms_does_not_reseed_platforms_or_categories_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    for category in ledger.classification_state().unwrap().categories {
        ledger.delete_category(&category.id).unwrap();
    }
    for platform in ledger.accounts().unwrap() {
        ledger.delete_platform(&platform.id).unwrap();
    }
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert!(ledger.accounts().unwrap().is_empty());
    assert!(ledger.classification_state().unwrap().categories.is_empty());
    ledger.create_platform("重新开始", "cash").unwrap();
    assert_eq!(ledger.accounts().unwrap().len(), 1);
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

#[test]
fn create_account_defaults_to_cny_and_preserves_foreign_currency() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("currency.sqlite3")).unwrap();
    let cny = ledger
        .create_platform_currency("人民币", "bank", None)
        .unwrap();
    let usd = ledger
        .create_platform_currency("美元", "bank", Some("USD"))
        .unwrap();
    let jpy = ledger
        .create_platform_currency("日元", "bank", Some("JPY"))
        .unwrap();
    assert_eq!(
        (cny.currency_code.as_str(), cny.currency_exponent),
        ("CNY", 2)
    );
    assert_eq!(
        (usd.currency_code.as_str(), usd.currency_exponent),
        ("USD", 2)
    );
    assert_eq!(
        (jpy.currency_code.as_str(), jpy.currency_exponent),
        ("JPY", 0)
    );
    assert!(
        ledger
            .create_platform_currency("无效", "bank", Some("XYZ"))
            .is_err()
    );
    ledger.update_platform(&usd.id, "美元新名", "bank").unwrap();
    let listed = ledger.accounts().unwrap();
    let saved = listed.iter().find(|a| a.id == usd.id).unwrap();
    assert_eq!(
        (saved.currency_code.as_str(), saved.currency_exponent),
        ("USD", 2)
    );
}

#[test]
fn foreign_day_entry_and_reference_use_account_precision() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("day-fx.sqlite3")).unwrap();
    let jpy = ledger
        .create_platform_currency("日元", "bank", Some("JPY"))
        .unwrap();
    assert!(
        ledger
            .save_entry(&jpy.id, "2026-10-14", "1.5", "", false)
            .is_err()
    );
    ledger
        .save_entry_with_rate(&jpy.id, "2026-10-14", "100", "", false, Some("200"))
        .unwrap();
    ledger.set_reference(&jpy.id, "2026-10-14", "1000").unwrap();
    let day = ledger.day(&jpy.id, "2026-10-14").unwrap();
    assert_eq!(day.expense, "0.50");
    assert_eq!(day.entries[0].currency_code, "JPY");
    assert_eq!(day.reference.unwrap().amount, "1000");
}
