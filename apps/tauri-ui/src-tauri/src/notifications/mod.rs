//! Notification commands and the platform-independent JNI payload boundary.
mod commands;
#[cfg(target_os = "android")]
mod jni;
#[cfg(test)]
mod tests;
pub use commands::*;
use ledger_app::ports::NotificationRepository;
use serde::Serialize;
#[cfg(any(target_os = "android", test))]
use std::path::Path;
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::plugin::{Builder, TauriPlugin};

#[cfg(any(target_os = "android", test))]
#[derive(Serialize)]
pub struct CaptureReply {
    code: String,
    transaction_id: Option<String>,
}
#[cfg(any(target_os = "android", test))]
fn capture_json(path: &Path, payload: &str) -> String {
    let result = std::panic::catch_unwind(|| {
        if payload.len() > 65_536 {
            return None;
        }
        let event: ledger_notifications::NotificationEvent = serde_json::from_str(payload).ok()?;
        let repo = ledger_storage::sqlite::SqliteLedgerRepository::open(path).ok()?;
        let result = ledger_notifications::capture_notification(&repo, &event).ok()?;
        Some(CaptureReply {
            code: result.code,
            transaction_id: result.transaction_id.map(|id| id.as_uuid().to_string()),
        })
    })
    .ok()
    .flatten();
    serde_json::to_string(&result.unwrap_or(CaptureReply {
        code: "error".into(),
        transaction_id: None,
    }))
    .expect("serializable reply")
}
#[cfg(target_os = "android")]
struct Platform<R: tauri::Runtime>(tauri::plugin::PluginHandle<R>);
pub fn init<R: tauri::Runtime>() -> TauriPlugin<R> {
    Builder::new("notification-access")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _app.manage(Platform(_api.register_android_plugin(
                "com.example.localledger",
                "NotificationPlugin",
            )?));
            Ok(())
        })
        .build()
}
