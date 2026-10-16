//! Unauthenticated status/unlock are isolated here; ledger commands use the shared lock guard.
use crate::{LedgerState, Result, access};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::State;
use zeroize::Zeroizing;

pub(crate) struct DeleteSession {
    id: String,
    created: Instant,
    scope: String,
    expected: [u8; 32],
}
#[derive(Serialize)]
pub struct PrivacyStatus {
    locked: bool,
    lock_enabled: bool,
    hide_amounts: bool,
    auto_lock_minutes: u16,
}
#[derive(Serialize)]
pub struct DeletePreview {
    id: String,
    scope: String,
    counts: Vec<(String, usize)>,
    pending: usize,
    confirmed: usize,
    receipts: usize,
    blocked: usize,
}
fn status(l: &crate::ledger::Ledger) -> PrivacyStatus {
    let mut privacy = l.privacy.borrow_mut();
    let _ = privacy.require_unlocked();
    if privacy.is_locked() {
        l.clear_import_preview();
        l.restore_preview.replace(None);
        l.delete_preview.replace(None);
    }
    PrivacyStatus {
        locked: privacy.is_locked(),
        lock_enabled: privacy.settings().lock_enabled,
        hide_amounts: privacy.settings().hide_amounts,
        auto_lock_minutes: privacy.settings().auto_lock_minutes,
    }
}
#[tauri::command]
pub fn privacy_status(state: State<'_, LedgerState>) -> Result<PrivacyStatus> {
    let l = state.0.lock().map_err(|_| "账本忙，请重试")?;
    Ok(status(&l))
}
#[tauri::command]
pub fn privacy_unlock(state: State<'_, LedgerState>, password: String) -> Result<PrivacyStatus> {
    let password = Zeroizing::new(password);
    let l = state.0.lock().map_err(|_| "账本忙，请重试")?;
    l.privacy
        .borrow_mut()
        .unlock(&password)
        .map_err(|e| e.to_string())?;
    Ok(status(&l))
}
#[tauri::command]
pub fn privacy_lock_now(state: State<'_, LedgerState>) -> Result<PrivacyStatus> {
    let l = state.0.lock().map_err(|_| "账本忙，请重试")?;
    l.privacy.borrow_mut().lock_now();
    l.clear_import_preview();
    l.restore_preview.replace(None);
    l.delete_preview.replace(None);
    Ok(status(&l))
}
#[tauri::command]
pub fn privacy_touch(state: State<'_, LedgerState>) -> Result<()> {
    let l = state.0.lock().map_err(|_| "账本忙，请重试")?;
    l.privacy.borrow_mut().touch().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn privacy_enable_lock(
    state: State<'_, LedgerState>,
    password: String,
) -> Result<PrivacyStatus> {
    let password = Zeroizing::new(password);
    access(state, |l| {
        l.delete_preview.replace(None);
        l.privacy
            .borrow_mut()
            .enable_lock(&l.repo()?, &password)
            .map_err(|e| e.to_string())?;
        Ok(status(l))
    })
}
#[tauri::command]
pub fn privacy_disable_lock(
    state: State<'_, LedgerState>,
    password: String,
) -> Result<PrivacyStatus> {
    let password = Zeroizing::new(password);
    access(state, |l| {
        l.delete_preview.replace(None);
        l.privacy
            .borrow_mut()
            .disable_lock(&l.repo()?, &password)
            .map_err(|e| e.to_string())?;
        Ok(status(l))
    })
}
#[tauri::command]
pub fn privacy_set_hide_amounts(
    state: State<'_, LedgerState>,
    hidden: bool,
) -> Result<PrivacyStatus> {
    access(state, |l| {
        l.delete_preview.replace(None);
        l.privacy
            .borrow_mut()
            .set_hide_amounts(&l.repo()?, hidden)
            .map_err(|e| e.to_string())?;
        Ok(status(l))
    })
}
#[tauri::command]
pub fn privacy_set_auto_lock_minutes(
    state: State<'_, LedgerState>,
    minutes: u16,
) -> Result<PrivacyStatus> {
    access(state, |l| {
        l.delete_preview.replace(None);
        l.privacy
            .borrow_mut()
            .set_auto_lock_minutes(&l.repo()?, minutes)
            .map_err(|e| e.to_string())?;
        Ok(status(l))
    })
}
#[tauri::command]
pub fn privacy_delete_preview(
    state: State<'_, LedgerState>,
    scope: String,
) -> Result<DeletePreview> {
    access(state, |l| {
        l.delete_preview.replace(None);
        if scope != "notifications" && scope != "all" {
            return Err("无效的删除范围".into());
        }
        let repo = l.repo()?;
        let snapshot = repo.snapshot().map_err(|_| "无法检查当前账本")?;
        let source = if scope == "notifications" {
            l.privacy
                .borrow_mut()
                .preview_source_data(&repo)
                .map_err(|e| e.to_string())?
        } else {
            ledger_app::privacy::SourceDeletePreview {
                pending: 0,
                confirmed: 0,
                receipts: 0,
                blocked: 0,
            }
        };
        let id = uuid::Uuid::new_v4().to_string();
        l.delete_preview.replace(Some(DeleteSession {
            id: id.clone(),
            created: Instant::now(),
            scope: scope.clone(),
            expected: snapshot.fingerprint().map_err(|_| "无法检查当前账本")?,
        }));
        Ok(DeletePreview {
            id,
            scope,
            counts: snapshot.counts(),
            pending: source.pending,
            confirmed: source.confirmed,
            receipts: source.receipts,
            blocked: source.blocked,
        })
    })
}
#[tauri::command]
pub fn privacy_delete_confirm(
    state: State<'_, LedgerState>,
    preview_id: String,
    phrase: String,
) -> Result<PrivacyStatus> {
    access(state, |l| {
        let session = l.delete_preview.take().ok_or("请重新预览删除范围")?;
        if session.id != preview_id || session.created.elapsed() > Duration::from_secs(600) {
            return Err("删除预览已过期，请重新预览".into());
        }
        let required = if session.scope == "all" {
            "删除全部数据"
        } else {
            "删除通知数据"
        };
        if phrase != required {
            return Err(format!("请输入“{required}”确认"));
        }
        let repo = l.repo()?;
        if session.scope == "all" {
            l.privacy
                .borrow_mut()
                .delete_all_data(&repo, session.expected)
                .map_err(|e| e.to_string())?;
        } else {
            l.privacy
                .borrow_mut()
                .delete_source_data(&repo, session.expected)
                .map_err(|e| e.to_string())?;
        }
        l.clear_import_preview();
        l.restore_preview.replace(None);
        Ok(status(l))
    })
}
