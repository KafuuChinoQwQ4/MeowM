use super::{RecordView, SummaryView};
use crate::{LedgerState, Result, access};
use tauri::State;
#[tauri::command]
pub fn reconciliation_list(
    state: State<'_, LedgerState>,
    account_id: String,
) -> Result<Vec<RecordView>> {
    access(state, |l| l.reconciliation_list(&account_id))
}
#[tauri::command]
pub fn reconciliation_summary(state: State<'_, LedgerState>, id: String) -> Result<SummaryView> {
    access(state, |l| l.reconciliation_summary(&id))
}
#[tauri::command]
pub fn reconciliation_start(
    state: State<'_, LedgerState>,
    account_id: String,
    date: String,
    actual: String,
) -> Result<SummaryView> {
    access(state, |l| {
        l.reconciliation_start(&account_id, &date, &actual)
    })
}
#[tauri::command]
pub fn reconciliation_review(
    state: State<'_, LedgerState>,
    id: String,
    entry_id: String,
    reviewed: bool,
    version: u32,
) -> Result<()> {
    access(state, |l| {
        l.reconciliation_review(&id, &entry_id, reviewed, version)
    })
}
#[tauri::command]
pub fn reconciliation_adjust(
    state: State<'_, LedgerState>,
    id: String,
    version: u32,
    reason: String,
) -> Result<String> {
    access(state, |l| l.reconciliation_adjust(&id, version, &reason))
}
#[tauri::command]
pub fn reconciliation_close(state: State<'_, LedgerState>, id: String, version: u32) -> Result<()> {
    access(state, |l| l.reconciliation_close(&id, version))
}
#[tauri::command]
pub fn reconciliation_reopen(
    state: State<'_, LedgerState>,
    id: String,
    version: u32,
) -> Result<()> {
    access(state, |l| l.reconciliation_reopen(&id, version))
}
