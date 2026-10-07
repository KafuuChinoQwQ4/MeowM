use chrono::{DateTime, Utc};
use ledger_app::{
    ImportRecord, MobileLedger, MobileRepository, PendingDetails, TransactionDraft,
    TransactionFilter,
};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
use uuid::Uuid;
fn now() -> DateTime<Utc> {
    "2026-10-07T08:00:00Z".parse().unwrap()
}
fn cny(n: i64) -> Money {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap())
}
fn app() -> MobileLedger<SqliteLedgerRepository> {
    MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap())
}
fn account(app: &MobileLedger<SqliteLedgerRepository>) -> AccountId {
    app.create_account(
        "Wallet",
        AccountKind::Asset,
        cny(10000),
        "2026-10-01T00:00:00Z".parse().unwrap(),
    )
    .unwrap()
}
fn draft(account: AccountId, kind: TransactionKind, n: i64) -> TransactionDraft {
    TransactionDraft {
        occurred_at: now(),
        kind,
        account_id: account,
        destination_id: None,
        amount: cny(n),
        report_amount_cny: Some(cny(n)),
        category_id: None,
        original_transaction_id: None,
        merchant: Some("Coffee".into()),
        memo: Some("memo".into()),
    }
}
#[test]
fn pending_can_be_completed_confirmed_once_and_refreshes_balance_and_report() {
    let app = app();
    let account = account(&app);
    let txn = Transaction {
        id: TransactionId::new(),
        occurred_at: now(),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Pending,
        postings: vec![],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(cny(1234)),
        merchant: Some("Coffee".into()),
        description: None,
    };
    let id = txn.id;
    let batch = Uuid::new_v4();
    app.repository()
        .capture_pending(
            ImportRecord {
                transaction: txn,
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("notification-key".into()),
                    import_batch_id: batch,
                    source_fields_digest: [1; 32],
                },
            },
            PendingDetails {
                amount: Some(cny(1234)),
                source: "wechat".into(),
                account_hint: Some("Wallet".into()),
                original_external_id: None,
            },
        )
        .unwrap();
    assert_eq!(
        app.snapshot(now(), TransactionFilter::default())
            .unwrap()
            .accounts[0]
            .balance
            .minor_units(),
        10000
    );
    app.confirm_pending(id, draft(account, TransactionKind::Expense, 1234))
        .unwrap();
    app.confirm_pending(id, draft(account, TransactionKind::Expense, 1234))
        .unwrap();
    let state = app.snapshot(now(), TransactionFilter::default()).unwrap();
    assert_eq!(state.accounts[0].balance.minor_units(), 8766);
    assert_eq!(state.month.expense_cny_minor, 1234);
    assert!(state.pending.is_empty());
    assert_eq!(
        state.transactions[0].transaction.status,
        TransactionStatus::Confirmed
    );
}
#[test]
fn filters_annotations_settings_and_ignored_candidates_persist() {
    let app = app();
    let account = account(&app);
    let category = app.create_category("Food").unwrap();
    let id = app
        .create_transaction(draft(account, TransactionKind::Expense, 200))
        .unwrap();
    app.edit_annotations(id, Some(category), Some("Lunch".into()))
        .unwrap();
    let filter = TransactionFilter {
        account_id: Some(account),
        category_id: Some(category),
        status: Some(TransactionStatus::Confirmed),
        source: Some("manual".into()),
        from: Some(now()),
        until: Some(now() + chrono::Duration::seconds(1)),
    };
    let state = app.snapshot(now(), filter).unwrap();
    assert_eq!(state.transactions.len(), 1);
    assert_eq!(
        state.transactions[0].transaction.description.as_deref(),
        Some("Lunch")
    );
    let mut settings = app.settings().unwrap();
    assert!(!settings.import_wechat);
    settings.import_wechat = true;
    settings.default_account = Some(account);
    app.save_settings(settings.clone()).unwrap();
    assert_eq!(app.settings().unwrap(), settings);
    assert!(
        app.snapshot(
            now(),
            TransactionFilter {
                source: Some("alipay".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .transactions
        .is_empty()
    );
}
#[test]
fn refund_offsets_and_monthly_merchant_category_reports_exclude_pending() {
    let app = app();
    let account = account(&app);
    let category = app.create_category("Food").unwrap();
    let mut expense = draft(account, TransactionKind::Expense, 2000);
    expense.category_id = Some(category);
    let id = app.create_transaction(expense).unwrap();
    let mut refund = draft(account, TransactionKind::Refund, 500);
    refund.original_transaction_id = Some(id);
    refund.category_id = Some(category);
    app.create_transaction(refund).unwrap();
    let state = app.snapshot(now(), TransactionFilter::default()).unwrap();
    assert_eq!(state.month.expense_cny_minor, 1500);
    assert_eq!(state.refund_cny_minor, 500);
    assert_eq!(state.merchants[0].1, 1500);
    assert_eq!(state.month.expense_by_category[&category], 1500);
    assert_eq!(state.trend.len(), 6);
}
#[test]
fn local_calendar_keeps_october_31_and_february_end_in_the_correct_month() {
    let app = app();
    let account = account(&app);
    for date in ["2026-10-31T15:59:00Z", "2026-02-28T15:59:00Z"] {
        let mut d = draft(account, TransactionKind::Expense, 111);
        d.occurred_at = date.parse().unwrap();
        app.create_transaction(d).unwrap();
        let state = app
            .snapshot_in_timezone(
                date.parse().unwrap(),
                chrono::FixedOffset::east_opt(8 * 3600).unwrap(),
                TransactionFilter::default(),
            )
            .unwrap();
        assert_eq!(state.month.expense_cny_minor, 111);
        assert_eq!(state.trend.last().unwrap().2, 111);
    }
}
#[test]
fn ignored_candidate_and_invalid_confirmation_never_change_balance() {
    let app = app();
    let account = account(&app);
    let batch = Uuid::new_v4();
    let id = TransactionId::new();
    app.repository()
        .capture_pending(
            ImportRecord {
                transaction: Transaction {
                    id,
                    occurred_at: now(),
                    kind: TransactionKind::Refund,
                    status: TransactionStatus::Pending,
                    postings: vec![],
                    category_id: None,
                    original_transaction_id: None,
                    report_amount_cny: Some(cny(100)),
                    merchant: None,
                    description: None,
                },
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("refund".into()),
                    import_batch_id: batch,
                    source_fields_digest: [2; 32],
                },
            },
            PendingDetails {
                amount: Some(cny(100)),
                source: "alipay".into(),
                account_hint: None,
                original_external_id: Some("original-order".into()),
            },
        )
        .unwrap();
    assert!(
        app.confirm_pending(id, draft(account, TransactionKind::Refund, 100))
            .is_err()
    );
    assert_eq!(
        app.snapshot(now(), TransactionFilter::default())
            .unwrap()
            .pending
            .len(),
        1
    );
    app.ignore(id).unwrap();
    let s = app.snapshot(now(), TransactionFilter::default()).unwrap();
    assert_eq!(s.accounts[0].balance.minor_units(), 10000);
    assert_eq!(s.month.expense_cny_minor, 0);
    assert!(s.pending.is_empty());
    assert_eq!(
        s.transactions[0].transaction.status,
        TransactionStatus::Ignored
    );
    assert!(
        app.confirm_pending(id, draft(account, TransactionKind::Expense, 100))
            .is_err()
    );
}
#[test]
fn foreign_notification_metadata_survives_reopen_without_raw_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let app = MobileLedger::new(SqliteLedgerRepository::open(&path).unwrap());
    let batch = Uuid::new_v4();
    let id = TransactionId::new();
    app.repository()
        .capture_pending(
            ImportRecord {
                transaction: Transaction {
                    id,
                    occurred_at: now(),
                    kind: TransactionKind::Expense,
                    status: TransactionStatus::Pending,
                    postings: vec![],
                    category_id: None,
                    original_transaction_id: None,
                    report_amount_cny: None,
                    merchant: None,
                    description: None,
                },
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: None,
                    import_batch_id: batch,
                    source_fields_digest: [3; 32],
                },
            },
            PendingDetails {
                amount: Some(Money::new(1200, CurrencyCode::new("JPY", 0).unwrap())),
                source: "wechat".into(),
                account_hint: Some("card".into()),
                original_external_id: None,
            },
        )
        .unwrap();
    drop(app);
    let app = MobileLedger::new(SqliteLedgerRepository::open(&path).unwrap());
    let s = app.snapshot(now(), TransactionFilter::default()).unwrap();
    let details = s.pending[0].details.as_ref().unwrap();
    assert_eq!(details.amount.as_ref().unwrap().minor_units(), 1200);
    assert_eq!(details.amount.as_ref().unwrap().currency().code(), "JPY");
    assert_eq!(details.account_hint.as_deref(), Some("card"));
}

