use super::*;
use ledger_app::LedgerRepository;
#[test]
fn real_sqlite_report_and_budget_commands_survive_reopen_and_reject_invalid_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    ledger
        .save_entry(&account, "2026-10-15", "12.30", "商户", false)
        .unwrap();
    let input = BudgetInput {
        id: None,
        from_date: "2026-10-01".into(),
        until_date: "2026-11-01".into(),
        account_id: Some(account.clone()),
        category_id: None,
        amount: "10.00".into(),
        custom_period: false,
        enabled: true,
    };
    let id = ledger.write_budget(input.clone()).unwrap();
    assert!(ledger.write_budget(input.clone()).is_err());
    let report = ledger
        .report(ReportInput {
            from_date: "2026-10-01".into(),
            until_date: "2026-11-01".into(),
            account_id: Some(account.clone()),
            category_id: None,
            merchant: None,
            merchant_offset: 0,
            merchant_limit: 20,
        })
        .unwrap();
    assert_eq!(report.expense, "12.30");
    assert_eq!(report.merchants[0].name, "未知商户");
    assert_eq!(report.trend[0].expense, "12.30");
    let reopened = Ledger::open(&path).unwrap();
    let views = reopened.budget_views().unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].spent, "12.30");
    assert_eq!(views[0].remaining, "-2.30");
    assert!(views[0].exceeded);
    reopened.archive_budget(&id).unwrap();
    assert!(!reopened.budget_views().unwrap()[0].enabled);
    let mut bad = input;
    bad.id = Some(id);
    bad.account_id = Some(uuid::Uuid::new_v4().to_string());
    assert!(reopened.write_budget(bad).is_err());
    assert_eq!(
        reopened.repo().unwrap().list_transactions().unwrap().len(),
        1
    );
}
#[test]
fn malformed_report_input_fails_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    for (from, until, limit) in [
        ("bad", "2026-11-01", 20),
        ("2026-11-01", "2026-10-01", 20),
        ("2026-10-01", "2026-11-01", 201),
    ] {
        assert!(
            ledger
                .report(ReportInput {
                    from_date: from.into(),
                    until_date: until.into(),
                    account_id: None,
                    category_id: None,
                    merchant: None,
                    merchant_offset: 0,
                    merchant_limit: limit
                })
                .is_err()
        );
    }
}
