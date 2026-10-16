//! Android Storage Access Framework bridge: no arbitrary filesystem paths cross IPC.
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::plugin::{Builder, TauriPlugin};
#[cfg(target_os = "android")]
struct Platform<R: tauri::Runtime>(tauri::plugin::PluginHandle<R>);
#[derive(Serialize, Deserialize)]
pub struct SelectedBackup {
    data: String,
}
pub fn init<R: tauri::Runtime>() -> TauriPlugin<R> {
    Builder::new("data-files")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _app.manage(Platform(_api.register_android_plugin(
                "com.example.localledger",
                "DataFilesPlugin",
            )?));
            Ok(())
        })
        .build()
}
#[tauri::command]
pub async fn save_data_file(
    app: tauri::AppHandle,
    name: String,
    mime: String,
    data: String,
) -> Result<(), String> {
    if name.len() > 100
        || !name.starts_with("meowm-")
        || name.contains(['/', '\\'])
        || !["application/octet-stream", "text/csv", "application/json"].contains(&mime.as_str())
        || data.len() > 45 * 1024 * 1024
    {
        return Err("文件名、格式或大小无效".into());
    }
    #[cfg(target_os = "android")]
    {
        app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async::<serde_json::Value>(
                "saveFile",
                serde_json::json!({"name":name,"mime":mime,"data":data}),
            )
            .await
            .map_err(|_| "保存失败或已取消".to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, name, mime, data);
        Err("请使用浏览器下载功能".into())
    }
}
#[tauri::command]
pub async fn open_backup_file(app: tauri::AppHandle) -> Result<SelectedBackup, String> {
    #[cfg(target_os = "android")]
    {
        app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async("openBackup", serde_json::json!({}))
            .await
            .map_err(|_| "读取失败或已取消".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err("请使用文件选择器".into())
    }
}
