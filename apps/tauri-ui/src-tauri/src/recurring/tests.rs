use super::*;
use ledger_app::{FeatureRepository, LedgerRepository};
#[test]
fn command_payload_edit_pause_skip_reopen_confirm_and_reminder_share_one_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let mut request = RecurringInput {
        id: None,
        expected_version: None,
        account_id: account.clone(),
        kind: "expense".into(),
        amount: "12.34".into(),
        category_id: None,
        merchant: "房租".into(),
        description: "固定开销".into(),
        cadence: "monthly".into(),
        interval: 1,
        next_due_date: "2026-01-31".into(),
        end_date: Some("2026-03-31".into()),
        enabled: true,
    };
    let id = ledger.write_recurring(request.clone()).unwrap();
    let state = ledger
        .recurring_state_at(date("2026-01-31").unwrap(), &chrono::Utc)
        .unwrap();
    assert_eq!(state.generated_count, 1);
    assert_eq!(state.templates[0].next_due_date, "2026-02-28");
    let tx_id = state.reminders[0].transaction_id.clone().unwrap();
    request.id = Some(id.clone());
    request.expected_version = Some(state.templates[0].version);
    request.next_due_date = "2026-02-28".into();
    request.amount = "20.00".into();
    ledger.write_recurring(request.clone()).unwrap();
    assert!(ledger.write_recurring(request).is_err());
    let t = ledger
        .recurring_state_at(date("2026-01-31").unwrap(), &chrono::Utc)
        .unwrap()
        .templates
        .remove(0);
    ledger.set_recurring_enabled(&id, t.version, false).unwrap();
    let paused = ledger
        .recurring_state_at(date("2026-02-28").unwrap(), &chrono::Utc)
        .unwrap();
    assert_eq!(paused.generated_count, 0);
    ledger
        .set_recurring_enabled(&id, paused.templates[0].version, true)
        .unwrap();
    let resumed = ledger
        .recurring_state_at(date("2026-01-31").unwrap(), &chrono::Utc)
        .unwrap();
    ledger
        .skip_recurring(&id, "2026-02-28", resumed.templates[0].version)
        .unwrap();
    let reopened = Ledger::open(&path).unwrap();
    let state = reopened
        .recurring_state_at(date("2026-03-31").unwrap(), &chrono::Utc)
        .unwrap();
    assert_eq!(state.generated_count, 1);
    assert!(state.templates[0].ended);
    assert_eq!(state.reminders[0].amount, "12.34");
    assert_eq!(state.reminders[1].amount, "20.00");
    reopened
        .confirm_pending(vec![tx_id.clone()], vec![])
        .unwrap();
    let instances = reopened.recurring_instance_views(&id).unwrap();
    assert_eq!(instances.len(), 3);
    assert_eq!(instances[0].status, "confirmed");
    assert_eq!(instances[1].status, "skipped");
    let repo = reopened.repo().unwrap();
    let tx = repo
        .get_transaction(crate::ledger::transaction_id(&tx_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.source_evidence(tx.id).unwrap()[0].source_kind,
        ledger_domain::SourceKind::Recurring
    );
}
#[test]
fn malformed_amount_dates_and_missing_version_do_not_create_or_edit_templates() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let input = RecurringInput {
        id: None,
        expected_version: None,
        account_id: account,
        kind: "income".into(),
        amount: "100.00".into(),
        category_id: None,
        merchant: "工资".into(),
        description: String::new(),
        cadence: "yearly".into(),
        interval: 1,
        next_due_date: "2028-02-29".into(),
        end_date: None,
        enabled: true,
    };
    for (amount, next, cadence, interval) in [
        ("0", "2028-02-29", "yearly", 1),
        ("-1", "2028-02-29", "yearly", 1),
        ("1.001", "2028-02-29", "yearly", 1),
        ("100", "2027-02-29", "yearly", 1),
        ("100", "2028-02-29", "daily", 1),
        ("100", "2028-02-29", "yearly", 0),
    ] {
        let mut bad = input.clone();
        bad.amount = amount.into();
        bad.next_due_date = next.into();
        bad.cadence = cadence.into();
        bad.interval = interval;
        assert!(ledger.write_recurring(bad).is_err());
    }
    let id = ledger.write_recurring(input.clone()).unwrap();
    let mut bad = input;
    bad.id = Some(id);
    assert!(ledger.write_recurring(bad).is_err());
    assert_eq!(
        ledger
            .recurring_state_at(date("2026-10-16").unwrap(), &chrono::Utc)
            .unwrap()
            .templates
            .len(),
        1
    );
}
