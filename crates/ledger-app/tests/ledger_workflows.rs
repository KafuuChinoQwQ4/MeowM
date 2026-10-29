use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use ledger_app::{AppError, LedgerRepository, RepositoryError, Timestamp, TransactionService};
use ledger_domain::{
    Account, AccountId, AccountKind, CurrencyCode, Money, Posting, Transaction, TransactionId,
    TransactionKind, TransactionStatus,
};

#[derive(Clone, Default)]
struct InMemoryRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryRepository {
    fn write_count(&self) -> usize {
        self.state.lock().unwrap().transaction_writes
    }
}

#[derive(Default)]
struct State {
    accounts: HashMap<AccountId, Account>,
    transactions: HashMap<TransactionId, Transaction>,
    transaction_writes: usize,
}

impl LedgerRepository for InMemoryRepository {
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError> {
        self.state
            .lock()
            .unwrap()
            .accounts
            .insert(account.id, account);
        Ok(())
    }

    fn get_account(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        Ok(self.state.lock().unwrap().accounts.get(&id).cloned())
    }

    fn list_accounts(&self) -> Result<Vec<Account>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .accounts
            .values()
            .cloned()
            .collect())
    }

    fn update_account(&self, account: Account) -> Result<(), RepositoryError> {
        self.state
            .lock()
            .unwrap()
            .accounts
            .insert(account.id, account);
        Ok(())
    }

    fn insert_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if state.transactions.contains_key(&transaction.id) {
            return Err(RepositoryError::Conflict);
        }
        state.transaction_writes += 1;
        state.transactions.insert(transaction.id, transaction);
        Ok(())
    }

    fn update_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        state.transaction_writes += 1;
        state.transactions.insert(transaction.id, transaction);
        Ok(())
    }

    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        Ok(self.state.lock().unwrap().transactions.get(&id).cloned())
    }

    fn list_transactions(&self) -> Result<Vec<Transaction>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .transactions
            .values()
            .cloned()
            .collect())
    }

    fn balance_as_of(
        &self,
        account_id: AccountId,
        cutoff_utc: Timestamp,
    ) -> Result<Option<Money>, RepositoryError> {
        let state = self.state.lock().unwrap();
        let account = state
            .accounts
            .get(&account_id)
            .ok_or(RepositoryError::NotFound)?;
        let Some(mut balance) = account.opening_balance.clone() else {
            return Ok(None);
        };
        let reference = account.opening_balance_effective_at.unwrap();
        let lower = reference.min(cutoff_utc);
        let upper = reference.max(cutoff_utc);
        for transaction in state.transactions.values().filter(|transaction| {
            transaction.status == TransactionStatus::Confirmed
                && transaction.occurred_at > lower
                && transaction.occurred_at <= upper
        }) {
            for posting in transaction
                .postings
                .iter()
                .filter(|posting| posting.account_id == account_id)
            {
                balance = if cutoff_utc >= reference {
                    balance.checked_add(posting.delta.clone())
                } else {
                    balance.checked_sub(posting.delta.clone())
                }
                .map_err(|error| RepositoryError::InvalidData(error.to_string()))?;
            }
        }
        Ok(Some(balance))
    }
}

fn utc(rfc3339: &str) -> Timestamp {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
}

fn service_with_account(
    currency: &str,
    opening_minor: i64,
) -> (TransactionService<InMemoryRepository>, AccountId) {
    let repo = InMemoryRepository::default();
    let code = CurrencyCode::new(currency, if currency == "JPY" { 0 } else { 2 }).unwrap();
    let account = Account {
        id: AccountId::new(),
        display_name: "Wallet".into(),
        kind: AccountKind::Asset,
        currency: code.clone(),
        opening_balance: Some(Money::new(opening_minor, code)),
        opening_balance_effective_at: Some(utc("2026-01-01T00:00:00Z")),
    };
    let account_id = account.id;
    repo.insert_account(account).unwrap();
    (TransactionService::new(repo), account_id)
}

fn expense_for(account_id: AccountId, minor_units: i64, occurred_at: Timestamp) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        occurred_at,
        kind: TransactionKind::Expense,
        status: TransactionStatus::Pending,
        postings: vec![Posting {
            account_id,
            delta: Money::new(-minor_units, CurrencyCode::new("CNY", 2).unwrap()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(
            minor_units,
            CurrencyCode::new("CNY", 2).unwrap(),
        )),
        merchant: None,
        description: None,
    }
}

