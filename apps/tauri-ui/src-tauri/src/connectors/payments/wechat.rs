use super::crypto::{validate_wechat_bill_date, wechat_canonical_request};
use super::http::{
    MAX_BILL_BYTES, MAX_JSON_BYTES, agent, map_http_status, map_ureq_error, read_bounded,
};
use crate::connectors::model::ProviderErrorCode;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Local, NaiveDate, Utc};
use flate2::read::MultiGzDecoder;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::{RsaPrivateKey, pkcs1v15};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::io::Read;
use zeroize::{Zeroize, Zeroizing};

const WECHAT_API: &str = "https://api.mch.weixin.qq.com";
const WECHAT_HOST: &str = "api.mch.weixin.qq.com";
const BILL_PATH: &str = "/v3/bill/tradebill";

#[derive(Clone, Copy)]
pub struct WechatCredentials<'a> {
    pub merchant_id: &'a str,
    pub certificate_serial: &'a str,
    pub private_key_pem: &'a str,
}

#[derive(Deserialize)]
struct BillEnvelope {
    #[serde(deserialize_with = "deserialize_zeroizing_string")]
    download_url: Zeroizing<String>,
    hash_type: String,
    hash_value: String,
}

pub struct BillMetadata {
    pub(super) download_url: Zeroizing<String>,
    pub(super) hash_type: String,
    pub(super) hash_value: String,
}

fn deserialize_zeroizing_string<'de, D>(deserializer: D) -> Result<Zeroizing<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Zeroizing::new)
}

pub struct WechatClient {
    agent: ureq::Agent,
    api_base: String,
}

impl Default for WechatClient {
    fn default() -> Self {
        Self::new(agent())
    }
}

impl WechatClient {
    pub fn new(agent: ureq::Agent) -> Self {
        Self {
            agent,
            api_base: WECHAT_API.to_string(),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(agent: ureq::Agent, api_base: String) -> Self {
        Self { agent, api_base }
    }

    pub fn read_bill(
        &self,
        bill_date: &str,
        credentials: WechatCredentials<'_>,
    ) -> Result<(String, u64), ProviderErrorCode> {
        let today = Local::now().date_naive();
        validate_wechat_bill_date(bill_date, today)
            .map_err(|_| ProviderErrorCode::InvalidRequest)?;
        validate_credentials(credentials)?;
        let metadata = self.request_trade_bill(bill_date, today, &credentials)?;
        if metadata.hash_type != "SHA1" {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let bytes = self.download_bill(&metadata.download_url, &credentials)?;
        let downloaded = verify_wechat_bill(&bytes, &metadata.hash_value)?;
        Ok((bill_date.to_string(), downloaded as u64))
    }

    pub(super) fn request_trade_bill(
        &self,
        bill_date: &str,
        today: NaiveDate,
        credentials: &WechatCredentials<'_>,
    ) -> Result<BillMetadata, ProviderErrorCode> {
        validate_wechat_bill_date(bill_date, today)
            .map_err(|_| ProviderErrorCode::InvalidRequest)?;
        validate_credentials(*credentials)?;
        let canonical_url =
            format!("{BILL_PATH}?bill_date={bill_date}&bill_type=ALL&tar_type=GZIP");
        let timestamp = Utc::now().timestamp().to_string();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let message = wechat_canonical_request("GET", &canonical_url, &timestamp, &nonce, "");
        let key = parse_private_key(credentials.private_key_pem)?;
        let signature = STANDARD.encode(
            pkcs1v15::SigningKey::<Sha256>::new(key)
                .sign(message.as_bytes())
                .to_bytes(),
        );
        let authorization = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",timestamp=\"{}\",serial_no=\"{}\",signature=\"{}\"",
            credentials.merchant_id, nonce, timestamp, credentials.certificate_serial, signature
        );
        let url = format!("{}{canonical_url}", self.api_base);
        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &authorization)
            .header("Accept", "application/json")
            .call()
            .map_err(map_ureq_error)?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(map_http_status(response.status().as_u16()));
        }
        let body = read_bounded(&mut response, MAX_JSON_BYTES)?;
        let envelope: BillEnvelope =
            serde_json::from_slice(&body).map_err(|_| ProviderErrorCode::InvalidResponse)?;
        validate_wechat_download_url(&envelope.download_url)?;
        Ok(BillMetadata {
            download_url: envelope.download_url,
            hash_type: envelope.hash_type,
            hash_value: envelope.hash_value,
        })
    }

    fn download_bill(
        &self,
        download_url: &str,
        credentials: &WechatCredentials<'_>,
    ) -> Result<Zeroizing<Vec<u8>>, ProviderErrorCode> {
        let url = url::Url::parse(download_url).map_err(|_| ProviderErrorCode::InvalidResponse)?;
        validate_wechat_download_url(download_url)?;
        let canonical_url = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_string(),
        };
        let timestamp = Utc::now().timestamp().to_string();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let message = wechat_canonical_request("GET", &canonical_url, &timestamp, &nonce, "");
        let key = parse_private_key(credentials.private_key_pem)?;
        let signature = STANDARD.encode(
            pkcs1v15::SigningKey::<Sha256>::new(key)
                .sign(message.as_bytes())
                .to_bytes(),
        );
        let authorization = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",timestamp=\"{}\",serial_no=\"{}\",signature=\"{}\"",
            credentials.merchant_id, nonce, timestamp, credentials.certificate_serial, signature
        );
        let mut response = self
            .agent
            .get(download_url)
            .header("Authorization", &authorization)
            .call()
            .map_err(map_ureq_error)?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(map_http_status(response.status().as_u16()));
        }
        read_bounded(&mut response, MAX_BILL_BYTES)
    }
}

fn validate_credentials(credentials: WechatCredentials<'_>) -> Result<(), ProviderErrorCode> {
    if credentials.merchant_id.is_empty()
        || credentials.merchant_id.len() > 32
        || !credentials
            .merchant_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
        || credentials.certificate_serial.is_empty()
        || credentials.certificate_serial.len() > 128
        || !credentials
            .certificate_serial
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ProviderErrorCode::InvalidCredentials);
    }
    Ok(())
}

fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, ProviderErrorCode> {
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|_| ProviderErrorCode::InvalidCredentials)
}

fn validate_wechat_download_url(value: &str) -> Result<(), ProviderErrorCode> {
    let url = url::Url::parse(value).map_err(|_| ProviderErrorCode::InvalidResponse)?;
    if url.scheme() != "https"
        || url.host_str() != Some(WECHAT_HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.port(), None | Some(443))
    {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(())
}

pub fn verify_wechat_bill(
    compressed: &[u8],
    expected_sha1: &str,
) -> Result<usize, ProviderErrorCode> {
    if expected_sha1.len() != 40 || !expected_sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    let decoder = MultiGzDecoder::new(compressed);
    let mut original = Zeroizing::new(Vec::new());
    decoder
        .take(MAX_BILL_BYTES + 1)
        .read_to_end(&mut original)
        .map_err(|_| ProviderErrorCode::InvalidResponse)?;
    if original.len() as u64 > MAX_BILL_BYTES {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    let digest = format!("{:x}", Sha1::digest(original.as_slice()));
    if !digest.eq_ignore_ascii_case(expected_sha1) {
        original.zeroize();
        return Err(ProviderErrorCode::HashMismatch);
    }
    Ok(original.len())
}
