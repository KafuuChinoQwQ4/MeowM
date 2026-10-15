use super::{BudgetInput, BudgetView, ReminderView, ReportInput, ReportView};
use crate::{LedgerState, Result, access};
use tauri::State;
#[tauri::command]
pub fn report_summary(state: State<'_, LedgerState>, query: ReportInput) -> Result<ReportView> {
    access(state, |l| l.report(query))
}
#[tauri::command]
pub fn list_budgets(state: State<'_, LedgerState>) -> Result<Vec<BudgetView>> {
    access(state, |l| l.budget_views())
}
#[tauri::command]
pub fn write_budget(state: State<'_, LedgerState>, request: BudgetInput) -> Result<String> {
    access(state, |l| l.write_budget(request))
}
#[tauri::command]
pub fn archive_budget(state: State<'_, LedgerState>, id: String) -> Result<()> {
    access(state, |l| l.archive_budget(&id))
}
#[tauri::command]
pub fn budget_reminders(state: State<'_, LedgerState>) -> Result<Vec<ReminderView>> {
    access(state, |l| l.budget_reminders())
}