#[test]
fn confirming_expense_changes_balance_once_and_only_after_its_time() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let expense = expense_for(account_id, 2_500, utc("2026-01-02T00:00:00Z"));
    service.confirm(expense.clone()).unwrap();
    service.confirm(expense).unwrap();
    assert_eq!(service.repository().write_count(), 1);

    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-01T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        7_500
    );
}

#[test]
fn pending_and_ignored_transactions_do_not_affect_balance() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let pending = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    let pending_id = pending.id;
    service.repository().insert_transaction(pending).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
    service.ignore(pending_id).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
}

#[test]
fn stored_pending_transaction_applies_balance_once_when_confirmed() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let pending = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    let pending_id = pending.id;
    service
        .repository()
        .insert_transaction(pending.clone())
        .unwrap();
    assert_eq!(service.repository().write_count(), 1);
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );

    service.confirm(pending.clone()).unwrap();
    assert_eq!(service.repository().write_count(), 2);
    assert_eq!(
        service
            .repository()
            .get_transaction(pending_id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Confirmed
    );
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        9_000
    );

    service.confirm(pending).unwrap();
    assert_eq!(service.repository().write_count(), 2);
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        9_000
    );
}

#[test]
fn ignored_transaction_cannot_be_confirmed_or_written_again() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let pending = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    let id = pending.id;
    service
        .repository()
        .insert_transaction(pending.clone())
        .unwrap();
    service.ignore(id).unwrap();
    let writes_after_ignore = service.repository().write_count();

    assert!(matches!(
        service.confirm(pending),
        Err(AppError::TransactionNotPending)
    ));
    assert_eq!(service.repository().write_count(), writes_after_ignore);
    assert_eq!(
        service
            .repository()
            .get_transaction(id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Ignored
    );
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
}

#[test]
fn conflicting_pending_content_is_rejected_without_update() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let stored = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    let mut proposed = stored.clone();
    proposed.report_amount_cny = Some(Money::new(900, CurrencyCode::new("CNY", 2).unwrap()));
    service
        .repository()
        .insert_transaction(stored.clone())
        .unwrap();
    let writes_before_confirm = service.repository().write_count();

    assert!(matches!(
        service.confirm(proposed),
        Err(AppError::TransactionConflict)
    ));
    assert_eq!(service.repository().write_count(), writes_before_confirm);
    assert_eq!(
        service.repository().get_transaction(stored.id).unwrap(),
        Some(stored)
    );
}

#[test]
fn invalid_stored_pending_candidate_is_rejected_without_update() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let mut invalid = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    invalid.postings.clear();
    let id = invalid.id;
    service
        .repository()
        .insert_transaction(invalid.clone())
        .unwrap();
    let writes_before_confirm = service.repository().write_count();

    assert!(matches!(service.confirm(invalid), Err(AppError::Domain(_))));
    assert_eq!(service.repository().write_count(), writes_before_confirm);
    assert_eq!(
        service
            .repository()
            .get_transaction(id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Pending
    );
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
}

#[test]
fn confirmed_transaction_with_no_postings_is_rejected() {
    let (service, _account_id) = service_with_account("CNY", 10_000);
    let mut pending = expense_for(AccountId::new(), 1_000, utc("2026-01-02T00:00:00Z"));
    pending.postings.clear();
    let id = pending.id;
    assert!(service.confirm(pending).is_err());
    assert!(service.repository().get_transaction(id).unwrap().is_none());
}

#[test]
fn confirmed_transaction_cannot_be_ignored() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let expense = expense_for(account_id, 500, utc("2026-01-02T00:00:00Z"));
    let id = expense.id;
    service.confirm(expense).unwrap();
    assert!(matches!(
        service.ignore(id),
        Err(AppError::TransactionNotPending)
    ));
}

