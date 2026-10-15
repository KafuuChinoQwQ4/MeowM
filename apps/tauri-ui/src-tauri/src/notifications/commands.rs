use super::*;
use crate::{LedgerState, Result, access};
use serde::Deserialize;
use tauri::State;
#[derive(Serialize, Deserialize)]
pub struct AccessStatus {
    android: bool,
    granted: bool,
    connected: bool,
}
#[derive(Serialize, Deserialize)]
pub struct SourceSettings {
    package_name: String,
    enabled: bool,
    account_id: Option<String>,
}
#[derive(Serialize)]
pub struct LastResult {
    package_name: String,
    code: String,
    received_at_ms: i64,
    transaction_id: Option<String>,
}
#[derive(Serialize)]
pub struct NotificationState {
    sources: Vec<SourceSettings>,
    latest: Vec<LastResult>,
}
#[tauri::command]
pub fn notification_state(state: State<'_, LedgerState>) -> Result<NotificationState> {
    access(state, |ledger| {
        let repo = ledger.repo()?;
        let sources = repo
            .notification_settings()
            .map_err(|_| "无法读取通知来源设置")?
            .into_iter()
            .map(|s| SourceSettings {
                package_name: s.package_name,
                enabled: s.enabled,
                account_id: s.account_id.map(|id| id.as_uuid().to_string()),
            })
            .collect();
        let latest = repo
            .notification_status()
            .map_err(|_| "无法读取采集状态")?
            .into_iter()
            .map(|s| LastResult {
                package_name: s.package_name,
                code: s.code,
                received_at_ms: s.received_at_ms,
                transaction_id: s.transaction_id.map(|id| id.as_uuid().to_string()),
            })
            .collect();
        Ok(NotificationState { sources, latest })
    })
}
#[tauri::command]
pub fn save_notification_source(
    state: State<'_, LedgerState>,
    source: SourceSettings,
    consent: bool,
) -> Result<()> {
    access(state, |ledger| {
        let repo = ledger.repo()?;
        let old = repo
            .notification_settings()
            .map_err(|_| "无法读取通知来源设置")?
            .into_iter()
            .find(|s| s.package_name == source.package_name)
            .ok_or("不支持的通知来源")?;
        if source.enabled && !old.enabled && !consent {
            return Err("请先阅读并同意通知采集说明".into());
        }
        repo.save_notification_settings(ledger_app::notifications::NotificationSettings {
            package_name: source.package_name,
            enabled: source.enabled,
            account_id: source
                .account_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(crate::ledger::account_id)
                .transpose()?,
        })
        .map_err(|_| "无法保存来源，请选择有效的人民币平台".into())
    })
}
#[tauri::command]
pub async fn notification_access(_app: tauri::AppHandle) -> Result<AccessStatus> {
    #[cfg(target_os = "android")]
    {
        _app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async("status", ())
            .await
            .map_err(|_| "无法读取系统授权状态".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(AccessStatus {
            android: false,
            granted: false,
            connected: false,
        })
    }
}
#[tauri::command]
pub async fn open_notification_settings(_app: tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        _app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async::<serde_json::Value>("openSettings", ())
            .await
            .map(|_| ())
            .map_err(|_| "无法打开系统通知访问设置，请从系统设置手动进入".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("请在 Android 设备中设置通知访问权限".into())
    }
}
