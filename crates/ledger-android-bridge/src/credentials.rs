//! Minimal JNI implementation of the secure-store port. Kotlin owns Keystore and
//! ciphertext files; byte arrays are transient and errors never contain tokens.
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JObject, JString, JValue},
    sys::{jboolean, jbyteArray},
};
use ledger_app::ports::{CredentialError, CredentialStore, ProviderId, SecretToken};

pub struct AndroidCredentialStore<'env, 'local> {
    pub env: &'env mut JNIEnv<'local>,
    pub object: JObject<'local>,
}
impl CredentialStore for AndroidCredentialStore<'_, '_> {
    fn load(&mut self, p: ProviderId) -> Result<Option<SecretToken>, CredentialError> {
        let provider = self.env.new_string(p.key()).map_err(|_| CredentialError)?;
        let value = self
            .env
            .call_method(
                &self.object,
                "load",
                "(Ljava/lang/String;)[B",
                &[JValue::Object(&provider)],
            )
            .map_err(|_| {
                let _ = self.env.exception_clear();
                CredentialError
            })?
            .l()
            .map_err(|_| CredentialError)?;
        if value.is_null() {
            return Ok(None);
        }
        let array = JByteArray::from(value);
        let bytes = self
            .env
            .convert_byte_array(&array)
            .map_err(|_| CredentialError)?;
        let secret = SecretToken::new(bytes);
        self.env
            .set_byte_array_region(&array, 0, &vec![0; secret.expose().len()])
            .map_err(|_| CredentialError)?;
        if secret.expose().is_empty() || secret.expose().len() > 16_384 {
            return Err(CredentialError);
        }
        Ok(Some(secret))
    }
    fn save(&mut self, p: ProviderId, token: &SecretToken) -> Result<(), CredentialError> {
        if token.expose().is_empty() || token.expose().len() > 16_384 {
            return Err(CredentialError);
        }
        let provider = self.env.new_string(p.key()).map_err(|_| CredentialError)?;
        let array = self
            .env
            .byte_array_from_slice(token.expose())
            .map_err(|_| CredentialError)?;
        let result = self.env.call_method(
            &self.object,
            "save",
            "(Ljava/lang/String;[B)Z",
            &[JValue::Object(&provider), JValue::Object(&array)],
        );
        // Clear a Java exception before wiping JNI arrays; no exception text is emitted.
        if result.is_err() {
            let _ = self.env.exception_clear();
        }
        self.env
            .set_byte_array_region(&array, 0, &vec![0; token.expose().len()])
            .map_err(|_| CredentialError)?;
        if result
            .map_err(|_| CredentialError)?
            .z()
            .map_err(|_| CredentialError)?
        {
            Ok(())
        } else {
            Err(CredentialError)
        }
    }
    fn delete(&mut self, p: ProviderId) -> Result<(), CredentialError> {
        let provider = self.env.new_string(p.key()).map_err(|_| CredentialError)?;
        if self
            .env
            .call_method(
                &self.object,
                "delete",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&provider)],
            )
            .map_err(|_| {
                let _ = self.env.exception_clear();
                CredentialError
            })?
            .z()
            .map_err(|_| CredentialError)?
        {
            Ok(())
        } else {
            Err(CredentialError)
        }
    }
}
fn provider(env: &mut JNIEnv<'_>, s: &JString<'_>) -> Result<ProviderId, CredentialError> {
    let s: String = env.get_string(s).map_err(|_| CredentialError)?.into();
    match s.as_str() {
        "wechat" => Ok(ProviderId::WeChat),
        "alipay" => Ok(ProviderId::Alipay),
        "bank" => Ok(ProviderId::Bank),
        _ => Err(CredentialError),
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_saveProviderCredentialNative<'a>(
    mut env: JNIEnv<'a>,
    _: JClass<'a>,
    object: JObject<'a>,
    name: JString<'a>,
    bytes: JByteArray<'a>,
) -> jboolean {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let p = provider(&mut env, &name)?;
        if env.get_array_length(&bytes).map_err(|_| CredentialError)? > 16_384 {
            return Err(CredentialError);
        }
        let token = SecretToken::new(
            env.convert_byte_array(&bytes)
                .map_err(|_| CredentialError)?,
        );
        AndroidCredentialStore {
            env: &mut env,
            object,
        }
        .save(p, &token)
    }));
    let _ = env.exception_clear();
    matches!(result, Ok(Ok(()))) as jboolean
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_loadProviderCredentialNative<'a>(
    mut env: JNIEnv<'a>,
    _: JClass<'a>,
    object: JObject<'a>,
    name: JString<'a>,
) -> jbyteArray {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let p = provider(&mut env, &name)?;
        let token = AndroidCredentialStore {
            env: &mut env,
            object,
        }
        .load(p)?;
        match token {
            Some(t) => Ok::<_, CredentialError>(
                env.byte_array_from_slice(t.expose())
                    .map_err(|_| CredentialError)?
                    .into_raw(),
            ),
            None => Ok(std::ptr::null_mut()),
        }
    }));
    let _ = env.exception_clear();
    result
        .ok()
        .and_then(Result::ok)
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_deleteProviderCredentialNative<
    'a,
>(
    mut env: JNIEnv<'a>,
    _: JClass<'a>,
    object: JObject<'a>,
    name: JString<'a>,
) -> jboolean {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let p = provider(&mut env, &name)?;
        AndroidCredentialStore {
            env: &mut env,
            object,
        }
        .delete(p)
    }));
    let _ = env.exception_clear();
    matches!(result, Ok(Ok(()))) as jboolean
}
