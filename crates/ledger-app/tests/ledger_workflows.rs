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
    ) -> Result<Money, RepositoryError> {
        let state = self.state.lock().unwrap();
        let account = state
            .accounts
            .get(&account_id)
            .ok_or(RepositoryError::NotFound)?;
        let mut balance = account.opening_balance.clone();
        for transaction in state.transactions.values().filter(|transaction| {
            transaction.status == TransactionStatus::Confirmed
                && transaction.occurred_at > account.opening_balance_effective_at
                && transaction.occurred_at <= cutoff_utc
        }) {
            for posting in transaction
                .postings
                .iter()
                .filter(|posting| posting.account_id == account_id)
            {
                balance = balance
                    .checked_add(posting.delta.clone())
                    .map_err(|error| RepositoryError::InvalidData(error.to_string()))?;
            }
        }
        Ok(balance)
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
        opening_balance: Money::new(opening_minor, code),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
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
            .minor_units(),
        10_000
    );
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
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
            .minor_units(),
        10_000
    );
    service.ignore(pending_id).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
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
            .minor_units(),
        9_000
    );

    service.confirm(pending).unwrap();
    assert_eq!(service.repository().write_count(), 2);
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
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
        opening_balance: Money::new(5_000, cny.clone()),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
    };
    let bank = Account {
        id: AccountId::new(),
        display_name: "bank".into(),
        kind: AccountKind::Asset,
        currency: cny.clone(),
        opening_balance: Money::new(2_000, cny.clone()),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
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
            .minor_units(),
        3_500
    );
    assert_eq!(
        service
            .account_balance(bank.id, cutoff)
            .unwrap()
            .minor_units(),
        3_500
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
