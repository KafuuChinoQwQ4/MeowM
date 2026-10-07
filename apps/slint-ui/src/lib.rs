//! Slint composition root. All ledger changes go through ledger-app.
mod adapter;
mod import_review;
use std::path::PathBuf;
slint::include_modules!();
pub fn run(path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.unwrap_or_else(|| {
        std::env::var_os("MEOWM_DATABASE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("ledger.sqlite3"))
    });
    let repository = ledger_storage::sqlite::SqliteLedgerRepository::open(&path)?;
    let window = LedgerWindow::new()?;
    adapter::connect(&window, ledger_app::MobileLedger::new(repository))?;
    window.run()?;
    Ok(())
}
#[cfg(target_os = "android")]
thread_local! { static ANDROID_APP: std::cell::RefCell<Option<slint::android::AndroidApp>> = const { std::cell::RefCell::new(None) }; }
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let path = app.internal_data_path().map(|p| p.join("ledger.sqlite3"));
    ANDROID_APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));
    if slint::android::init(app).is_ok() {
        let _ = run(path);
    }
}
#[cfg(target_os = "android")]
fn open_notification_settings() -> Result<(), String> {
    ANDROID_APP.with(|slot| {
        let slot = slot.borrow();
        let app = slot.as_ref().ok_or("Android activity unavailable")?;
        // The Kotlin method performs only the Android Activity transition.
        unsafe {
            let vm = jni::JavaVM::from_raw(app.vm_as_ptr().cast())
                .map_err(|_| "Android VM unavailable")?;
            let mut env = vm
                .attach_current_thread()
                .map_err(|_| "Android thread unavailable")?;
            let activity = jni::objects::JObject::from_raw(app.activity_as_ptr().cast());
            env.call_method(&activity, "openNotificationSettings", "()V", &[])
                .map_err(|_| "Could not open Android notification settings")?;
        }
        Ok(())
    })
}
#[cfg(not(target_os = "android"))]
fn open_notification_settings() -> Result<(), String> {
    Err("Notification capture is available in the Android app.".into())
}

#[cfg(target_os = "android")]
fn choose_document(kind: i32, bytes: &[u8]) -> Result<(), String> {
    ANDROID_APP.with(|slot| {
        let slot = slot.borrow();
        let app = slot.as_ref().ok_or("Android activity unavailable")?;
        unsafe {
            let vm = jni::JavaVM::from_raw(app.vm_as_ptr().cast())
                .map_err(|_| "Android VM unavailable")?;
            let mut env = vm
                .attach_current_thread()
                .map_err(|_| "Android thread unavailable")?;
            let activity = jni::objects::JObject::from_raw(app.activity_as_ptr().cast());
            let data = env.byte_array_from_slice(bytes).map_err(|_| "文件过大")?;
            env.call_method(
                &activity,
                "chooseDocument",
                "(I[B)V",
                &[kind.into(), (&data).into()],
            )
            .map_err(|_| "无法打开文件选择器")?;
            env.delete_local_ref(data).map_err(|_| "文件缓存释放失败")?;
        }
        Ok(())
    })
}
#[cfg(target_os = "android")]
fn document_result() -> Result<String, String> {
    ANDROID_APP.with(|slot| {
        let slot = slot.borrow();
        let app = slot.as_ref().ok_or("Android activity unavailable")?;
        unsafe {
            let vm = jni::JavaVM::from_raw(app.vm_as_ptr().cast())
                .map_err(|_| "Android VM unavailable")?;
            let mut env = vm
                .attach_current_thread()
                .map_err(|_| "Android thread unavailable")?;
            let activity = jni::objects::JObject::from_raw(app.activity_as_ptr().cast());
            let result = env
                .call_method(&activity, "takeDocumentResult", "()Ljava/lang/String;", &[])
                .map_err(|_| "文件选择器结果读取失败")?
                .l()
                .map_err(|_| "结果无效")?;
            let result = jni::objects::JString::from(result);
            let value: String = env.get_string(&result).map_err(|_| "结果无效")?.into();
            env.delete_local_ref(result).map_err(|_| "结果释放失败")?;
            Ok(value)
        }
    })
}

#[cfg(not(target_os = "android"))]
fn local_deletion(_finish: bool) -> Result<(), String> {
    Ok(())
}
#[cfg(target_os = "android")]
fn local_deletion(finish: bool) -> Result<(), String> {
    ANDROID_APP.with(|slot| {
        let slot = slot.borrow();
        let app = slot.as_ref().ok_or("Android activity unavailable")?;
        unsafe {
            let vm = jni::JavaVM::from_raw(app.vm_as_ptr().cast())
                .map_err(|_| "Android VM unavailable")?;
            let mut env = vm
                .attach_current_thread()
                .map_err(|_| "Android thread unavailable")?;
            let activity = jni::objects::JObject::from_raw(app.activity_as_ptr().cast());
            let ok = env
                .call_method(&activity, "deleteLocalData", "(Z)Z", &[finish.into()])
                .and_then(|v| v.z())
                .unwrap_or(false);
            if !ok {
                return Err("本机清理未完成，请重试全部删除；账本与安全存储分别提交".into());
            }
        }
        Ok(())
    })
}
