//! Budget lifecycle and integer-only progress; disabled budgets retain their history.
use crate::{AppError, BudgetRepository, CalendarRange, RepositoryError};
use chrono::{Datelike, NaiveDate, TimeZone};
use ledger_domain::{AccountId, BudgetId, CategoryId, TransactionStatus, report_effect};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetPlan {
    pub id: BudgetId,
    pub from_date: NaiveDate,
    pub until_date: NaiveDate,
    pub category_id: Option<CategoryId>,
    pub account_id: Option<AccountId>,
    pub amount_cny_minor: i64,
    pub enabled: bool,
    pub custom_period: bool,
}
impl BudgetPlan {
    pub fn validate(&self) -> Result<(), RepositoryError> {
        if self.amount_cny_minor < 0
            || self.from_date >= self.until_date
            || self
                .until_date
                .signed_duration_since(self.from_date)
                .num_days()
                > 3660
        {
            return Err(RepositoryError::InvalidData(
                "预算额度或日期范围无效".into(),
            ));
        }
        if !self.custom_period
            && (self.from_date.day() != 1
                || self.from_date.checked_add_months(chrono::Months::new(1))
                    != Some(self.until_date))
        {
            return Err(RepositoryError::InvalidData(
                "自然月预算必须覆盖完整月份".into(),
            ));
        }
        Ok(())
    }
    /// Fits the pre-existing unique budget scope index, including custom periods.
    pub fn period_key(&self) -> String {
        if self.custom_period {
            format!("custom:{}:{}", self.from_date, self.until_date)
        } else {
            self.from_date.format("%Y-%m").to_string()
        }
    }
}
#[derive(Debug)]
pub struct BudgetProgress {
    pub budget: BudgetPlan,
    pub spent_cny_minor: i64,
    pub remaining_cny_minor: i64,
    pub percent: u128,
    pub exceeded: bool,
}
pub struct BudgetService<R> {
    repository: R,
}
impl<R: BudgetRepository> BudgetService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn repository(&self) -> &R {
        &self.repository
    }
    pub fn list(&self) -> Result<Vec<BudgetPlan>, AppError> {
        Ok(self.repository.list_budgets()?)
    }
    pub fn create(&self, budget: BudgetPlan) -> Result<(), AppError> {
        budget.validate()?;
        self.repository.save_budget(budget, true)?;
        Ok(())
    }
    pub fn update(&self, budget: BudgetPlan) -> Result<(), AppError> {
        budget.validate()?;
        self.repository.save_budget(budget, false)?;
        Ok(())
    }
    pub fn archive(&self, id: BudgetId) -> Result<(), AppError> {
        self.repository.archive_budget(id)?;
        Ok(())
    }
    pub fn progress<T: TimeZone>(&self, id: BudgetId, zone: T) -> Result<BudgetProgress, AppError> {
        let plan = self
            .repository
            .list_budgets()?
            .into_iter()
            .find(|b| b.id == id)
            .ok_or(RepositoryError::NotFound)?;
        progress(&self.repository, plan, zone)
    }
}
pub(crate) fn progress<R: BudgetRepository, T: TimeZone>(
    repo: &R,
    plan: BudgetPlan,
    zone: T,
) -> Result<BudgetProgress, AppError> {
    let range = CalendarRange::new(plan.from_date, plan.until_date, zone)?;
    // Budget spending does not require income totals or account balance snapshots.
    let mut spent = 0_i64;
    for tx in repo.report_transactions(
        range.query.from_utc_inclusive,
        range.query.until_utc_exclusive,
    )? {
        if tx.status != TransactionStatus::Confirmed
            || plan
                .account_id
                .is_some_and(|id| !tx.postings.iter().any(|p| p.account_id == id))
        {
            continue;
        }
        let (category, _) = crate::report_details::effective_scope(repo, &tx)?;
        if plan.category_id.is_some_and(|id| category != Some(id)) {
            continue;
        }
        if let Some(effect) = repo.report_override(tx.id)?.or_else(|| report_effect(&tx)) {
            spent = spent
                .checked_add(effect.expense_cny_minor)
                .ok_or_else(|| RepositoryError::InvalidData("预算支出超出范围".into()))?;
        }
    }
    let remaining = plan
        .amount_cny_minor
        .checked_sub(spent)
        .ok_or_else(|| RepositoryError::InvalidData("预算剩余额度超出范围".into()))?;
    let percent = if plan.amount_cny_minor == 0 {
        if spent > 0 { 100 } else { 0 }
    } else {
        (spent.max(0) as u128) * 100 / (plan.amount_cny_minor as u128)
    };
    Ok(BudgetProgress {
        exceeded: spent > plan.amount_cny_minor,
        budget: plan,
        spent_cny_minor: spent,
        remaining_cny_minor: remaining,
        percent,
    })
}
