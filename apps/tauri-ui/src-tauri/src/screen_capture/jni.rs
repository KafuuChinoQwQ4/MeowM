use super::*;
use ::jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jstring},
};
fn with_session<T>(session: &str, f: impl FnOnce(&crate::ledger::Ledger, &str) -> T) -> Option<T> {
    let app = APP.get()?;
    let state = app.try_state::<LedgerState>()?;
    let l = state.0.lock().ok()?;
    Some(f(&l, session))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeScreenCapture(
    mut env: JNIEnv,
    _class: JClass,
    session: JString,
    payload: JString,
) -> jstring {
    let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let session: String = env.get_string(&session).ok()?.into();
        let payload: String = env.get_string(&payload).ok()?.into();
        with_session(&session, |l, s| capture(l, s, &payload))
    }))
    .ok()
    .flatten()
    .unwrap_or_else(|| r#"{"code":"stopped"}"#.into());
    env.new_string(reply)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeScreenAllowed(
    mut env: JNIEnv,
    _class: JClass,
    session: JString,
    package: JString,
) -> jboolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let session: String = env.get_string(&session).ok()?.into();
        let package: String = env.get_string(&package).ok()?.into();
        with_session(&session, |l, s| {
            allowed(l, s)
                && l.repo()
                    .ok()
                    .and_then(|r| r.screen_settings().ok())
                    .is_some_and(|v| {
                        v.iter()
                            .any(|s| s.enabled && (package.is_empty() || s.package_name == package))
                    })
        })
    }))
    .ok()
    .flatten()
    .unwrap_or(false) as jboolean
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeScreenStop(
    mut env: JNIEnv,
    _class: JClass,
    session: JString,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let session: String = env.get_string(&session).ok()?.into();
        with_session(&session, close_session)
    }));
}
