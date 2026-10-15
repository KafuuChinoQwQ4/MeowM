use super::{InstanceView, RecurringInput, RecurringState};
use crate::{LedgerState, Result, access};
use tauri::State;
#[tauri::command]
pub fn recurring_state(state: State<'_, LedgerState>) -> Result<RecurringState> {
    access(state, |l| l.recurring_state())
}
#[tauri::command]
pub fn write_recurring(state: State<'_, LedgerState>, request: RecurringInput) -> Result<String> {
    access(state, |l| l.write_recurring(request))
}
#[tauri::command]
pub fn set_recurring_enabled(
    state: State<'_, LedgerState>,
    id: String,
    version: u32,
    enabled: bool,
) -> Result<()> {
    access(state, |l| l.set_recurring_enabled(&id, version, enabled))
}
#[tauri::command]
pub fn skip_recurring(
    state: State<'_, LedgerState>,
    id: String,
    due_date: String,
    version: u32,
) -> Result<()> {
    access(state, |l| l.skip_recurring(&id, &due_date, version))
}
#[tauri::command]
pub fn recurring_instances(state: State<'_, LedgerState>, id: String) -> Result<Vec<InstanceView>> {
    access(state, |l| l.recurring_instance_views(&id))
}