#[test]
fn directed_transfers_respect_asset_and_liability_balance_conventions() {
    let app = app();
    let cash = account(&app);
    let card = app
        .create_account(
            "Card",
            AccountKind::Liability,
            cny(10000),
            "2026-10-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    let other = app
        .create_account(
            "Other card",
            AccountKind::Liability,
            cny(10000),
            "2026-10-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    for (from, to, expected) in [
        (cash, card, [-1000, -1000]),
        (card, cash, [1000, 1000]),
        (card, other, [1000, -1000]),
    ] {
        let mut d = draft(from, TransactionKind::Transfer, 1000);
        d.destination_id = Some(to);
        let id = app.create_transaction(d).unwrap();
        let t = ledger_app::LedgerRepository::get_transaction(app.repository(), id)
            .unwrap()
            .unwrap();
        assert_eq!(
            [
                t.postings[0].delta.minor_units(),
                t.postings[1].delta.minor_units()
            ],
            expected
        );
    }
}
#[test]
fn cny_confirmation_uses_corrected_amount_for_both_posting_and_report() {
    let app = app();
    let account = account(&app);
    let mut d = draft(account, TransactionKind::Expense, 1500);
    d.report_amount_cny = Some(cny(1234));
    let id = app.create_transaction(d).unwrap();
    let t = ledger_app::LedgerRepository::get_transaction(app.repository(), id)
        .unwrap()
        .unwrap();
    assert_eq!(t.report_amount_cny.unwrap().minor_units(), 1500);
}
#[test]
fn notification_filter_matches_capture_kind_and_provider_independently() {
    let app = app();
    let batch = Uuid::new_v4();
    let id = TransactionId::new();
    app.repository()
        .capture_pending(
            ImportRecord {
                transaction: Transaction {
                    id,
                    occurred_at: now(),
                    kind: TransactionKind::Expense,
                    status: TransactionStatus::Pending,
                    postings: vec![],
                    category_id: None,
                    original_transaction_id: None,
                    report_amount_cny: Some(cny(100)),
                    merchant: None,
                    description: None,
                },
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("filter-notice".into()),
                    import_batch_id: batch,
                    source_fields_digest: [5; 32],
                },
            },
            PendingDetails {
                amount: Some(cny(100)),
                source: "wechat".into(),
                ..Default::default()
            },
        )
        .unwrap();
    for source in ["notification", "wechat"] {
        assert_eq!(
            app.snapshot(
                now(),
                TransactionFilter {
                    source: Some(source.into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .transactions
            .len(),
            1
        );
    }
    assert!(
        app.snapshot(
            now(),
            TransactionFilter {
                source: Some("alipay".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .transactions
        .is_empty()
    );
}

#[test]
fn existing_v4_database_gets_default_provider_grants_and_reopens_persisted_choices() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    // Reproduce the previous release's schema: ledger schema v4, no local grant table.
    drop(SqliteLedgerRepository::open(&path).unwrap());
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("DROP TABLE provider_settings;").unwrap();
    assert_eq!(
        old.query_row("SELECT max(version) FROM schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4
    );
    drop(old);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let mut s = repo.settings().unwrap();
    assert!(!s.api_enabled && !s.api_wechat && !s.api_alipay && !s.api_bank);
    s.api_enabled = true;
    s.api_alipay = true;
    repo.save_settings(&s).unwrap();
    drop(repo);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.settings().unwrap(), s);
}
