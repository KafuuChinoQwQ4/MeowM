use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use ledger_app::{
    AppError, CategoryService, ClassificationRule, ClassificationRuleRepository,
    ClassificationService, LedgerRepository, MonthBoundary, ReportQuery, ReportService,
    RepositoryError, Timestamp,
};
use ledger_domain::{
    Account, AccountId, Category, CategoryId, CurrencyCode, Money, Posting, RuleId, Transaction,
    TransactionId, TransactionKind, TransactionStatus,
};

#[derive(Clone, Default)]
struct MemoryRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    accounts: HashMap<AccountId, Account>,
    transactions: HashMap<TransactionId, Transaction>,
    categories: HashMap<CategoryId, Category>,
    rules: HashMap<RuleId, ClassificationRule>,
}

impl LedgerRepository for MemoryRepository {
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if state.accounts.insert(account.id, account).is_some() {
            return Err(RepositoryError::Conflict);
        }
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
        let mut state = self.state.lock().unwrap();
        if !state.accounts.contains_key(&account.id) {
            return Err(RepositoryError::NotFound);
        }
        state.accounts.insert(account.id, account);
        Ok(())
    }
    fn insert_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if state.transactions.contains_key(&transaction.id) {
            return Err(RepositoryError::Conflict);
        }
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
    fn update_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if !state.transactions.contains_key(&transaction.id) {
            return Err(RepositoryError::NotFound);
        }
        state.transactions.insert(transaction.id, transaction);
        Ok(())
    }
    fn balance_as_of(
        &self,
        account_id: AccountId,
        _: Timestamp,
    ) -> Result<Option<Money>, RepositoryError> {
        self.state
            .lock()
            .unwrap()
            .accounts
            .get(&account_id)
            .map(|account| account.opening_balance.clone())
            .ok_or(RepositoryError::NotFound)
    }
}

impl ClassificationRuleRepository for MemoryRepository {
    fn insert_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        let Some(category) = state.categories.get(&rule.category_id) else {
            return Err(RepositoryError::NotFound);
        };
        if !category.enabled {
            return Err(RepositoryError::InvalidData("disabled category".into()));
        }
        if state.rules.insert(rule.id, rule).is_some() {
            return Err(RepositoryError::Conflict);
        }
        Ok(())
    }
    fn list_rules(&self) -> Result<Vec<ClassificationRule>, RepositoryError> {
        Ok(self.state.lock().unwrap().rules.values().cloned().collect())
    }
    fn update_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if !state.rules.contains_key(&rule.id) {
            return Err(RepositoryError::NotFound);
        }
        state.rules.insert(rule.id, rule);
        Ok(())
    }
    fn insert_category(&self, category: Category) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if state.categories.insert(category.id, category).is_some() {
            return Err(RepositoryError::Conflict);
        }
        Ok(())
    }
    fn get_category(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
        Ok(self.state.lock().unwrap().categories.get(&id).cloned())
    }
    fn update_category(&self, category: Category) -> Result<(), RepositoryError> {
        let mut state = self.state.lock().unwrap();
        if !state.categories.contains_key(&category.id) {
            return Err(RepositoryError::NotFound);
        }
        state.categories.insert(category.id, category);
        Ok(())
    }
    fn list_categories(&self) -> Result<Vec<Category>, RepositoryError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .categories
            .values()
            .cloned()
            .collect())
    }
}

