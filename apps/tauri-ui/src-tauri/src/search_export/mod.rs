#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::plugin::{Builder, TauriPlugin};
#[cfg(target_os = "android")]
struct Platform<R: tauri::Runtime>(tauri::plugin::PluginHandle<R>);
pub fn init<R: tauri::Runtime>() -> TauriPlugin<R> {
    Builder::new("search-export")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _app.manage(Platform(_api.register_android_plugin(
                "com.example.localledger",
                "SearchExportPlugin",
            )?));
            Ok(())
        })
        .build()
}
#[tauri::command]
pub async fn save_search_csv(app: tauri::AppHandle, csv: String) -> Result<(), String> {
    if csv.is_empty() || csv.len() > 16 * 1024 * 1024 {
        return Err("导出内容为空或超过 16 MB".into());
    }
    #[cfg(target_os = "android")]
    {
        app.state::<Platform<tauri::Wry>>()
            .0
            .run_mobile_plugin_async::<serde_json::Value>("saveCsv", serde_json::json!({"csv":csv}))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err("系统另存为仅适用于 Android".into())
    }
}
