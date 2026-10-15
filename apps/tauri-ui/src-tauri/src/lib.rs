mod analytics;
mod ledger;
mod notifications;
mod recurring;
mod transactions;
use ledger::{Account, DayView, Ledger};
use std::sync::Mutex;
use tauri::{Manager, State};
type Result<T> = std::result::Result<T, String>;
struct LedgerState(Mutex<Ledger>);
fn access<T>(state: State<'_, LedgerState>, f: impl FnOnce(&Ledger) -> Result<T>) -> Result<T> {
    let ledger = state.0.lock().map_err(|_| "账本忙，请重启应用")?;
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
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("ledger.sqlite3");
            let ledger = Ledger::open(&path).map_err(std::io::Error::other)?;
            app.manage(LedgerState(Mutex::new(ledger)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            notifications::notification_state,
            notifications::save_notification_source,
            notifications::notification_access,
            notifications::open_notification_settings,
            recurring::commands::recurring_state,
            recurring::commands::write_recurring,
            recurring::commands::set_recurring_enabled,
            recurring::commands::skip_recurring,
            recurring::commands::recurring_instances,
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
