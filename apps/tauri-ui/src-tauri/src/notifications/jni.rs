//! No UI state dependency: the Android service can start before the WebView.
use ::jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jstring},
};
use ledger_app::ports::NotificationRepository;
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeCapture(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    payload: JString,
) -> jstring {
    let reply = (|| {
        let path: String = env.get_string(&path).ok()?.into();
        let payload: String = env.get_string(&payload).ok()?.into();
        Some(super::capture_json(std::path::Path::new(&path), &payload))
    })()
    .unwrap_or_else(|| r#"{"code":"error","transaction_id":null}"#.into());
    env.new_string(reply)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeEnabled(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    package: JString,
) -> jboolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let path: String = env.get_string(&path).ok()?.into();
        let package: String = env.get_string(&package).ok()?.into();
        let repo =
            ledger_storage::sqlite::SqliteLedgerRepository::open(std::path::Path::new(&path))
                .ok()?;
        Some(
            repo.notification_settings()
                .ok()?
                .iter()
                .any(|s| s.package_name == package && s.enabled),
        )
    }))
    .ok()
    .flatten()
    .unwrap_or(false) as jboolean
}
