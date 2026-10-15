//! Stable derived budget reminders, with no transaction-writing side effects.
use crate::{AppError, BudgetRepository};
use chrono::{NaiveDate, TimeZone};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderEvent {
    pub id: String,
    pub budget_id: ledger_domain::BudgetId,
    pub level: &'static str,
    pub spent_cny_minor: i64,
    pub amount_cny_minor: i64,
}
pub struct ReminderService<'a, R> {
    repository: &'a R,
}
impl<'a, R: BudgetRepository> ReminderService<'a, R> {
    pub fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    pub fn due<T: TimeZone + Clone>(
        &self,
        today: NaiveDate,
        zone: T,
    ) -> Result<Vec<ReminderEvent>, AppError> {
        let mut result = Vec::new();
        for budget in self.repository.list_budgets()? {
            if !budget.enabled || today < budget.from_date || today >= budget.until_date {
                continue;
            }
            let p = crate::budgets::progress(self.repository, budget, zone.clone())?;
            let level = if p.exceeded {
                "exceeded"
            } else if p.spent_cny_minor >= p.budget.amount_cny_minor && p.spent_cny_minor > 0 {
                "exhausted"
            } else if p.percent >= 80 {
                "warning"
            } else {
                continue;
            };
            result.push(ReminderEvent {
                id: format!(
                    "budget:{}:{}:{}",
                    p.budget.id.as_uuid(),
                    p.budget.period_key(),
                    level
                ),
                budget_id: p.budget.id,
                level,
                spent_cny_minor: p.spent_cny_minor,
                amount_cny_minor: p.budget.amount_cny_minor,
            });
        }
        Ok(result)
    }
}
