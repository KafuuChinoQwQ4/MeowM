//! Screen capture shares the UI's lock and session; JNI never opens a second privacy context.
use crate::{LedgerState, Result, access};
use ledger_app::ports::ScreenCaptureRepository;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    State,
    plugin::{Builder, TauriPlugin},
};
#[cfg(target_os = "android")]
mod jni;
#[cfg(test)]
mod tests;
#[cfg(target_os = "android")]
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
#[cfg(target_os = "android")]
struct Platform<R: tauri::Runtime>(tauri::plugin::PluginHandle<R>);

#[derive(Serialize, Deserialize)]
pub struct Source {
    pub package_name: String,
    pub enabled: bool,
    pub account_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Status {
    pub android: bool,
    pub running: bool,
    pub usage_granted: bool,
    pub code: String,
}
#[derive(Serialize)]
pub struct CaptureState {
    sources: Vec<Source>,
    latest: Vec<Last>,
}
#[derive(Serialize)]
pub struct Last {
    package_name: String,
    code: String,
    received_at_ms: i64,
    transaction_id: Option<String>,
}
pub fn init<R: tauri::Runtime>() -> TauriPlugin<R> {
    Builder::new("screen-capture")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _app.manage(Platform(_api.register_android_plugin(
                "com.example.localledger",
                "ScreenCapturePlugin",
            )?));
            Ok(())
        })
        .build()
}
#[cfg(target_os = "android")]
pub fn attach(app: tauri::AppHandle) {
    let _ = APP.set(app);
}
#[tauri::command]
pub fn screen_capture_state(state: State<'_, LedgerState>) -> Result<CaptureState> {
    let l = state.0.lock().map_err(|_| "账本忙，请重试")?;
    l.require_unlocked()?;
    {
        let repo = l.repo()?;
        Ok(CaptureState {
            sources: repo
                .screen_settings()
                .map_err(|_| "无法读取屏幕来源")?
                .into_iter()
                .map(|s| Source {
                    package_name: s.package_name,
                    enabled: s.enabled,
                    account_id: s.account_id.map(|id| id.as_uuid().to_string()),
                })
                .collect(),
            latest: repo
                .screen_status()
                .map_err(|_| "无法读取识别结果")?
                .into_iter()
                .map(|s| Last {
                    package_name: s.package_name,
                    code: s.code,
                    received_at_ms: s.received_at_ms,
                    transaction_id: s.transaction_id.map(|id| id.as_uuid().to_string()),
                })
                .collect(),
        })
    }
}
#[tauri::command]
pub fn save_screen_capture_source(
    state: State<'_, LedgerState>,
    source: Source,
    consent: bool,
) -> Result<()> {
    access(state, |l| {
        if source.enabled && !consent {
            return Err("请先同意屏幕识别用途".into());
        }
        l.repo()?
            .save_screen_settings(ledger_app::notifications::NotificationSettings {
                package_name: source.package_name,
                enabled: source.enabled,
                account_id: source
                    .account_id
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(crate::ledger::account_id)
                    .transpose()?,
            })
            .map_err(|_| "无法保存屏幕来源设置".into())
    })
}
#[tauri::command]
pub async fn screen_capture_status(_app: tauri::AppHandle) -> Result<Status> {
    #[cfg(target_os = "android")]
    {
        _app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async("status", ())
            .await
            .map_err(|_| "无法读取屏幕识别状态".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(Status {
            android: false,
            running: false,
            usage_granted: false,
            code: "unsupported".into(),
        })
    }
}
#[tauri::command]
pub async fn screen_capture_usage_settings(_app: tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        _app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async::<serde_json::Value>("openUsageSettings", ())
            .await
            .map(|_| ())
            .map_err(|_| "无法打开使用情况访问设置".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("仅 Android 支持此功能".into())
    }
}
#[tauri::command]
pub async fn start_screen_capture(
    app: tauri::AppHandle,
    state: State<'_, LedgerState>,
    consent: bool,
) -> Result<()> {
    if !consent {
        return Err("请先同意屏幕识别用途".into());
    }
    let session = access(state.clone(), |l| {
        if l.capture_session.borrow().is_some() {
            return Err("已有识别会话，请先停止".into());
        }
        if !l
            .repo()?
            .screen_settings()
            .map_err(|_| "无法读取屏幕来源")?
            .iter()
            .any(|s| s.enabled)
        {
            return Err("请先启用至少一个屏幕来源".into());
        }
        let session = uuid::Uuid::new_v4().to_string();
        l.capture_session.replace(Some(session.clone()));
        Ok(session)
    })?;
    #[cfg(target_os = "android")]
    let result = app
        .state::<Platform<tauri::Wry>>()
        .0
        .run_mobile_plugin_async::<serde_json::Value>(
            "request",
            serde_json::json!({"session":session}),
        )
        .await
        .map(|_| ())
        .map_err(|_| "屏幕授权已取消或启动失败，请检查通知及屏幕捕获权限".to_string());
    #[cfg(not(target_os = "android"))]
    let result: Result<()> = {
        let _ = app;
        Err("仅 Android 支持此功能".into())
    };
    if result.is_err() {
        if let Ok(l) = state.0.lock() {
            close_session(&l, &session);
        }
    }
    result
}
#[tauri::command]
pub async fn stop_screen_capture(
    _app: tauri::AppHandle,
    state: State<'_, LedgerState>,
) -> Result<()> {
    state
        .0
        .lock()
        .map_err(|_| "账本忙，请重试")?
        .capture_session
        .replace(None);
    #[cfg(target_os = "android")]
    _app.state::<Platform<tauri::Wry>>()
        .0
        .run_mobile_plugin_async::<serde_json::Value>("stop", ())
        .await
        .map_err(|_| "无法停止系统捕获服务".to_string())?;
    Ok(())
}
pub(crate) fn close_session(l: &crate::ledger::Ledger, session: &str) {
    if l.capture_session.borrow().as_deref() == Some(session) {
        l.capture_session.replace(None);
    }
}
#[cfg(any(target_os = "android", test))]
fn allowed(l: &crate::ledger::Ledger, session: &str) -> bool {
    if l.require_unlocked().is_err() {
        l.capture_session.replace(None);
        return false;
    }
    !session.is_empty() && l.capture_session.borrow().as_deref() == Some(session)
}
#[cfg(any(target_os = "android", test))]
fn capture(l: &crate::ledger::Ledger, session: &str, payload: &str) -> String {
    if !allowed(l, session) {
        return r#"{"code":"stopped"}"#.into();
    }
    let result = (|| {
        if payload.len() > 65_536 {
            return None;
        }
        let event: ledger_notifications::ScreenCaptureEvent = serde_json::from_str(payload).ok()?;
        if !event.event_id.starts_with(&format!("{session}:")) {
            return None;
        }
        let result = ledger_notifications::capture_screen(&l.repo().ok()?, &event).ok()?;
        Some(serde_json::json!({"code":result.code,"transaction_id":result.transaction_id.map(|id|id.as_uuid().to_string())}).to_string())
    })();
    result.unwrap_or_else(|| r#"{"code":"error"}"#.into())
}
