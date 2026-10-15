//! Calendar literals and real SQLite catch schedule drift, duplicate candidates and premature balances.
use chrono::{NaiveDate, TimeZone, Utc};
use ledger_app::{
    FeatureRepository, LedgerRepository,
    recurring::{RecurringService, next_occurrence},
};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn setup() -> (SqliteLedgerRepository, RecurringTemplate) {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut account = Account::new(
        "钱包",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    account.opening_balance = Some(Money::new(10000, account.currency.clone()));
    account.opening_balance_effective_at = Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    repo.insert_account(account.clone()).unwrap();
    let template = RecurringTemplate {
        id: RecurringTemplateId::new(),
        account_id: account.id,
        kind: TransactionKind::Expense,
        amount_cny_minor: 1234,
        category_id: None,
        merchant: Some("房租".into()),
        description: Some("每月固定费用".into()),
        cadence: RecurringCadence::Monthly,
        interval: 1,
        anchor_date: date("2026-01-31"),
        next_due_date: date("2026-01-31"),
        end_date: None,
        enabled: true,
        version: 0,
    };
    (repo, template)
}
#[test]
fn calendars_keep_anchor_after_short_months_leap_years_and_weekly_intervals() {
    for (cadence, interval, anchor, current, want) in [
        (
            RecurringCadence::Monthly,
            1,
            "2026-01-31",
            "2026-01-31",
            "2026-02-28",
        ),
        (
            RecurringCadence::Monthly,
            1,
            "2026-01-31",
            "2026-02-28",
            "2026-03-31",
        ),
        (
            RecurringCadence::Monthly,
            2,
            "2026-01-31",
            "2026-01-31",
            "2026-03-31",
        ),
        (
            RecurringCadence::Yearly,
            1,
            "2024-02-29",
            "2027-02-28",
            "2028-02-29",
        ),
        (
            RecurringCadence::Weekly,
            1,
            "2026-10-16",
            "2026-10-16",
            "2026-10-23",
        ),
    ] {
        assert_eq!(
            next_occurrence(cadence, interval, date(anchor), date(current)).unwrap(),
            date(want)
        );
    }
}
#[test]
fn generation_is_pending_confirmed_once_and_stops_at_inclusive_end_date() {
    let (repo, mut template) = setup();
    template.end_date = Some(date("2026-02-28"));
    let service = RecurringService::new(&repo);
    service.create(template.clone()).unwrap();
    let generated = service.generate_due(date("2026-03-31"), &Utc).unwrap();
    assert_eq!(generated.len(), 2);
    assert!(
        service
            .generate_due(date("2026-03-31"), &Utc)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.balance_as_of(
            template.account_id,
            Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()
        )
        .unwrap()
        .unwrap()
        .minor_units(),
        10000
    );
    let tx = repo.get_transaction(generated[0]).unwrap().unwrap();
    assert_eq!(tx.status, TransactionStatus::Pending);
    assert_eq!(tx.postings[0].delta.minor_units(), -1234);
    assert_eq!(
        repo.source_evidence(tx.id).unwrap()[0].source_kind,
        SourceKind::Recurring
    );
    repo.confirm_bulk(BulkConfirmation {
        transaction_ids: vec![tx.id],
    })
    .unwrap();
    repo.confirm_bulk(BulkConfirmation {
        transaction_ids: vec![tx.id],
    })
    .unwrap();
    let instances = service.instances(template.id).unwrap();
    assert_eq!(instances[0].status, RecurringInstanceStatus::Confirmed);
    assert_eq!(instances[1].status, RecurringInstanceStatus::Pending);
    assert_eq!(
        repo.balance_as_of(
            template.account_id,
            Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()
        )
        .unwrap()
        .unwrap()
        .minor_units(),
        8766
    );
}
#[test]
fn pause_resume_skip_and_edits_preserve_existing_candidates_and_optimistic_versions() {
    let (repo, template) = setup();
    let s = RecurringService::new(&repo);
    s.create(template.clone()).unwrap();
    s.pause(template.id, 0).unwrap();
    assert!(s.generate_due(date("2026-02-28"), &Utc).unwrap().is_empty());
    s.resume(template.id, 1).unwrap();
    s.skip_instance(template.id, date("2026-01-31"), 2).unwrap();
    s.skip_instance(template.id, date("2026-01-31"), 2).unwrap(); // retry is harmless
    let feb = s.generate_due(date("2026-02-28"), &Utc).unwrap()[0];
    let mut updated = s.list().unwrap().remove(0);
    let version = updated.version;
    updated.amount_cny_minor = 2500;
    s.update(updated.clone(), version).unwrap();
    assert!(s.update(updated, version).is_err());
    let march = s.generate_due(date("2026-03-31"), &Utc).unwrap()[0];
    assert_eq!(
        repo.get_transaction(feb)
            .unwrap()
            .unwrap()
            .report_amount_cny
            .unwrap()
            .minor_units(),
        1234
    );
    assert_eq!(
        repo.get_transaction(march)
            .unwrap()
            .unwrap()
            .report_amount_cny
            .unwrap()
            .minor_units(),
        2500
    );
    let instances = s.instances(template.id).unwrap();
    assert_eq!(instances.len(), 3);
    assert_eq!(instances[0].status, RecurringInstanceStatus::Skipped);
    assert_eq!(instances[0].transaction_id, None);
    assert_eq!(s.list().unwrap()[0].next_due_date, date("2026-04-30"));
}
#[test]
fn reminders_include_overdue_and_exact_seven_day_boundary_without_confirmed_or_paused_future() {
    let (repo, template) = setup();
    let s = RecurringService::new(&repo);
    s.create(template.clone()).unwrap();
    let ids = s.generate_due(date("2026-01-31"), &Utc).unwrap();
    assert_eq!(s.reminders(date("2026-02-21")).unwrap().len(), 2); // overdue Jan + Feb 28
    assert_eq!(s.reminders(date("2026-02-20")).unwrap().len(), 1);
    repo.confirm_bulk(BulkConfirmation {
        transaction_ids: ids,
    })
    .unwrap();
    assert!(s.reminders(date("2026-02-20")).unwrap().is_empty());
    let v = s.list().unwrap()[0].version;
    s.pause(template.id, v).unwrap();
    assert!(s.reminders(date("2026-02-21")).unwrap().is_empty());
}
#[test]
fn local_calendar_uses_device_date_across_dst_and_liability_direction() {
    let (repo, mut template) = setup();
    let account = Account::new(
        "信用卡",
        AccountKind::Liability,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(account.clone()).unwrap();
    template.account_id = account.id;
    template.anchor_date = date("2026-03-08");
    template.next_due_date = date("2026-03-08");
    template.cadence = RecurringCadence::Weekly;
    let s = RecurringService::new(&repo);
    s.create(template).unwrap();
    let ids = s
        .generate_due(date("2026-03-15"), &chrono_tz::America::New_York)
        .unwrap();
    let first = repo.get_transaction(ids[0]).unwrap().unwrap();
    let second = repo.get_transaction(ids[1]).unwrap().unwrap();
    assert_eq!(first.occurred_at.to_rfc3339(), "2026-03-08T05:00:00+00:00");
    assert_eq!(second.occurred_at.to_rfc3339(), "2026-03-15T04:00:00+00:00");
    assert_eq!(first.postings[0].delta.minor_units(), 1234);
}
#[test]
fn bounded_backlog_finishes_without_duplicates_and_schedule_cannot_rewrite_history() {
    let (repo, mut t) = setup();
    t.cadence = RecurringCadence::Weekly;
    t.anchor_date = date("2020-01-01");
    t.next_due_date = t.anchor_date;
    let s = RecurringService::new(&repo);
    s.create(t.clone()).unwrap();
    let first = s.generate_due(date("2026-01-01"), &Utc).unwrap();
    assert_eq!(first.len(), 200);
    let second = s.generate_due(date("2026-01-01"), &Utc).unwrap();
    assert_eq!(second.len(), 114);
    assert!(s.generate_due(date("2026-01-01"), &Utc).unwrap().is_empty());
    let mut edit = s.list().unwrap().remove(0);
    let version = edit.version;
    edit.next_due_date = date("2025-12-31");
    assert!(s.update(edit.clone(), version).is_err());
    edit.next_due_date = date("2026-02-01");
    edit.cadence = RecurringCadence::Monthly;
    s.update(edit, version).unwrap();
    s.generate_due(date("2026-03-01"), &Utc).unwrap();
    assert_eq!(s.list().unwrap()[0].next_due_date, date("2026-04-01"));
    assert_eq!(repo.list_transactions().unwrap().len(), 316);
}
