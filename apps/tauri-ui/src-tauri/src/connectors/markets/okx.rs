use super::binance::decimal_is_nonzero;
use super::{default_agent, map_exchange_http_status, parse_json_response, request_error};
use crate::connectors::model::{AssetBalance, ProviderErrorCode};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use ureq::Agent;

const API_BASE: &str = "https://openapi.okx.com";
const BALANCE_PATH: &str = "/api/v5/account/balance";

pub(super) struct OkxCredentials<'a> {
    pub api_key: &'a str,
    pub secret_key: &'a str,
    pub passphrase: &'a str,
}

pub(super) struct OkxClient {
    agent: Agent,
    base_url: String,
}

impl Default for OkxClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_url: API_BASE.to_string(),
        }
    }
}

impl OkxClient {
    #[cfg(test)]
    pub(super) fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    pub(super) fn read_balances(
        &self,
        credentials: &OkxCredentials<'_>,
    ) -> Result<Vec<AssetBalance>, ProviderErrorCode> {
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let prehash = format!("{timestamp}GET{BALANCE_PATH}");
        let signature = hmac_base64(credentials.secret_key.as_bytes(), prehash.as_bytes())?;
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), BALANCE_PATH);
        let mut response = self
            .agent
            .get(&url)
            .header("OK-ACCESS-KEY", credentials.api_key)
            .header("OK-ACCESS-SIGN", &signature)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", credentials.passphrase)
            .call()
            .map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        let envelope: OkxBalanceResponse =
            serde_json::from_value(value).map_err(|_| ProviderErrorCode::InvalidResponse)?;
        if envelope.code != "0" {
            return Err(map_okx_code(&envelope.code));
        }

        let mut balances = Vec::new();
        for account in envelope.data {
            for detail in account.details {
                validate_currency(&detail.currency)?;
                let nonzero =
                    decimal_is_nonzero(&detail.available)? || decimal_is_nonzero(&detail.frozen)?;
                if nonzero {
                    balances.push(AssetBalance {
                        asset: detail.currency,
                        free: detail.available,
                        locked: detail.frozen,
                    });
                    if balances.len() > 1000 {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                }
            }
        }
        Ok(balances)
    }
}

fn hmac_base64(key: &[u8], message: &[u8]) -> Result<String, ProviderErrorCode> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| ProviderErrorCode::InvalidCredentials)?;
    mac.update(message);
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

fn validate_currency(currency: &str) -> Result<(), ProviderErrorCode> {
    if currency.is_empty()
        || currency.len() > 32
        || !currency.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(())
}

pub(super) fn map_okx_code(code: &str) -> ProviderErrorCode {
    match code {
        "50011" | "50040" => ProviderErrorCode::RateLimited,
        "50102" => ProviderErrorCode::InvalidRequest,
        "50100" | "50101" | "50111" | "50113" | "50119" | "60009" => {
            ProviderErrorCode::InvalidCredentials
        }
        _ => ProviderErrorCode::ProviderFailure,
    }
}

#[derive(Deserialize)]
struct OkxBalanceResponse {
    code: String,
    data: Vec<OkxAccountBalance>,
}

#[derive(Deserialize)]
struct OkxAccountBalance {
    details: Vec<OkxBalanceDetail>,
}

#[derive(Deserialize)]
struct OkxBalanceDetail {
    #[serde(rename = "ccy")]
    currency: String,
    #[serde(rename = "availBal")]
    available: String,
    #[serde(rename = "frozenBal")]
    frozen: String,
}
