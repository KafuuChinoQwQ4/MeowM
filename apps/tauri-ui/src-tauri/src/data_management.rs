//! Backup previews hold validated data, never passwords; replacement requires a fresh ledger revision.
use crate::{LedgerState, ledger::PageQuery};
use base64::{Engine, engine::general_purpose::STANDARD};
use ledger_backup::{BackupInfo, BackupService, InspectedBackup};
use ledger_export::{ExportFormat, ExportService};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::State;
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, String>;
pub(crate) struct RestoreSession {
    id: String,
    created: Instant,
    backup: InspectedBackup,
    expected: [u8; 32],
}
#[derive(Serialize)]
pub struct DataFile {
    name: String,
    mime: String,
    data: String,
    count: Option<usize>,
}
#[derive(Serialize)]
pub struct RestorePreview {
    id: String,
    backup: BackupInfo,
    current_counts: Vec<(String, usize)>,
}
fn file(bytes: Vec<u8>, extension: &str, mime: &str, count: Option<usize>) -> DataFile {
    DataFile {
        name: format!(
            "meowm-{}.{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            extension
        ),
        mime: mime.into(),
        data: STANDARD.encode(bytes),
        count,
    }
}
#[tauri::command]
pub fn create_backup(state: State<'_, LedgerState>, password: String) -> Result<DataFile> {
    let password = Zeroizing::new(password);
    crate::access(state, |l| {
        let bytes = BackupService::create(&l.repo()?, &password).map_err(|e| e.to_string())?;
        Ok(file(bytes, "meowm", "application/octet-stream", None))
    })
}
#[tauri::command]
pub fn inspect_backup(
    state: State<'_, LedgerState>,
    data: String,
    password: String,
) -> Result<RestorePreview> {
    let password = Zeroizing::new(password);
    crate::access(state, |l| {
        l.restore_preview.replace(None);
        if data.len() > 45 * 1024 * 1024 {
            return Err("文件超过 32 MB".into());
        }
        let bytes = STANDARD.decode(data).map_err(|_| "备份文件编码无效")?;
        let backup = BackupService::inspect(&bytes, &password).map_err(|e| e.to_string())?;
        let current = l.repo()?.snapshot().map_err(|_| "无法检查当前账本")?;
        let expected = current.fingerprint().map_err(|_| "无法检查当前账本")?;
        let id = uuid::Uuid::new_v4().to_string();
        let preview = RestorePreview {
            id: id.clone(),
            backup: backup.info.clone(),
            current_counts: current.counts(),
        };
        l.restore_preview.replace(Some(RestoreSession {
            id,
            created: Instant::now(),
            backup,
            expected,
        }));
        Ok(preview)
    })
}
#[tauri::command]
pub fn restore_backup(
    state: State<'_, LedgerState>,
    preview_id: String,
    confirmed: bool,
) -> Result<()> {
    crate::access(state, |l| {
        if !confirmed {
            return Err("请先确认替换范围".into());
        }
        let session = l.restore_preview.take().ok_or("请重新检查备份后再恢复")?;
        if session.id != preview_id || session.created.elapsed() > Duration::from_secs(600) {
            return Err("备份预览已过期，请重新检查".into());
        }
        BackupService::restore(&l.repo()?, &session.backup, session.expected)
            .map_err(|e| e.to_string())?;
        l.privacy
            .borrow_mut()
            .reload_settings(&l.repo()?)
            .map_err(|e| e.to_string())?;
        l.clear_import_preview();
        l.delete_preview.replace(None);
        Ok(())
    })
}
#[tauri::command]
pub fn export_data(
    state: State<'_, LedgerState>,
    query: PageQuery,
    format: String,
    bom: bool,
    include_unconfirmed: bool,
) -> Result<DataFile> {
    crate::access(state, |l| {
        let format = match format.as_str() {
            "csv" => ExportFormat::Csv { bom },
            "json" => ExportFormat::Json,
            _ => return Err("无效导出格式".into()),
        };
        let query = query.into_domain().map_err(|e| e.message)?;
        let (bytes, count) = ExportService::export(&l.repo()?, query, include_unconfirmed, format)
            .map_err(|_| "导出失败，请检查筛选条件；数据过多时请缩小范围")?;
        Ok(match format {
            ExportFormat::Csv { .. } => file(bytes, "csv", "text/csv", Some(count)),
            ExportFormat::Json => file(bytes, "json", "application/json", Some(count)),
        })
    })
}
