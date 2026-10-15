//! Report and budget IPC adapters, separated from daily ledger commands.
mod budgets;
pub mod commands;
mod reports;
#[cfg(test)]
mod tests;
use crate::ledger::{Ledger, account_id, date, money, parse_amount};
pub use budgets::{BudgetInput, BudgetView, ReminderView};
use chrono::Local;
use ledger_app::{
    BudgetPlan, BudgetService, CalendarRange, ClassificationRuleRepository, ReminderService,
    ReportService, ScopedReportQuery,
};
use ledger_domain::{BudgetId, CategoryId};
pub use reports::{ReportInput, ReportView};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
type Result<T> = std::result::Result<T, String>;
fn category_id(id: Option<String>) -> Result<Option<CategoryId>> {
    id.map(|s| {
        Uuid::parse_str(&s)
            .map(CategoryId::from_uuid)
            .map_err(|_| "无效的分类标识".into())
    })
    .transpose()
}
