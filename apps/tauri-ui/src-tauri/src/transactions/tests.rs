use super::*;

fn request(account: &str, kind: &str, amount: &str) -> WriteTransaction {
    WriteTransaction {
        edit: PendingEdit {
            destination_id: None,
            id: String::new(),
            expected_version: 0,
            account_id: account.into(),
            date: "2026-10-14".into(),
            amount: amount.into(),
            kind: kind.into(),
            merchant: "测试商户".into(),
            description: "阶段二测试".into(),
            category_id: None,
            original_transaction_id: None,
        },
        destination_id: None,
    }
}

#[test]
fn new_transactions_auto_classify_known_unknown_and_custom_merchants() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let categories = ledger.categories().unwrap();
    let drinks = categories
        .iter()
        .find(|c| c.name == "饮品")
        .unwrap()
        .id
        .clone();
    let other = categories
        .iter()
        .find(|c| c.name == "其他")
        .unwrap()
        .id
        .clone();
    let mut known = request(&account, "expense", "12.00");
    known.edit.merchant = "喜茶中心店".into();
    let known_id = ledger.write_transaction(known).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&known_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        drinks
    );
    assert_eq!(
        ledger.day(&account, "2026-10-14").unwrap().entries[0]
            .category_name
            .as_deref(),
        Some("饮品")
    );

    let mut unknown = request(&account, "expense", "8.00");
    unknown.edit.merchant = "小众小店".into();
    let unknown_id = ledger.write_transaction(unknown).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&unknown_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        other
    );

    ledger
        .write_rule(crate::search_rules::RuleInput {
            id: None,
            priority: 0,
            merchant_contains: Some("小众小店".into()),
            description_contains: None,
            category_id: drinks.clone(),
            enabled: true,
        })
        .unwrap();
    let mut custom = request(&account, "expense", "9.00");
    custom.edit.merchant = "小众小店二号".into();
    let custom_id = ledger.write_transaction(custom).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&custom_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        drinks
    );
}

#[test]
fn all_platform_day_totals_count_refunds_and_transfers_once() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let accounts = ledger.accounts().unwrap();
    let source = &accounts[0].id;
    let destination = &accounts[1].id;
    ledger
        .write_transaction(request(source, "income", "10.10"))
        .unwrap();
    ledger
        .write_transaction(request(destination, "income", "20.20"))
        .unwrap();
    let expense = ledger
        .write_transaction(request(source, "expense", "15.10"))
        .unwrap();
    let mut refund = request(source, "refund", "3.00");
    refund.edit.original_transaction_id = Some(expense);
    ledger.write_transaction(refund).unwrap();
    let mut transfer = request(source, "transfer", "5.00");
    transfer.destination_id = Some(destination.clone());
    ledger.write_transaction(transfer).unwrap();
    ledger
        .save_entry(source, "2026-10-15", "99.99", "", true)
        .unwrap();
    let day = ledger.day_all("2026-10-14").unwrap();
    assert_eq!(
        (&*day.income, &*day.expense, &*day.net),
        ("30.30", "12.10", "18.20")
    );
    assert_eq!(day.entries.len(), 5);
    assert!(day.balance.is_none() && day.reference.is_none());
    assert_eq!(ledger.day(source, "2026-10-14").unwrap().income, "10.10");
    assert_eq!(ledger.day_all("2026-10-13").unwrap().net, "0.00");
}

#[test]
fn manual_transfer_and_refund_keep_links_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let accounts = ledger.accounts().unwrap();
    let source = &accounts[0].id;
    let destination = &accounts[1].id;
    let expense = ledger
        .write_transaction(request(source, "expense", "25.00"))
        .unwrap();
    let mut refund = request(source, "refund", "5.00");
    refund.edit.original_transaction_id = Some(expense.clone());
    let refund_id = ledger.write_transaction(refund).unwrap();
    let mut transfer = request(source, "transfer", "10.00");
    transfer.destination_id = Some(destination.clone());
    let transfer_id = ledger.write_transaction(transfer).unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(
        ledger
            .transaction_detail(&refund_id)
            .unwrap()
            .original_transaction_id
            .as_deref(),
        Some(expense.as_str())
    );
    assert!(
        ledger
            .transaction_detail(&expense)
            .unwrap()
            .linked_ids
            .contains(&refund_id)
    );
    assert_eq!(
        ledger
            .transaction_detail(&transfer_id)
            .unwrap()
            .destination_id
            .as_deref(),
        Some(destination.as_str())
    );
    let transfer = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&transfer_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(transfer.postings.len(), 2);
    assert_eq!(
        transfer.postings[0].delta.minor_units() + transfer.postings[1].delta.minor_units(),
        0
    );
}

#[test]
fn confirmed_transaction_kind_and_refund_parent_are_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let expense = ledger
        .write_transaction(request(&account, "expense", "25.00"))
        .unwrap();
    let mut edit = request(&account, "income", "25.00");
    edit.edit.id = expense.clone();
    assert!(ledger.write_transaction(edit).is_err());
    let mut refund = request(&account, "refund", "5.00");
    refund.edit.original_transaction_id = Some(expense.clone());
    let refund_id = ledger.write_transaction(refund).unwrap();
    let mut edit = request(&account, "refund", "5.00");
    edit.edit.id = refund_id.clone();
    edit.edit.original_transaction_id = Some(expense);
    assert!(ledger.write_transaction(edit).is_ok());
    let mut change_parent = request(&account, "refund", "5.00");
    change_parent.edit.id = refund_id;
    assert!(ledger.write_transaction(change_parent).is_err());
}

#[test]
fn editing_income_updates_totals_and_rejects_stale_versions() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let id = ledger
        .write_transaction(request(&account, "income", "25.00"))
        .unwrap();
    let mut edit = request(&account, "income", "30.00");
    edit.edit.id = id.clone();
    ledger.write_transaction(edit).unwrap();
    assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
    let mut stale = request(&account, "income", "40.00");
    stale.edit.id = id;
    assert!(ledger.write_transaction(stale).is_err());
    assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
}

#[test]
fn reversal_updates_day_totals_and_rejects_new_refunds() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let today = chrono::Local::now().date_naive().to_string();
    let mut expense = request(&account, "expense", "25.00");
    expense.edit.date = today.clone();
    let id = ledger.write_transaction(expense).unwrap();
    ledger.reverse_transaction(&id, "重复记账").unwrap();
    assert_eq!(ledger.day(&account, &today).unwrap().expense, "0.00");
    let mut refund = request(&account, "refund", "5.00");
    refund.edit.original_transaction_id = Some(id);
    assert!(ledger.write_transaction(refund).is_err());
}