#[test]
fn transfer_changes_both_account_balances_without_changing_total_funds() {
    let repo = InMemoryRepository::default();
    let cny = CurrencyCode::new("CNY", 2).unwrap();
    let cash = Account {
        id: AccountId::new(),
        display_name: "cash".into(),
        kind: AccountKind::Asset,
        currency: cny.clone(),
        opening_balance: Some(Money::new(5_000, cny.clone())),
        opening_balance_effective_at: Some(utc("2026-01-01T00:00:00Z")),
    };
    let bank = Account {
        id: AccountId::new(),
        display_name: "bank".into(),
        kind: AccountKind::Asset,
        currency: cny.clone(),
        opening_balance: Some(Money::new(2_000, cny.clone())),
        opening_balance_effective_at: Some(utc("2026-01-01T00:00:00Z")),
    };
    repo.insert_account(cash.clone()).unwrap();
    repo.insert_account(bank.clone()).unwrap();
    let service = TransactionService::new(repo);
    let transfer = Transaction {
        id: TransactionId::new(),
        occurred_at: utc("2026-01-02T00:00:00Z"),
        kind: TransactionKind::Transfer,
        status: TransactionStatus::Pending,
        postings: vec![
            Posting {
                account_id: cash.id,
                delta: Money::new(-1_500, cny.clone()),
            },
            Posting {
                account_id: bank.id,
                delta: Money::new(1_500, cny.clone()),
            },
        ],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: None,
        merchant: None,
        description: None,
    };
    service.confirm(transfer).unwrap();
    let cutoff = utc("2026-01-03T00:00:00Z");
    assert_eq!(
        service
            .account_balance(cash.id, cutoff)
            .unwrap()
            .unwrap()
            .minor_units(),
        3_500
    );
    assert_eq!(
        service
            .account_balance(bank.id, cutoff)
            .unwrap()
            .unwrap()
            .minor_units(),
        3_500
    );
}

#[test]
fn unlinked_refund_can_be_confirmed_and_updates_the_account_balance() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let mut refund = expense_for(account_id, 250, utc("2026-01-03T00:00:00Z"));
    refund.kind = TransactionKind::Refund;
    refund.postings[0].delta = Money::new(250, CurrencyCode::new("CNY", 2).unwrap());
    service.confirm(refund).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-04T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_250
    );
}

#[test]
fn refund_requires_a_confirmed_expense_parent() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let mut refund = expense_for(account_id, 250, utc("2026-01-03T00:00:00Z"));
    refund.kind = TransactionKind::Refund;
    refund.postings[0].delta = Money::new(250, CurrencyCode::new("CNY", 2).unwrap());
    refund.original_transaction_id = Some(TransactionId::new());
    assert!(matches!(
        service.confirm(refund),
        Err(AppError::RefundParentInvalid)
    ));
}

#[test]
fn refund_can_be_confirmed_when_parent_is_a_confirmed_expense() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let parent = expense_for(account_id, 1_000, utc("2026-01-02T00:00:00Z"));
    let parent_id = service.confirm(parent).unwrap();
    let mut refund = expense_for(account_id, 250, utc("2026-01-03T00:00:00Z"));
    refund.kind = TransactionKind::Refund;
    refund.postings[0].delta = Money::new(250, CurrencyCode::new("CNY", 2).unwrap());
    refund.original_transaction_id = Some(parent_id);
    service.confirm(refund).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-04T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        9_250
    );
}

#[test]
fn opening_balance_update_applies_from_its_effective_time() {
    let (service, account_id) = service_with_account("CNY", 1_000);
    let accounts = ledger_app::AccountService::new(service.repository().clone());
    accounts
        .set_opening_balance(
            account_id,
            Money::new(2_000, CurrencyCode::new("CNY", 2).unwrap()),
            utc("2026-01-02T00:00:00Z"),
        )
        .unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-01T12:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        2_000
    );
}

#[test]
fn cutoff_and_opening_balance_effective_time_are_respected() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    service
        .confirm(expense_for(account_id, 500, utc("2025-12-31T00:00:00Z")))
        .unwrap();
    service
        .confirm(expense_for(account_id, 700, utc("2026-01-03T00:00:00Z")))
        .unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-02T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
}

#[test]
fn repeated_id_with_different_content_is_rejected() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let expense = expense_for(account_id, 500, utc("2026-01-02T00:00:00Z"));
    let mut conflicting = expense.clone();
    conflicting.report_amount_cny = Some(Money::new(600, CurrencyCode::new("CNY", 2).unwrap()));
    service.confirm(expense).unwrap();
    assert!(matches!(
        service.confirm(conflicting),
        Err(AppError::TransactionConflict)
    ));
}

#[test]
fn invalid_posting_is_rejected_without_repository_writes() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let mut invalid = expense_for(account_id, 500, utc("2026-01-02T00:00:00Z"));
    invalid.postings[0].delta = Money::new(500, CurrencyCode::new("CNY", 2).unwrap());
    let id = invalid.id;
    assert!(service.confirm(invalid).is_err());
    assert!(service.repository().get_transaction(id).unwrap().is_none());
    assert_eq!(service.repository().write_count(), 0);
}

