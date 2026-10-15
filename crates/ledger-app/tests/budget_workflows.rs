use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use ledger_app::{
    BudgetPlan, BudgetService, CalendarRange, ClassificationRuleRepository, LedgerRepository,
    ReminderService, ReportQuery, ReportService, ScopedReportQuery,
};
use ledger_domain::{
    Account, AccountKind, BudgetId, Category, CategoryId, CurrencyCode, Money, Posting,
    Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn fixture() -> (SqliteLedgerRepository, Account, Category) {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    account.opening_balance = Some(Money::new(10000, account.currency.clone()));
    account.opening_balance_effective_at = Some(utc("2026-10-01T00:00:00Z"));
    repo.insert_account(account.clone()).unwrap();
    let category = Category {
        id: CategoryId::new(),
        name: "餐饮".into(),
        enabled: true,
    };
    repo.insert_category(category.clone()).unwrap();
    (repo, account, category)
}
fn tx(
    account: &Account,
    category: Option<CategoryId>,
    kind: TransactionKind,
    amount: i64,
    status: TransactionStatus,
) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        occurred_at: utc("2026-10-15T12:00:00Z"),
        kind,
        status,
        postings: vec![Posting {
            account_id: account.id,
            delta: Money::new(
                if kind == TransactionKind::Expense {
                    -amount
                } else {
                    amount
                },
                account.currency.clone(),
            ),
        }],
        category_id: category,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(amount, account.currency.clone())),
        merchant: Some("面包店".into()),
        description: None,
    }
}
fn range() -> ReportQuery {
    ReportQuery {
        from_utc_inclusive: utc("2026-10-01T00:00:00Z"),
        until_utc_exclusive: utc("2026-11-01T00:00:00Z"),
        month_boundaries_utc: vec![],
    }
}
fn budget(account: &Account, category: &Category, amount: i64) -> BudgetPlan {
    BudgetPlan {
        id: BudgetId::new(),
        from_date: day("2026-10-01"),
        until_date: day("2026-11-01"),
        category_id: Some(category.id),
        account_id: Some(account.id),
        amount_cny_minor: amount,
        enabled: true,
        custom_period: false,
    }
}
#[test]
fn reports_and_category_budgets_inherit_refund_scope_and_ignore_pending_and_transfer() {
    let (repo, account, category) = fixture();
    let expense = tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        3000,
        TransactionStatus::Confirmed,
    );
    repo.insert_transaction(expense.clone()).unwrap();
    let mut refund = tx(
        &account,
        None,
        TransactionKind::Refund,
        500,
        TransactionStatus::Confirmed,
    );
    refund.original_transaction_id = Some(expense.id);
    refund.merchant = None;
    repo.insert_transaction(refund).unwrap();
    repo.insert_transaction(tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        7000,
        TransactionStatus::Pending,
    ))
    .unwrap();
    let other = Account::new("银行", AccountKind::Asset, account.currency.clone());
    repo.insert_account(other.clone()).unwrap();
    let mut transfer = tx(
        &account,
        None,
        TransactionKind::Transfer,
        5000,
        TransactionStatus::Confirmed,
    );
    transfer.report_amount_cny = None;
    transfer.postings[0].delta = Money::new(-5000, account.currency.clone());
    transfer.postings.push(Posting {
        account_id: other.id,
        delta: Money::new(5000, other.currency),
    });
    repo.insert_transaction(transfer).unwrap();
    let report = ReportService::new(repo)
        .summarize_scoped(ScopedReportQuery {
            range: range(),
            account_id: Some(account.id),
            category_id: Some(category.id),
            merchant: None,
            merchant_offset: 0,
            merchant_limit: 20,
        })
        .unwrap();
    assert_eq!(report.summary.expense_cny_minor, 2500);
    assert_eq!(report.summary.income_cny_minor, 0);
    assert_eq!(report.categories[0].expense_cny_minor, 2500);
    assert_eq!(report.merchants[0].expense_cny_minor, 2500);
}
#[test]
fn category_budget_progress_is_persistent_archivable_and_reminders_are_stable() {
    let (repo, account, category) = fixture();
    repo.insert_transaction(tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        800,
        TransactionStatus::Confirmed,
    ))
    .unwrap();
    let service = BudgetService::new(repo);
    let plan = budget(&account, &category, 1000);
    service.create(plan.clone()).unwrap();
    let progress = service
        .progress(plan.id, FixedOffset::east_opt(0).unwrap())
        .unwrap();
    assert_eq!(progress.spent_cny_minor, 800);
    assert_eq!(progress.remaining_cny_minor, 200);
    assert_eq!(progress.percent, 80);
    let reminder = ReminderService::new(service.repository())
        .due(day("2026-10-16"), FixedOffset::east_opt(0).unwrap())
        .unwrap();
    assert_eq!(reminder.len(), 1);
    assert_eq!(reminder[0].level, "warning");
    assert_eq!(
        reminder,
        ReminderService::new(service.repository())
            .due(day("2026-10-16"), FixedOffset::east_opt(0).unwrap())
            .unwrap()
    );
    service.archive(plan.id).unwrap();
    assert_eq!(
        service
            .progress(plan.id, FixedOffset::east_opt(0).unwrap())
            .unwrap()
            .spent_cny_minor,
        800
    );
    assert!(
        ReminderService::new(service.repository())
            .due(day("2026-10-16"), FixedOffset::east_opt(0).unwrap())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn budget_rejects_disabled_category_and_duplicate_scope_without_writes() {
    let (repo, account, mut category) = fixture();
    category.enabled = false;
    repo.update_category(category.clone()).unwrap();
    let service = BudgetService::new(repo);
    assert!(service.create(budget(&account, &category, 100)).is_err());
    assert!(service.list().unwrap().is_empty());
    category.enabled = true;
    service
        .repository()
        .update_category(category.clone())
        .unwrap();
    service.create(budget(&account, &category, 100)).unwrap();
    assert!(service.create(budget(&account, &category, 200)).is_err());
    assert_eq!(service.list().unwrap().len(), 1);
}
#[test]
fn report_filters_and_merchant_pagination_keep_total_and_balance_snapshot() {
    let (repo, account, category) = fixture();
    repo.insert_transaction(tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        3000,
        TransactionStatus::Confirmed,
    ))
    .unwrap();
    let report = ReportService::new(repo)
        .summarize_scoped(ScopedReportQuery {
            range: range(),
            account_id: Some(account.id),
            category_id: Some(category.id),
            merchant: None,
            merchant_offset: 0,
            merchant_limit: 1,
        })
        .unwrap();
    assert_eq!(report.summary.expense_cny_minor, 3000);
    assert_eq!(report.categories[0].name, "餐饮");
    assert_eq!(report.merchants[0].name, "面包店");
    assert_eq!(report.merchant_total, 1);
    assert_eq!(report.balances[0].amount_cny_minor, Some(7000));
}
#[test]
fn percentage_uses_wide_integer_and_zero_budget_has_defined_exceeded_state() {
    let (repo, account, category) = fixture();
    repo.insert_transaction(tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        i64::MAX,
        TransactionStatus::Confirmed,
    ))
    .unwrap();
    let service = BudgetService::new(repo);
    let plan = budget(&account, &category, 1);
    service.create(plan.clone()).unwrap();
    assert_eq!(
        service
            .progress(plan.id, FixedOffset::east_opt(0).unwrap())
            .unwrap()
            .percent,
        922337203685477580700_u128
    );
    let mut plan = plan;
    plan.amount_cny_minor = 0;
    service.update(plan.clone()).unwrap();
    let p = service
        .progress(plan.id, FixedOffset::east_opt(0).unwrap())
        .unwrap();
    assert!(p.exceeded);
    assert_eq!(p.percent, 100);
}
#[test]
fn local_month_range_uses_local_midnight_and_half_open_end() {
    let zone = FixedOffset::east_opt(8 * 3600).unwrap();
    let r = CalendarRange::new(day("2026-10-01"), day("2026-11-01"), zone).unwrap();
    assert_eq!(r.query.from_utc_inclusive, utc("2026-09-30T16:00:00Z"));
    assert_eq!(r.query.until_utc_exclusive, utc("2026-10-31T16:00:00Z"));
    assert_eq!(r.query.month_boundaries_utc.len(), 1);
    assert!(CalendarRange::new(day("2026-11-01"), day("2026-10-01"), zone).is_err());
    assert_eq!(
        zone.with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
            .unwrap()
            .with_timezone(&Utc),
        r.query.from_utc_inclusive
    );
}
#[test]
fn reversal_of_uncategorized_refund_restores_original_category_budget() {
    let (repo, account, category) = fixture();
    let expense = tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        3000,
        TransactionStatus::Confirmed,
    );
    repo.insert_transaction(expense.clone()).unwrap();
    let mut refund = tx(
        &account,
        None,
        TransactionKind::Refund,
        500,
        TransactionStatus::Confirmed,
    );
    refund.original_transaction_id = Some(expense.id);
    refund.merchant = None;
    repo.insert_transaction(refund.clone()).unwrap();
    repo.reverse_with_reason(refund.id, utc("2026-10-16T12:00:00Z"), "退错退款")
        .unwrap();
    let service = BudgetService::new(repo);
    let plan = budget(&account, &category, 4000);
    service.create(plan.clone()).unwrap();
    assert_eq!(
        service
            .progress(plan.id, FixedOffset::east_opt(0).unwrap())
            .unwrap()
            .spent_cny_minor,
        3000
    );
}
#[test]
fn custom_budget_and_disabled_state_survive_database_reopen() {
    let path = std::env::temp_dir().join(format!("meowm-budget-{}.sqlite3", uuid::Uuid::new_v4()));
    let (repo, account, category) = fixture();
    drop(repo);
    {
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        repo.insert_account(account.clone()).unwrap();
        repo.insert_category(category.clone()).unwrap();
        repo.insert_transaction(tx(
            &account,
            Some(category.id),
            TransactionKind::Expense,
            1200,
            TransactionStatus::Confirmed,
        ))
        .unwrap();
        let service = BudgetService::new(repo);
        let mut plan = budget(&account, &category, 2000);
        plan.custom_period = true;
        plan.from_date = day("2026-10-10");
        plan.until_date = day("2026-10-20");
        service.create(plan.clone()).unwrap();
        service.archive(plan.id).unwrap();
    }
    let service = BudgetService::new(SqliteLedgerRepository::open(&path).unwrap());
    let plans = service.list().unwrap();
    assert_eq!(plans.len(), 1);
    assert!(plans[0].custom_period);
    assert!(!plans[0].enabled);
    assert_eq!(
        service
            .progress(plans[0].id, FixedOffset::east_opt(8 * 3600).unwrap())
            .unwrap()
            .remaining_cny_minor,
        800
    );
    drop(service);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn budget_reminder_distinguishes_warning_exhausted_and_exceeded_boundaries() {
    for (spent, level) in [
        (799, None),
        (800, Some("warning")),
        (1000, Some("exhausted")),
        (1001, Some("exceeded")),
    ] {
        let (repo, account, category) = fixture();
        repo.insert_transaction(tx(
            &account,
            Some(category.id),
            TransactionKind::Expense,
            spent,
            TransactionStatus::Confirmed,
        ))
        .unwrap();
        let service = BudgetService::new(repo);
        service.create(budget(&account, &category, 1000)).unwrap();
        let events = ReminderService::new(service.repository())
            .due(day("2026-10-16"), FixedOffset::east_opt(0).unwrap())
            .unwrap();
        assert_eq!(events.first().map(|e| e.level), level);
        assert!(
            ReminderService::new(service.repository())
                .due(day("2026-11-01"), FixedOffset::east_opt(0).unwrap())
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn dst_months_use_wall_clock_boundaries_and_exclude_next_month() {
    let zone = chrono_tz::America::New_York;
    for (from, until, hours) in [
        ("2026-03-01", "2026-04-01", 743),
        ("2026-11-01", "2026-12-01", 721),
    ] {
        let range = CalendarRange::new(day(from), day(until), zone).unwrap();
        assert_eq!(
            (range.query.until_utc_exclusive - range.query.from_utc_inclusive).num_hours(),
            hours
        );
        let (repo, account, category) = fixture();
        for (at, amount) in [
            (range.query.from_utc_inclusive, 100),
            (
                range.query.until_utc_exclusive - chrono::Duration::microseconds(1),
                200,
            ),
            (range.query.until_utc_exclusive, 400),
        ] {
            let mut entry = tx(
                &account,
                Some(category.id),
                TransactionKind::Expense,
                amount,
                TransactionStatus::Confirmed,
            );
            entry.occurred_at = at;
            repo.insert_transaction(entry).unwrap();
        }
        let report = ReportService::new(repo)
            .summarize_scoped(ScopedReportQuery {
                range: range.query,
                account_id: None,
                category_id: None,
                merchant: None,
                merchant_offset: 0,
                merchant_limit: 10,
            })
            .unwrap();
        assert_eq!(report.summary.expense_cny_minor, 300);
        assert_eq!(report.transaction_count, 2);
        assert_eq!(
            report
                .summary
                .expense_by_month
                .values()
                .copied()
                .collect::<Vec<_>>(),
            vec![300]
        );
    }
}
#[test]
fn merchant_ranking_pages_do_not_change_scoped_totals() {
    let (repo, account, category) = fixture();
    for (name, amount) in [("店一", 100), ("店二", 200), ("其他", 1000)] {
        let mut entry = tx(
            &account,
            Some(category.id),
            TransactionKind::Expense,
            amount,
            TransactionStatus::Confirmed,
        );
        entry.merchant = Some(name.into());
        repo.insert_transaction(entry).unwrap();
    }
    let service = ReportService::new(repo);
    for (offset, name) in [(0, "店二"), (1, "店一")] {
        let report = service
            .summarize_scoped(ScopedReportQuery {
                range: range(),
                account_id: Some(account.id),
                category_id: Some(category.id),
                merchant: Some("店".into()),
                merchant_offset: offset,
                merchant_limit: 1,
            })
            .unwrap();
        assert_eq!(report.summary.expense_cny_minor, 300);
        assert_eq!(report.merchant_total, 2);
        assert_eq!(report.merchants[0].name, name);
    }
}
#[test]
fn budget_progress_does_not_depend_on_unrelated_income_or_balance_overflow() {
    let (repo, account, category) = fixture();
    for amount in [i64::MAX, 1] {
        repo.insert_transaction(tx(
            &account,
            Some(category.id),
            TransactionKind::Income,
            amount,
            TransactionStatus::Confirmed,
        ))
        .unwrap();
    }
    repo.insert_transaction(tx(
        &account,
        Some(category.id),
        TransactionKind::Expense,
        500,
        TransactionStatus::Confirmed,
    ))
    .unwrap();
    let service = BudgetService::new(repo);
    let plan = budget(&account, &category, 1000);
    service.create(plan.clone()).unwrap();
    let p = service
        .progress(plan.id, FixedOffset::east_opt(0).unwrap())
        .unwrap();
    assert_eq!(p.spent_cny_minor, 500);
    assert_eq!(p.percent, 50);
}