fn utc(value: &str) -> Timestamp {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn category(name: &str) -> Category {
    Category {
        id: CategoryId::new(),
        name: name.into(),
        enabled: true,
    }
}

fn transaction(
    kind: TransactionKind,
    status: TransactionStatus,
    amount: i64,
    occurred_at: Timestamp,
) -> Transaction {
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    Transaction {
        id: TransactionId::new(),
        occurred_at,
        kind,
        status,
        postings: vec![Posting {
            account_id: AccountId::new(),
            delta: Money::new(-amount.abs(), currency.clone()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(amount.abs(), currency)),
        merchant: None,
        description: None,
    }
}

fn month_query(year_month: &str) -> ReportQuery {
    let (from, until) = match year_month {
        "2026-02" => ("2026-02-01T00:00:00Z", "2026-03-01T00:00:00Z"),
        _ => panic!("fixture only supports 2026-02"),
    };
    ReportQuery {
        from_utc_inclusive: utc(from),
        until_utc_exclusive: utc(until),
        month_boundaries_utc: vec![MonthBoundary::new("2026-02", utc(from), utc(until))],
    }
}

#[test]
fn category_lifecycle_preserves_reference_and_disables_new_rules() {
    let repo = MemoryRepository::default();
    let service = CategoryService::new(repo.clone());
    let category = category("Food");
    let category_id = category.id;
    service.create(category).unwrap();
    service.rename(category_id, "餐饮".into()).unwrap();
    service.disable(category_id).unwrap();
    assert_eq!(service.list().unwrap()[0].name, "餐饮");
    assert!(!service.list().unwrap()[0].enabled);
    let error = repo
        .insert_rule(ClassificationRule {
            id: RuleId::new(),
            priority: 1,
            merchant_contains: Some("market".into()),
            description_contains: None,
            category_id,
            enabled: true,
        })
        .unwrap_err();
    assert!(matches!(error, RepositoryError::InvalidData(_)));
}

#[test]
fn classification_is_case_insensitive_and_uses_priority() {
    let repo = MemoryRepository::default();
    let first = category("Groceries");
    let second = category("Dining");
    repo.insert_category(first.clone()).unwrap();
    repo.insert_category(second.clone()).unwrap();
    repo.insert_rule(ClassificationRule {
        id: RuleId::new(),
        priority: 20,
        merchant_contains: Some("market".into()),
        description_contains: None,
        category_id: first.id,
        enabled: true,
    })
    .unwrap();
    repo.insert_rule(ClassificationRule {
        id: RuleId::new(),
        priority: 10,
        merchant_contains: Some("MARKET".into()),
        description_contains: None,
        category_id: second.id,
        enabled: true,
    })
    .unwrap();
    let mut transaction = transaction(
        TransactionKind::Expense,
        TransactionStatus::Confirmed,
        100,
        utc("2026-02-01T00:00:00Z"),
    );
    transaction.merchant = Some("Fresh Market".into());
    assert_eq!(
        ClassificationService::new(repo)
            .suggest(&transaction)
            .unwrap(),
        Some(second.id)
    );
}

#[test]
fn report_uses_only_confirmed_effective_transactions() {
    let repo = MemoryRepository::default();
    let category = category("Food");
    repo.insert_category(category.clone()).unwrap();
    let mut expense = transaction(
        TransactionKind::Expense,
        TransactionStatus::Confirmed,
        3_000,
        utc("2026-02-01T00:00:00Z"),
    );
    expense.category_id = Some(category.id);
    let pending = transaction(
        TransactionKind::Expense,
        TransactionStatus::Pending,
        7_000,
        utc("2026-02-02T00:00:00Z"),
    );
    let transfer = transaction(
        TransactionKind::Transfer,
        TransactionStatus::Confirmed,
        5_000,
        utc("2026-02-03T00:00:00Z"),
    );
    let mut refund = transaction(
        TransactionKind::Refund,
        TransactionStatus::Confirmed,
        500,
        utc("2026-02-04T00:00:00Z"),
    );
    refund.category_id = Some(category.id);
    for item in [expense, pending, transfer, refund] {
        repo.insert_transaction(item).unwrap();
    }
    let summary = ReportService::new(repo)
        .summarize(month_query("2026-02"))
        .unwrap();
    assert_eq!(summary.expense_cny_minor, 2_500);
    assert_eq!(summary.net_cny_minor, -2_500);
    assert_eq!(summary.expense_by_category[&category.id], 2_500);
    assert_eq!(summary.expense_by_month["2026-02"], 2_500);
}

#[test]
fn report_month_boundary_is_half_open() {
    let repo = MemoryRepository::default();
    repo.insert_transaction(transaction(
        TransactionKind::Income,
        TransactionStatus::Confirmed,
        100,
        utc("2026-03-01T00:00:00Z"),
    ))
    .unwrap();
    let summary = ReportService::new(repo)
        .summarize(month_query("2026-02"))
        .unwrap();
    assert_eq!(summary.income_cny_minor, 0);
}

#[test]
fn unmatched_classification_returns_none() {
    let repo = MemoryRepository::default();
    let category = category("Food");
    repo.insert_category(category.clone()).unwrap();
    repo.insert_rule(ClassificationRule {
        id: RuleId::new(),
        priority: 1,
        merchant_contains: Some("coffee".into()),
        description_contains: None,
        category_id: category.id,
        enabled: false,
    })
    .unwrap();
    let transaction = transaction(
        TransactionKind::Expense,
        TransactionStatus::Confirmed,
        100,
        utc("2026-02-01T00:00:00Z"),
    );
    assert_eq!(
        ClassificationService::new(repo)
            .suggest(&transaction)
            .unwrap(),
        None
    );
}

#[allow(dead_code)]
fn _assert_error(_: AppError) {}
