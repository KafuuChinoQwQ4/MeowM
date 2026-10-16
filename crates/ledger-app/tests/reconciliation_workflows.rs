//! Reconciliation uses real SQLite, exact cutoffs and explicit, auditable adjustments.
use chrono::{Duration, TimeZone, Utc};
use ledger_app::{
    LedgerRepository,
    reconciliation::{ReconciliationService, ReconciliationStatus},
};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;

fn money(n: i64) -> Money {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap())
}
fn cutoff() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 16, 16, 0, 0).unwrap()
}
fn setup(repo: &SqliteLedgerRepository, reference: bool) -> Account {
    let mut a = Account::new(
        "信用卡",
        AccountKind::Liability,
        money(0).currency().clone(),
    );
    if reference {
        a.opening_balance = Some(money(-10000));
        a.opening_balance_effective_at = Some(cutoff() - Duration::days(30));
    }
    repo.insert_account(a.clone()).unwrap();
    a
}
fn entry(
    repo: &SqliteLedgerRepository,
    a: AccountId,
    at: chrono::DateTime<Utc>,
    delta: i64,
    status: TransactionStatus,
) -> TransactionId {
    let t = Transaction {
        id: TransactionId::new(),
        occurred_at: at,
        kind: TransactionKind::Expense,
        status,
        postings: vec![Posting {
            account_id: a,
            delta: money(delta),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(money(delta.abs())),
        merchant: Some("商户".into()),
        description: None,
    };
    repo.insert_transaction(t.clone()).unwrap();
    t.id
}
#[test]
fn needs_reference_and_snapshot_uses_inclusive_microsecond_cutoff_and_confirmed_only() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let missing = setup(&repo, false);
    let service = ReconciliationService::new(&repo);
    assert!(service.start(missing.id, cutoff(), money(0)).is_err());
    let a = setup(&repo, true);
    let at = entry(&repo, a.id, cutoff(), -101, TransactionStatus::Confirmed);
    entry(
        &repo,
        a.id,
        cutoff() + Duration::microseconds(1),
        -500,
        TransactionStatus::Confirmed,
    );
    entry(&repo, a.id, cutoff(), -200, TransactionStatus::Pending);
    entry(&repo, a.id, cutoff(), -300, TransactionStatus::Ignored);
    let s = service.start(a.id, cutoff(), money(-10200)).unwrap();
    assert_eq!(s.record.session.expected_minor, -10101);
    assert_eq!(s.record.session.difference_minor(), Some(-99));
    assert_eq!(s.entries.len(), 3);
    assert_eq!(s.pending_count(), 1);
    assert!(s.entries.iter().any(|e| e.transaction_id == at));
    entry(
        &repo,
        a.id,
        cutoff() - Duration::microseconds(1),
        -17,
        TransactionStatus::Confirmed,
    );
    let historical = service.summary(s.record.session.id).unwrap();
    assert_eq!(historical, s); // No live rows can rewrite a saved snapshot.
    assert!(
        service
            .add_adjustment(s.record.session.id, 0, "核实后补差")
            .is_err()
    );
}
#[test]
fn adjustment_requires_review_reason_and_is_idempotent_with_close_reopen_audit() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let a = setup(&repo, true);
    let id = entry(&repo, a.id, cutoff(), -100, TransactionStatus::Confirmed);
    let service = ReconciliationService::new(&repo);
    let mut s = service.start(a.id, cutoff(), money(-10200)).unwrap();
    let sid = s.record.session.id;
    assert!(service.close(sid, 0).is_err());
    assert!(service.add_adjustment(sid, 0, "核实补差").is_err());
    service.mark_reviewed(sid, id, true, 0).unwrap();
    s = service.summary(sid).unwrap();
    assert!(service.add_adjustment(sid, s.record.version, "  ").is_err());
    let adjustment = service
        .add_adjustment(sid, s.record.version, "现金核对差额")
        .unwrap();
    assert_eq!(
        service
            .add_adjustment(sid, s.record.version, "现金核对差额")
            .unwrap(),
        adjustment
    );
    let tx = repo.get_transaction(adjustment).unwrap().unwrap();
    assert_eq!(tx.kind, TransactionKind::BalanceAdjustment);
    assert_eq!(tx.postings[0].delta, money(-100));
    assert_eq!(repo.report_override(adjustment).unwrap(), None);
    assert!(
        repo.audit_history(adjustment)
            .unwrap()
            .iter()
            .any(|(a, _)| a.starts_with("reconciliation_adjustment:"))
    );
    assert_eq!(
        repo.balance_as_of(a.id, cutoff()).unwrap(),
        Some(money(-10200))
    );
    s = service.summary(sid).unwrap();
    service.close(sid, s.record.version).unwrap();
    service.close(sid, s.record.version).unwrap(); // retry succeeds without another audit event
    let closed = service.summary(sid).unwrap();
    assert_eq!(closed.record.status, ReconciliationStatus::Closed);
    assert!(
        service
            .mark_reviewed(sid, id, false, closed.record.version)
            .is_err()
    );
    assert!(
        service
            .add_adjustment(sid, closed.record.version, "再补差")
            .is_err()
    );
    service.reopen(sid, closed.record.version).unwrap();
    let reopened = service.summary(sid).unwrap();
    service.close(sid, reopened.record.version).unwrap();
    assert_eq!(
        service
            .summary(sid)
            .unwrap()
            .events
            .iter()
            .filter(|e| e.action == "closed")
            .count(),
        2
    );
    assert!(
        repo.reverse_with_reason(adjustment, cutoff(), "错误冲销")
            .is_err()
    );
    let mut modified = tx;
    modified.description = Some("改写".into());
    assert!(repo.update_transaction(modified).is_err()); // protected provenance
}
#[test]
fn zero_difference_needs_no_adjustment_and_conflicts_never_silently_write() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let a = setup(&repo, true);
    let id = entry(&repo, a.id, cutoff(), -100, TransactionStatus::Pending);
    let service = ReconciliationService::new(&repo);
    let s = service.start(a.id, cutoff(), money(-10000)).unwrap();
    let sid = s.record.session.id;
    service.mark_reviewed(sid, id, true, 0).unwrap();
    assert!(service.mark_reviewed(sid, id, false, 0).is_err());
    let s = service.summary(sid).unwrap();
    service.close(sid, s.record.version).unwrap();
    assert!(
        service
            .summary(sid)
            .unwrap()
            .record
            .session
            .adjustment_id
            .is_none()
    );
    let later = service
        .start(a.id, cutoff() + Duration::days(1), money(-10000))
        .unwrap();
    assert_eq!(later.record.period_from, Some(cutoff()));
    assert!(later.entries.is_empty());
}
#[test]
fn snapshot_and_audit_survive_real_file_reopen_and_reference_changes_block_actions() {
    let path =
        std::env::temp_dir().join(format!("meowm-reconcile-{}.sqlite3", uuid::Uuid::new_v4()));
    let (sid, mut a) = {
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        let a = setup(&repo, true);
        let s = ReconciliationService::new(&repo)
            .start(a.id, cutoff(), money(-10000))
            .unwrap();
        (s.record.session.id, a)
    };
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let service = ReconciliationService::new(&repo);
    let s = service.summary(sid).unwrap();
    assert_eq!(s.events[0].action, "started");
    a.opening_balance_effective_at = Some(cutoff() - Duration::days(2));
    repo.update_account(a).unwrap();
    assert!(service.close(sid, s.record.version).is_err());
    assert_eq!(service.summary(sid).unwrap(), s);
    drop(repo);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn adjustment_cannot_be_at_or_before_reference_and_overflow_is_rejected() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let a = setup(&repo, true);
    let service = ReconciliationService::new(&repo);
    let s = service
        .start(a.id, a.opening_balance_effective_at.unwrap(), money(-10001))
        .unwrap();
    assert!(
        service
            .add_adjustment(s.record.session.id, 0, "边界补差")
            .is_err()
    );
    assert!(service.start(a.id, cutoff(), money(i64::MAX)).is_err());
    assert!(
        service
            .start(
                a.id,
                cutoff(),
                Money::new(0, CurrencyCode::new("USD", 2).unwrap())
            )
            .is_err()
    );
}