#[test]
fn sqlite_feature_workflow_keeps_pending_out_of_reports_then_reverses_confirmation() {
    use ledger_app::{FeatureRepository, ReportQuery, ReportService};
    use ledger_domain::{BulkConfirmation, PendingQuery};
    use ledger_storage::sqlite::SqliteLedgerRepository;

    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    let wallet = Account::new("review wallet", AccountKind::Asset, currency);
    repo.insert_account(wallet.clone()).unwrap();
    let mut candidate = expense_for(wallet.id, 1250, utc("2026-10-15T12:00:00Z"));
    candidate.status = TransactionStatus::Pending;
    repo.insert_transaction(candidate.clone()).unwrap();
    let reports = ReportService::new(repo);
    let query = || ReportQuery {
        from_utc_inclusive: utc("2026-10-01T00:00:00Z"),
        until_utc_exclusive: utc("2026-11-01T00:00:00Z"),
        month_boundaries_utc: vec![],
    };
    assert_eq!(reports.summarize(query()).unwrap().expense_cny_minor, 0);

    // A missing ID prevents even the valid candidate from being confirmed.
    assert!(
        reports
            .repository()
            .confirm_bulk(BulkConfirmation {
                transaction_ids: vec![candidate.id, TransactionId::new()],
            })
            .is_err()
    );
    let pending = reports
        .repository()
        .query_transactions(PendingQuery {
            status: Some(TransactionStatus::Pending),
            limit: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(pending.total, 1);
    assert_eq!(pending.items[0].transaction.id, candidate.id);
    for _ in 0..2 {
        reports
            .repository()
            .confirm_bulk(BulkConfirmation {
                transaction_ids: vec![candidate.id],
            })
            .unwrap();
    }
    assert_eq!(reports.summarize(query()).unwrap().expense_cny_minor, 1250);
    let reversal = reports
        .repository()
        .reverse_atomic(candidate.id, TransactionId::new(), candidate.occurred_at)
        .unwrap();
    assert_eq!(
        reports
            .repository()
            .reverse_atomic(candidate.id, TransactionId::new(), candidate.occurred_at,)
            .unwrap(),
        reversal
    );
    let summary = reports.summarize(query()).unwrap();
    assert_eq!(
        (
            summary.income_cny_minor,
            summary.expense_cny_minor,
            summary.net_cny_minor
        ),
        (0, 0, 0)
    );
    assert_eq!(reports.repository().list_transactions().unwrap().len(), 2);
}

#[test]
fn sqlite_pending_batch_and_edited_confirmation_are_atomic_and_idempotent() {
    use ledger_app::{FeatureRepository, PendingService};
    use ledger_domain::{PendingQuery, SourceEvidence, SourceKind, TransactionEdit};
    use ledger_storage::sqlite::SqliteLedgerRepository;
    use uuid::Uuid;
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let wallet = Account::new(
        "import wallet",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(wallet.clone()).unwrap();
    let pending = expense_for(wallet.id, 1250, utc("2026-10-15T12:00:00Z"));
    let batch = Uuid::new_v4();
    let evidence = SourceEvidence {
        source_kind: SourceKind::Bank,
        external_transaction_id: Some("bank-1".into()),
        original_external_transaction_id: None,
        merchant_order_id: None,
        import_batch_id: batch,
        source_fields_digest: [5; 32],
    };
    let service = PendingService::new(repo);
    let ids = service
        .stage_batch(batch, vec![(pending.clone(), evidence.clone())])
        .unwrap();
    assert_eq!(ids, vec![pending.id]);
    assert_eq!(
        service
            .stage_batch(batch, vec![(pending.clone(), evidence)])
            .unwrap(),
        ids
    );
    assert_eq!(
        service.repository().source_evidence(pending.id).unwrap()[0].source_kind,
        SourceKind::Bank
    );
    let mut edited = pending.clone();
    edited.description = Some("corrected".into());
    assert!(
        service
            .confirm(
                vec![pending.id, TransactionId::new()],
                vec![TransactionEdit {
                    transaction: edited.clone(),
                    expected_version: 0
                }]
            )
            .is_err()
    );
    assert_eq!(
        service.repository().get_transaction(pending.id).unwrap(),
        Some(pending.clone())
    );
    service
        .confirm(
            vec![pending.id],
            vec![TransactionEdit {
                transaction: edited.clone(),
                expected_version: 0,
            }],
        )
        .unwrap();
    service
        .confirm(
            vec![pending.id],
            vec![TransactionEdit {
                transaction: edited,
                expected_version: 0,
            }],
        )
        .unwrap();
    assert_eq!(
        service
            .list(PendingQuery {
                status: Some(TransactionStatus::Confirmed),
                limit: 10,
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
}

#[test]
fn sqlite_pending_ignore_batch_rolls_back_and_restore_retries() {
    use ledger_app::PendingService;
    use ledger_storage::sqlite::SqliteLedgerRepository;
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let wallet = Account::new(
        "wallet",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(wallet.clone()).unwrap();
    let pending = expense_for(wallet.id, 100, utc("2026-10-01T00:00:00Z"));
    repo.insert_transaction(pending.clone()).unwrap();
    let service = PendingService::new(repo);
    assert!(service.ignore(&[pending.id, TransactionId::new()]).is_err());
    assert_eq!(
        service
            .repository()
            .get_transaction(pending.id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Pending
    );
    service.ignore(&[pending.id]).unwrap();
    service.ignore(&[pending.id]).unwrap();
    service.undo_ignore(pending.id).unwrap();
    service.undo_ignore(pending.id).unwrap();
    assert_eq!(
        service
            .repository()
            .get_transaction(pending.id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Pending
    );
}

#[test]
fn sqlite_invalid_import_row_rolls_back_batch_postings_and_evidence() {
    use ledger_app::PendingService;
    use ledger_domain::{SourceEvidence, SourceKind};
    use ledger_storage::sqlite::SqliteLedgerRepository;
    use uuid::Uuid;
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let wallet = Account::new(
        "wallet",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(wallet.clone()).unwrap();
    let first = expense_for(wallet.id, 100, utc("2026-10-01T00:00:00Z"));
    let bad = expense_for(AccountId::new(), 100, utc("2026-10-02T00:00:00Z"));
    let batch = Uuid::new_v4();
    let evidence = |id: &str| SourceEvidence {
        source_kind: SourceKind::WeChat,
        external_transaction_id: Some(id.into()),
        original_external_transaction_id: None,
        merchant_order_id: None,
        import_batch_id: batch,
        source_fields_digest: [7; 32],
    };
    let service = PendingService::new(repo);
    assert!(
        service
            .stage_batch(
                batch,
                vec![(first, evidence("first")), (bad, evidence("bad"))]
            )
            .is_err()
    );
    assert!(service.repository().list_transactions().unwrap().is_empty());
    assert_eq!(service.repository().posting_count().unwrap(), 0);
    assert_eq!(service.repository().source_evidence_count().unwrap(), 0);
    assert_eq!(service.repository().import_batch_count().unwrap(), 0);
}

#[test]
fn sqlite_pending_refund_edits_allow_unresolved_parent_but_validate_explicit_links() {
    use ledger_app::{FeatureRepository, PendingService};
    use ledger_domain::{SourceEvidence, SourceKind, TransactionEdit};
    use ledger_storage::sqlite::SqliteLedgerRepository;
    use uuid::Uuid;
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let wallet = Account::new(
        "wallet",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    repo.insert_account(wallet.clone()).unwrap();
    let parent = expense_for(wallet.id, 1000, utc("2026-10-01T00:00:00Z"));
    repo.insert_transaction(parent.clone()).unwrap();
    let mut refund = expense_for(wallet.id, 500, utc("2026-10-02T00:00:00Z"));
    refund.kind = TransactionKind::Refund;
    refund.postings[0].delta = Money::new(500, CurrencyCode::new("CNY", 2).unwrap());
    let batch = Uuid::new_v4();
    let service = PendingService::new(repo);
    service
        .stage_batch(
            batch,
            vec![(
                refund.clone(),
                SourceEvidence {
                    source_kind: SourceKind::WeChat,
                    external_transaction_id: Some("refund-review".into()),
                    original_external_transaction_id: None,
                    merchant_order_id: None,
                    import_batch_id: batch,
                    source_fields_digest: [5; 32],
                },
            )],
        )
        .unwrap();
    refund.description = Some("corrected refund".into());
    service
        .repository()
        .edit_atomic(TransactionEdit {
            transaction: refund.clone(),
            expected_version: 0,
        })
        .unwrap();
    // Saving an unlinked draft is allowed. An explicitly chosen pending parent
    // must still be confirmed before this linked refund can be confirmed.
    refund.original_transaction_id = Some(parent.id);
    service
        .repository()
        .edit_atomic(TransactionEdit {
            transaction: refund.clone(),
            expected_version: 1,
        })
        .unwrap();
    assert!(service.confirm(vec![refund.id], vec![]).is_err());
    assert_eq!(
        service
            .repository()
            .get_transaction(refund.id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Pending
    );
    service.confirm(vec![parent.id], vec![]).unwrap();
    service.confirm(vec![refund.id], vec![]).unwrap();
    assert_eq!(
        service
            .repository()
            .get_transaction(refund.id)
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Confirmed
    );
}
