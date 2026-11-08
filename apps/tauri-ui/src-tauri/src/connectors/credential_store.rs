use super::model::{
    ConnectionStatus, CredentialField, CredentialInput, ProviderId, SensitiveString,
};
use keyring_core::{Entry, Error as KeyringError};
use std::collections::HashMap;
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

const SERVICE_NAME: &str = "com.example.localledger.providers";

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeInitializeCredentialContext(
    env: jni::JNIEnv<'_>,
    class: jni::objects::JClass<'_>,
    context: jni::objects::JObject<'_>,
) {
    android_native_keyring_store::Java_io_crates_keyring_Keyring_00024Companion_initializeNdkContext(
        env,
        class.into(),
        context,
    );
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_nativeSetHttpProxy(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    host: jni::objects::JString<'_>,
    port: jni::sys::jint,
) {
    let host: Option<String> = if host.is_null() {
        None
    } else {
        env.get_string(&host).ok().map(Into::into)
    };
    crate::connectors::payments::initialize_android_http_proxy(host.as_deref(), port);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialStoreError {
    Unavailable,
}

pub trait CredentialStore: Send + Sync {
    fn contains(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<bool, CredentialStoreError>;

    fn get(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<Option<Zeroizing<String>>, CredentialStoreError>;

    fn set(
        &self,
        provider: ProviderId,
        field: CredentialField,
        value: &str,
    ) -> Result<(), CredentialStoreError>;

    fn delete(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<(), CredentialStoreError>;
}

pub struct NativeCredentialStore {
    available: bool,
}

impl NativeCredentialStore {
    pub fn initialize() -> Self {
        Self {
            available: install_native_store().is_ok(),
        }
    }

    fn entry(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<Entry, CredentialStoreError> {
        if !self.available {
            return Err(CredentialStoreError::Unavailable);
        }
        let account = format!("{}.{}", provider.as_str(), field.as_str());
        Entry::new(SERVICE_NAME, &account).map_err(|_| CredentialStoreError::Unavailable)
    }
}

fn install_native_store() -> Result<(), ()> {
    #[cfg(target_os = "android")]
    {
        let store = android_native_keyring_store::Store::new().map_err(|_| ())?;
        keyring_core::set_default_store(store);
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let store = windows_native_keyring_store::Store::new().map_err(|_| ())?;
        keyring_core::set_default_store(store);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let store = apple_native_keyring_store::keychain::Store::new().map_err(|_| ())?;
        keyring_core::set_default_store(store);
        return Ok(());
    }
    #[cfg(target_os = "ios")]
    {
        let store = apple_native_keyring_store::protected::Store::new().map_err(|_| ())?;
        keyring_core::set_default_store(store);
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let store = zbus_secret_service_keyring_store::Store::new().map_err(|_| ())?;
        keyring_core::set_default_store(store);
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(())
}

impl CredentialStore for NativeCredentialStore {
    fn contains(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<bool, CredentialStoreError> {
        self.get(provider, field).map(|value| value.is_some())
    }

    fn get(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<Option<Zeroizing<String>>, CredentialStoreError> {
        let entry = self.entry(provider, field)?;
        let mut bytes = match entry.get_secret() {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(KeyringError::NoEntry) => return Ok(None),
            Err(_) => return Err(CredentialStoreError::Unavailable),
        };
        let value =
            String::from_utf8(bytes.to_vec()).map_err(|_| CredentialStoreError::Unavailable)?;
        bytes.zeroize();
        Ok(Some(Zeroizing::new(value)))
    }

    fn set(
        &self,
        provider: ProviderId,
        field: CredentialField,
        value: &str,
    ) -> Result<(), CredentialStoreError> {
        let entry = self.entry(provider, field)?;
        let bytes = Zeroizing::new(value.as_bytes().to_vec());
        entry
            .set_secret(bytes.as_slice())
            .map_err(|_| CredentialStoreError::Unavailable)
    }

    fn delete(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<(), CredentialStoreError> {
        let entry = self.entry(provider, field)?;
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(CredentialStoreError::Unavailable),
        }
    }
}

pub struct CredentialService {
    store: Arc<dyn CredentialStore>,
}

impl CredentialService {
    pub fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self { store }
    }

    pub fn statuses(&self) -> Result<Vec<ConnectionStatus>, CredentialStoreError> {
        [
            ProviderId::WechatPay,
            ProviderId::Alipay,
            ProviderId::Binance,
            ProviderId::BinanceTradeDemo,
            ProviderId::BinanceTradeLive,
            ProviderId::BinanceWithdraw,
            ProviderId::Okx,
            ProviderId::OkxTradeDemo,
            ProviderId::OkxTradeLive,
            ProviderId::OkxWithdraw,
            ProviderId::Ifind,
        ]
        .into_iter()
        .map(|provider| self.status(provider))
        .collect()
    }

    pub fn status(&self, provider: ProviderId) -> Result<ConnectionStatus, CredentialStoreError> {
        let configured =
            required_fields(provider)
                .iter()
                .try_fold(true, |all_present, field| {
                    self.store
                        .contains(provider, *field)
                        .map(|present| all_present && present)
                })?;
        Ok(ConnectionStatus {
            provider,
            configured,
        })
    }

    pub fn save(&self, input: CredentialInput) -> Result<ConnectionStatus, CredentialStoreError> {
        let provider = input.provider();
        let fields = input.into_fields();
        let mut previous = Vec::with_capacity(fields.len());
        for (field, _) in &fields {
            previous.push((*field, self.store.get(provider, *field)?));
        }

        for (field, value) in &fields {
            if self
                .store
                .set(provider, *field, value.expose_secret())
                .is_err()
            {
                for (previous_field, previous_value) in previous.iter().rev() {
                    match previous_value {
                        Some(value) => {
                            let _ = self.store.set(provider, *previous_field, value.as_str());
                        }
                        None => {
                            let _ = self.store.delete(provider, *previous_field);
                        }
                    }
                }
                return Err(CredentialStoreError::Unavailable);
            }
        }

        self.status(provider)
    }

    pub fn load(
        &self,
        provider: ProviderId,
    ) -> Result<Option<CredentialInput>, CredentialStoreError> {
        let mut fields = HashMap::new();
        for field in required_fields(provider) {
            let Some(value) = self.store.get(provider, *field)? else {
                return Ok(None);
            };
            let sensitive = SensitiveString::from_zeroizing(value)
                .map_err(|_| CredentialStoreError::Unavailable)?;
            fields.insert(*field, sensitive);
        }

        let take = |fields: &mut HashMap<CredentialField, SensitiveString>,
                    field: CredentialField|
         -> Result<SensitiveString, CredentialStoreError> {
            fields
                .remove(&field)
                .ok_or(CredentialStoreError::Unavailable)
        };

        let input = match provider {
            ProviderId::WechatPay => CredentialInput::WechatPay {
                merchant_id: take(&mut fields, CredentialField::MerchantId)?,
                certificate_serial: take(&mut fields, CredentialField::CertificateSerial)?,
                private_key_pem: take(&mut fields, CredentialField::PrivateKeyPem)?,
            },
            ProviderId::Alipay => CredentialInput::Alipay {
                app_id: take(&mut fields, CredentialField::AppId)?,
                app_private_key_pem: take(&mut fields, CredentialField::AppPrivateKeyPem)?,
                alipay_public_key_pem: take(&mut fields, CredentialField::AlipayPublicKeyPem)?,
            },
            ProviderId::Binance => CredentialInput::Binance {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
            },
            ProviderId::BinanceTradeDemo => CredentialInput::BinanceTradeDemo {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
            },
            ProviderId::BinanceTradeLive => CredentialInput::BinanceTradeLive {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
            },
            ProviderId::BinanceWithdraw => CredentialInput::BinanceWithdraw {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
            },
            ProviderId::Okx => CredentialInput::Okx {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
                passphrase: take(&mut fields, CredentialField::Passphrase)?,
            },
            ProviderId::OkxTradeDemo => CredentialInput::OkxTradeDemo {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
                passphrase: take(&mut fields, CredentialField::Passphrase)?,
            },
            ProviderId::OkxTradeLive => CredentialInput::OkxTradeLive {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
                passphrase: take(&mut fields, CredentialField::Passphrase)?,
            },
            ProviderId::OkxWithdraw => CredentialInput::OkxWithdraw {
                api_key: take(&mut fields, CredentialField::ApiKey)?,
                secret_key: take(&mut fields, CredentialField::SecretKey)?,
                passphrase: take(&mut fields, CredentialField::Passphrase)?,
            },
            ProviderId::Ifind => CredentialInput::Ifind {
                refresh_token: take(&mut fields, CredentialField::RefreshToken)?,
            },
        };
        Ok(Some(input))
    }

    pub fn remove(&self, provider: ProviderId) -> Result<ConnectionStatus, CredentialStoreError> {
        let fields = required_fields(provider);
        let mut previous = Vec::with_capacity(fields.len());
        for field in fields {
            previous.push((*field, self.store.get(provider, *field)?));
        }

        for field in fields {
            if self.store.delete(provider, *field).is_err() {
                for (previous_field, previous_value) in previous.iter().rev() {
                    match previous_value {
                        Some(value) => {
                            let _ = self.store.set(provider, *previous_field, value.as_str());
                        }
                        None => {
                            let _ = self.store.delete(provider, *previous_field);
                        }
                    }
                }
                return Err(CredentialStoreError::Unavailable);
            }
        }
        Ok(ConnectionStatus {
            provider,
            configured: false,
        })
    }
}

fn required_fields(provider: ProviderId) -> &'static [CredentialField] {
    use CredentialField::*;
    match provider {
        ProviderId::WechatPay => &[MerchantId, CertificateSerial, PrivateKeyPem],
        ProviderId::Alipay => &[AppId, AppPrivateKeyPem, AlipayPublicKeyPem],
        ProviderId::Binance => &[ApiKey, SecretKey],
        ProviderId::BinanceTradeDemo
        | ProviderId::BinanceTradeLive
        | ProviderId::BinanceWithdraw => &[ApiKey, SecretKey],
        ProviderId::Okx => &[ApiKey, SecretKey, Passphrase],
        ProviderId::OkxTradeDemo | ProviderId::OkxTradeLive | ProviderId::OkxWithdraw => {
            &[ApiKey, SecretKey, Passphrase]
        }
        ProviderId::Ifind => &[RefreshToken],
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MemoryCredentialStore {
    state: std::sync::Mutex<MemoryCredentialState>,
}

#[cfg(test)]
#[derive(Default)]
struct MemoryCredentialState {
    values: HashMap<(ProviderId, CredentialField), Zeroizing<String>>,
    successful_writes_before_failure: Option<usize>,
    successful_deletes_before_failure: Option<usize>,
}

#[cfg(test)]
impl MemoryCredentialStore {
    pub fn fail_next_write_after(&self, successful_writes: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.successful_writes_before_failure = Some(successful_writes);
        }
    }

    pub fn fail_next_delete_after(&self, successful_deletes: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.successful_deletes_before_failure = Some(successful_deletes);
        }
    }
}

#[cfg(test)]
impl CredentialStore for MemoryCredentialStore {
    fn contains(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<bool, CredentialStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        Ok(state.values.contains_key(&(provider, field)))
    }

    fn get(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<Option<Zeroizing<String>>, CredentialStoreError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        Ok(state
            .values
            .get(&(provider, field))
            .map(|value| Zeroizing::new(value.to_string())))
    }

    fn set(
        &self,
        provider: ProviderId,
        field: CredentialField,
        value: &str,
    ) -> Result<(), CredentialStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        if let Some(remaining) = state.successful_writes_before_failure.as_mut() {
            if *remaining == 0 {
                state.successful_writes_before_failure = None;
                return Err(CredentialStoreError::Unavailable);
            }
            *remaining -= 1;
        }
        state
            .values
            .insert((provider, field), Zeroizing::new(value.to_owned()));
        Ok(())
    }

    fn delete(
        &self,
        provider: ProviderId,
        field: CredentialField,
    ) -> Result<(), CredentialStoreError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        if let Some(remaining) = state.successful_deletes_before_failure.as_mut() {
            if *remaining == 0 {
                state.successful_deletes_before_failure = None;
                return Err(CredentialStoreError::Unavailable);
            }
            *remaining -= 1;
        }
        state.values.remove(&(provider, field));
        Ok(())
    }
}

#[cfg(test)]
#[path = "credential_store_tests.rs"]
mod tests;
