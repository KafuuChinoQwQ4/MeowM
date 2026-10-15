use super::*;

fn request(account: &str, kind: &str, amount: &str) -> WriteTransaction {
    WriteTransaction {
        edit: PendingEdit {
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
