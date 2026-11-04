use super::binance::decimal_is_nonzero;
use super::{default_agent, map_exchange_http_status, parse_json_response, request_error};
use crate::connectors::model::{AssetBalance, ProviderErrorCode};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ureq::Agent;

mod account;
mod history;
mod market;
pub(crate) mod model;
mod positions;
#[cfg(test)]
mod tests;
use self::model::OkxEnvironment;

const API_BASE: &str = "https://openapi.okx.com";
const BALANCE_PATH: &str = "/api/v5/account/balance";
const TRADE_POST_PATHS: &[&str] = &["/api/v5/trade/order", "/api/v5/trade/cancel-order"];

pub(crate) struct OkxCredentials<'a> {
    pub api_key: &'a str,
    pub secret_key: &'a str,
    pub passphrase: &'a str,
}

pub(crate) struct OkxClient {
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
    fn post<T, B>(
        &self,
        path: &'static str,
        body: &B,
        credentials: &OkxCredentials<'_>,
        environment: OkxEnvironment,
    ) -> Result<T, ProviderErrorCode>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        if !TRADE_POST_PATHS.contains(&path) {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let body = serde_json::to_vec(body).map_err(|_| ProviderErrorCode::InvalidRequest)?;
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut prehash = format!("{timestamp}POST{path}").into_bytes();
        prehash.extend_from_slice(&body);
        let signature = hmac_base64(credentials.secret_key.as_bytes(), &prehash)?;
        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let mut request = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header("OK-ACCESS-KEY", credentials.api_key)
            .header("OK-ACCESS-SIGN", &signature)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", credentials.passphrase);
        if environment == OkxEnvironment::Demo {
            request = request.header("x-simulated-trading", "1");
        }
        let mut response = request.send(body.as_slice()).map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        parse_okx_envelope(value)
    }

    fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
        credentials: Option<&OkxCredentials<'_>>,
    ) -> Result<T, ProviderErrorCode> {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
            .finish();
        let request_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        };
        let url = format!("{}{request_path}", self.base_url.trim_end_matches('/'));
        let mut request = self.agent.get(&url);
        if let Some(credentials) = credentials {
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            let signature = hmac_base64(
                credentials.secret_key.as_bytes(),
                format!("{timestamp}GET{request_path}").as_bytes(),
            )?;
            request = request
                .header("OK-ACCESS-KEY", credentials.api_key)
                .header("OK-ACCESS-SIGN", &signature)
                .header("OK-ACCESS-TIMESTAMP", &timestamp)
                .header("OK-ACCESS-PASSPHRASE", credentials.passphrase);
        }
        let mut response = request.call().map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        parse_okx_envelope(value)
    }
    #[cfg(test)]
    pub(super) fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    pub(super) fn read_balances(
        &self,
        credentials: &OkxCredentials<'_>,
    ) -> Result<Vec<AssetBalance>, ProviderErrorCode> {
        let accounts: Vec<OkxAccountBalance> = self.get(BALANCE_PATH, &[], Some(credentials))?;

        let mut balances = Vec::new();
        for account in accounts {
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

fn parse_okx_envelope<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, ProviderErrorCode> {
    let code = value
        .get("code")
        .and_then(serde_json::Value::as_str)
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    if code != "0" {
        return Err(map_okx_code(code));
    }
    serde_json::from_value(
        value
            .get("data")
            .cloned()
            .ok_or(ProviderErrorCode::InvalidResponse)?,
    )
    .map_err(|_| ProviderErrorCode::InvalidResponse)
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
