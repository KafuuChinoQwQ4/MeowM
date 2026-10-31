use serde::{Deserialize, Deserializer, Serialize, de};
use std::fmt;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    WechatPay,
    Alipay,
    Binance,
    Okx,
    Ifind,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WechatPay => "wechat_pay",
            Self::Alipay => "alipay",
            Self::Binance => "binance",
            Self::Okx => "okx",
            Self::Ifind => "ifind",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum CredentialInput {
    WechatPay {
        merchant_id: SensitiveString,
        certificate_serial: SensitiveString,
        private_key_pem: SensitiveString,
    },
    Alipay {
        app_id: SensitiveString,
        app_private_key_pem: SensitiveString,
        alipay_public_key_pem: SensitiveString,
    },
    Binance {
        api_key: SensitiveString,
        secret_key: SensitiveString,
    },
    Okx {
        api_key: SensitiveString,
        secret_key: SensitiveString,
        passphrase: SensitiveString,
    },
    Ifind {
        refresh_token: SensitiveString,
    },
}

#[derive(Eq, PartialEq)]
pub struct SensitiveString(Zeroizing<String>);

impl SensitiveString {
    pub fn new(value: String) -> Result<Self, &'static str> {
        Self::from_zeroizing(Zeroizing::new(value))
    }

    pub fn from_zeroizing(value: Zeroizing<String>) -> Result<Self, &'static str> {
        if value.trim().is_empty() {
            return Err("credential_empty");
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveString([REDACTED])")
    }
}

impl<'de> Deserialize<'de> for SensitiveString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CredentialField {
    MerchantId,
    CertificateSerial,
    PrivateKeyPem,
    AppId,
    AppPrivateKeyPem,
    AlipayPublicKeyPem,
    ApiKey,
    SecretKey,
    Passphrase,
    RefreshToken,
}

impl CredentialField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MerchantId => "merchant_id",
            Self::CertificateSerial => "certificate_serial",
            Self::PrivateKeyPem => "private_key_pem",
            Self::AppId => "app_id",
            Self::AppPrivateKeyPem => "app_private_key_pem",
            Self::AlipayPublicKeyPem => "alipay_public_key_pem",
            Self::ApiKey => "api_key",
            Self::SecretKey => "secret_key",
            Self::Passphrase => "passphrase",
            Self::RefreshToken => "refresh_token",
        }
    }
}

impl CredentialInput {
    pub fn provider(&self) -> ProviderId {
        match self {
            Self::WechatPay { .. } => ProviderId::WechatPay,
            Self::Alipay { .. } => ProviderId::Alipay,
            Self::Binance { .. } => ProviderId::Binance,
            Self::Okx { .. } => ProviderId::Okx,
            Self::Ifind { .. } => ProviderId::Ifind,
        }
    }

    pub fn into_fields(self) -> Vec<(CredentialField, SensitiveString)> {
        match self {
            Self::WechatPay {
                merchant_id,
                certificate_serial,
                private_key_pem,
            } => vec![
                (CredentialField::MerchantId, merchant_id),
                (CredentialField::CertificateSerial, certificate_serial),
                (CredentialField::PrivateKeyPem, private_key_pem),
            ],
            Self::Alipay {
                app_id,
                app_private_key_pem,
                alipay_public_key_pem,
            } => vec![
                (CredentialField::AppId, app_id),
                (CredentialField::AppPrivateKeyPem, app_private_key_pem),
                (CredentialField::AlipayPublicKeyPem, alipay_public_key_pem),
            ],
            Self::Binance {
                api_key,
                secret_key,
            } => vec![
                (CredentialField::ApiKey, api_key),
                (CredentialField::SecretKey, secret_key),
            ],
            Self::Okx {
                api_key,
                secret_key,
                passphrase,
            } => vec![
                (CredentialField::ApiKey, api_key),
                (CredentialField::SecretKey, secret_key),
                (CredentialField::Passphrase, passphrase),
            ],
            Self::Ifind { refresh_token } => {
                vec![(CredentialField::RefreshToken, refresh_token)]
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionTestOutcome {
    Connected,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorCode {
    InvalidCredentials,
    PermissionDenied,
    BillUnavailable,
    DataUnavailable,
    RateLimited,
    Timeout,
    NetworkFailure,
    InvalidResponse,
    HashMismatch,
    SecureStorageUnavailable,
    UnsupportedProvider,
    InvalidRequest,
    ProviderFailure,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConnectionTestSummary {
    BillRead {
        bill_date: String,
        downloaded_bytes: u64,
        hash_verified: Option<bool>,
    },
    BinanceBalances {
        balances: Vec<AssetBalance>,
    },
    OkxBalances {
        balances: Vec<AssetBalance>,
    },
    IfindQuote {
        quote: IfindQuote,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AssetBalance {
    pub asset: String,
    pub free: String,
    pub locked: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IfindQuote {
    pub symbol: String,
    pub quote_time: Option<String>,
    pub last: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConnectionTestRequest {
    pub provider: ProviderId,
    pub bill_date: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConnectionTestResult {
    pub provider: ProviderId,
    pub outcome: ConnectionTestOutcome,
    pub summary: Option<ConnectionTestSummary>,
    pub error_code: Option<ProviderErrorCode>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConnectionStatus {
    pub provider: ProviderId,
    pub configured: bool,
}
