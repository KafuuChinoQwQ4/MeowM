use super::{default_agent, map_exchange_http_status, parse_json_response, request_error};
use crate::connectors::model::{AssetBalance, ProviderErrorCode};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use ureq::Agent;

const API_BASE: &str = "https://api.binance.com";
const ACCOUNT_PATH: &str = "/api/v3/account";
const RECEIVE_WINDOW: &str = "5000";

pub(super) struct BinanceCredentials<'a> {
    pub api_key: &'a str,
    pub secret_key: &'a str,
}

pub(super) struct BinanceClient {
    agent: Agent,
    base_url: String,
}

impl Default for BinanceClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_url: API_BASE.to_string(),
        }
    }
}

impl BinanceClient {
    #[cfg(test)]
    pub(super) fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    pub(super) fn read_balances(
        &self,
        credentials: &BinanceCredentials<'_>,
    ) -> Result<Vec<AssetBalance>, ProviderErrorCode> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ProviderErrorCode::InvalidRequest)?
            .as_millis();
        let payload = format!("recvWindow={RECEIVE_WINDOW}&timestamp={timestamp}");
        let signature = hmac_hex(credentials.secret_key.as_bytes(), payload.as_bytes())?;
        let url = format!(
            "{}{ACCOUNT_PATH}?{payload}&signature={signature}",
            self.base_url.trim_end_matches('/')
        );
        let mut response = self
            .agent
            .get(&url)
            .header("X-MBX-APIKEY", credentials.api_key)
            .call()
            .map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        let envelope: BinanceAccountResponse =
            serde_json::from_value(value).map_err(|_| ProviderErrorCode::InvalidResponse)?;
        if let Some(code) = envelope.code {
            return Err(map_binance_code(code));
        }
        let balances = envelope
            .balances
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        let mut output = Vec::new();
        for balance in balances {
            if balance.is_nonzero()? {
                output.push(AssetBalance {
                    asset: balance.asset,
                    free: balance.free,
                    locked: balance.locked,
                });
                if output.len() > 1000 {
                    return Err(ProviderErrorCode::InvalidResponse);
                }
            }
        }
        Ok(output)
    }
}

fn hmac_hex(key: &[u8], message: &[u8]) -> Result<String, ProviderErrorCode> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| ProviderErrorCode::InvalidCredentials)?;
    mac.update(message);
    let mut output = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").map_err(|_| ProviderErrorCode::ProviderFailure)?;
    }
    Ok(output)
}

#[derive(Deserialize)]
struct BinanceAccountResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    balances: Option<Vec<BinanceBalance>>,
}

#[derive(Deserialize)]
struct BinanceBalance {
    asset: String,
    free: String,
    locked: String,
}

impl BinanceBalance {
    fn is_nonzero(&self) -> Result<bool, ProviderErrorCode> {
        validate_asset(&self.asset)?;
        let free = decimal_is_nonzero(&self.free)?;
        let locked = decimal_is_nonzero(&self.locked)?;
        Ok(free || locked)
    }
}

fn validate_asset(asset: &str) -> Result<(), ProviderErrorCode> {
    if asset.is_empty()
        || asset.len() > 32
        || !asset.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(())
}

pub(super) fn decimal_is_nonzero(value: &str) -> Result<bool, ProviderErrorCode> {
    if value.is_empty() || value.len() > 128 {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    let mut decimal_points = 0;
    let mut digits = 0;
    let mut nonzero = false;
    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' => {
                digits += 1;
                nonzero |= byte != b'0';
            }
            b'.' => {
                decimal_points += 1;
                if decimal_points > 1 {
                    return Err(ProviderErrorCode::InvalidResponse);
                }
            }
            _ => return Err(ProviderErrorCode::InvalidResponse),
        }
    }
    if digits == 0 {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(nonzero)
}

pub(super) fn map_binance_code(code: i64) -> ProviderErrorCode {
    match code {
        -1003 => ProviderErrorCode::RateLimited,
        -1021 => ProviderErrorCode::InvalidRequest,
        -1022 | -2014 | -2015 => ProviderErrorCode::InvalidCredentials,
        _ => ProviderErrorCode::ProviderFailure,
    }
}
