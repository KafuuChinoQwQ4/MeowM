mod analytics;
mod data_files;
mod data_management;
mod ledger;
mod notifications;
mod privacy_commands;
mod reconciliation;
mod recurring;
mod search_export;
mod search_rules;
mod transactions;
use ledger::{Account, DayView, Ledger};
use std::sync::Mutex;
use tauri::{Manager, State};
type Result<T> = std::result::Result<T, String>;
struct LedgerState(Mutex<Ledger>);
fn access<T>(state: State<'_, LedgerState>, f: impl FnOnce(&Ledger) -> Result<T>) -> Result<T> {
    let ledger = state.0.lock().map_err(|_| "账本忙，请重启应用")?;
    ledger.require_unlocked()?;
    ledger
        .privacy
        .borrow_mut()
        .touch()
        .map_err(|e| e.to_string())?;
    f(&ledger)
}
#[tauri::command]
fn list_accounts(state: State<'_, LedgerState>) -> Result<Vec<Account>> {
    access(state, |l| l.accounts())
}
#[tauri::command]
fn create_account(
    state: State<'_, LedgerState>,
    name: String,
    category: String,
) -> Result<Account> {
    access(state, |l| l.create_platform(&name, &category))
}
#[tauri::command]
fn update_platform(
    state: State<'_, LedgerState>,
    account_id: String,
    name: String,
    category: String,
) -> Result<()> {
    access(state, |l| l.update_platform(&account_id, &name, &category))
}
#[tauri::command]
fn get_day(state: State<'_, LedgerState>, account_id: String, date: String) -> Result<DayView> {
    access(state, |l| l.day(&account_id, &date))
}
#[tauri::command]
fn save_entry(
    state: State<'_, LedgerState>,
    account_id: String,
    date: String,
    amount: String,
    note: String,
    income: bool,
) -> Result<()> {
    access(state, |l| {
        l.save_entry(&account_id, &date, &amount, &note, income)
    })
}
#[tauri::command]
fn set_reference(
    state: State<'_, LedgerState>,
    account_id: String,
    date: String,
    amount: String,
) -> Result<()> {
    access(state, |l| l.set_reference(&account_id, &date, &amount))
}
#[tauri::command]
fn clear_reference(state: State<'_, LedgerState>, account_id: String) -> Result<()> {
    access(state, |l| l.clear_reference(&account_id))
}
#[tauri::command]
fn query_entries(
    state: State<'_, LedgerState>,
    query: ledger::PageQuery,
) -> std::result::Result<ledger::EntryPage, ledger::CommandError> {
    let ledger = state.0.lock().map_err(|_| ledger::CommandError {
        code: "busy".into(),
        message: "账本忙，请重试".into(),
    })?;
    ledger
        .require_unlocked()
        .map_err(|message| ledger::CommandError {
            code: "locked".into(),
            message,
        })?;
    ledger
        .privacy
        .borrow_mut()
        .touch()
        .map_err(|e| ledger::CommandError {
            code: "locked".into(),
            message: e.to_string(),
        })?;
    ledger.page(query)
}
#[tauri::command]
fn ignore_pending(state: State<'_, LedgerState>, ids: Vec<String>) -> Result<()> {
    access(state, |l| ledger::ignore_pending(l, ids))
}
#[tauri::command]
fn restore_pending(state: State<'_, LedgerState>, id: String) -> Result<()> {
    access(state, |l| ledger::restore_pending(l, id))
}
#[tauri::command]
fn preview_bill(
    state: State<'_, LedgerState>,
    provider: String,
    bytes: Vec<u8>,
) -> Result<ledger::BillPreview> {
    access(state, |l| l.bill_preview(&provider, bytes))
}
#[tauri::command]
fn stage_bill(
    state: State<'_, LedgerState>,
    preview_id: String,
    account_id: String,
    ids: Vec<String>,
    edits: Vec<ledger::PendingEdit>,
) -> Result<Vec<String>> {
    access(state, |l| {
        l.stage_preview(&preview_id, &account_id, ids, edits)
    })
}
#[tauri::command]
fn confirm_pending(
    state: State<'_, LedgerState>,
    ids: Vec<String>,
    edits: Vec<ledger::PendingEdit>,
) -> Result<()> {
    access(state, |l| l.confirm_pending(ids, edits))
}
#[tauri::command]
fn edit_pending(state: State<'_, LedgerState>, edit: ledger::PendingEdit) -> Result<()> {
    access(state, |l| l.edit_pending(edit))
}
#[tauri::command]
fn list_categories(state: State<'_, LedgerState>) -> Result<Vec<ledger::CategoryOption>> {
    access(state, |l| l.categories())
}
#[tauri::command]
fn transaction_detail(state: State<'_, LedgerState>, id: String) -> Result<transactions::Detail> {
    access(state, |l| l.transaction_detail(&id))
}
#[tauri::command]
fn write_transaction(
    state: State<'_, LedgerState>,
    request: transactions::WriteTransaction,
) -> Result<String> {
    access(state, |l| l.write_transaction(request))
}
#[tauri::command]
fn reverse_transaction(
    state: State<'_, LedgerState>,
    id: String,
    reason: String,
) -> Result<String> {
    access(state, |l| l.reverse_transaction(&id, &reason))
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(notifications::init())
        .plugin(search_export::init())
        .plugin(data_files::init())
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("ledger.sqlite3");
            let ledger = Ledger::open(&path).map_err(std::io::Error::other)?;
            app.manage(LedgerState(Mutex::new(ledger)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            privacy_commands::privacy_status,
            privacy_commands::privacy_unlock,
            privacy_commands::privacy_lock_now,
            privacy_commands::privacy_touch,
            privacy_commands::privacy_enable_lock,
            privacy_commands::privacy_disable_lock,
            privacy_commands::privacy_set_hide_amounts,
            privacy_commands::privacy_set_auto_lock_minutes,
            privacy_commands::privacy_delete_preview,
            privacy_commands::privacy_delete_confirm,
            notifications::notification_state,
            notifications::save_notification_source,
            notifications::notification_access,
            notifications::open_notification_settings,
            recurring::commands::recurring_state,
            recurring::commands::write_recurring,
            recurring::commands::set_recurring_enabled,
            recurring::commands::skip_recurring,
            recurring::commands::recurring_instances,
            search_rules::classification_state,
            search_rules::preview_classification_rule,
            search_rules::write_classification_rule,
            search_rules::write_category,
            search_rules::save_merchant_alias,
            search_rules::delete_merchant_alias,
            search_export::save_search_csv,
            reconciliation::commands::reconciliation_list,
            reconciliation::commands::reconciliation_summary,
            reconciliation::commands::reconciliation_start,
            reconciliation::commands::reconciliation_review,
            reconciliation::commands::reconciliation_adjust,
            reconciliation::commands::reconciliation_close,
            reconciliation::commands::reconciliation_reopen,
            data_management::create_backup,
            data_management::inspect_backup,
            data_management::restore_backup,
            data_management::export_data,
            data_files::save_data_file,
            data_files::open_backup_file,
            list_accounts,
            create_account,
            update_platform,
            get_day,
            save_entry,
            set_reference,
            clear_reference,
            query_entries,
            ignore_pending,
            restore_pending,
            preview_bill,
            stage_bill,
            confirm_pending,
            edit_pending,
            list_categories,
            transaction_detail,
            write_transaction,
            reverse_transaction,
            analytics::commands::report_summary,
            analytics::commands::list_budgets,
            analytics::commands::write_budget,
            analytics::commands::archive_budget,
            analytics::commands::budget_reminders
        ])
        .run(tauri::generate_context!())
        .expect("启动 MeowM 失败");
}
