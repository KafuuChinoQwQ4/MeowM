mod ledger;
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
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("ledger.sqlite3");
            let ledger = Ledger::open(&path).map_err(std::io::Error::other)?;
            app.manage(LedgerState(Mutex::new(ledger)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_accounts,
            create_account,
            update_platform,
            get_day,
            save_entry,
            set_reference,
            clear_reference,
            query_entries
        ])
        .run(tauri::generate_context!())
        .expect("启动 MeowM 失败");
}
