//! WorkManager can synchronize without the Activity or Tauri runtime.
use ::jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::jstring,
};
use ledger_storage::sqlite::SqliteLedgerRepository;

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeSyncExchangeRates(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let reply = (|| {
            let path: String = env.get_string(&path).ok()?.into();
            if path.is_empty() || path.len() > 4096 {
                return None;
            }
            let repo = SqliteLedgerRepository::open(std::path::Path::new(&path)).ok()?;
            let state = super::service::sync(&repo, &super::provider::HttpRateProvider::default(), chrono::Utc::now(), false).ok()?;
            let latest = repo.newest_exchange_rate_date().ok()?.map(|day| day.to_string());
            Some(serde_json::json!({"status":state.status, "latest_date":latest, "error_code":state.error_code}).to_string())
        })().filter(|reply| reply.len() <= 1024)
        .unwrap_or_else(|| r#"{"status":"invalid_data","error_code":"sync_failed"}"#.into());
        env.new_string(reply).map(|value| value.into_raw()).unwrap_or(std::ptr::null_mut())
    })).unwrap_or(std::ptr::null_mut())
}
