use super::{default_agent, map_ifind_http_status, request_error};
use crate::connectors::model::{IfindQuote, ProviderErrorCode};
use crate::connectors::payments::read_bounded;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use ureq::Agent;
use zeroize::Zeroizing;

const API_BASE: &str = "https://quantapi.51ifind.com";
const TOKEN_PATH: &str = "/api/v1/get_access_token";
const UPDATE_TOKEN_PATH: &str = "/api/v1/update_access_token";
const QUOTE_PATH: &str = "/api/v1/real_time_quotation";
const ACCESS_TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60 - 5 * 60);

pub struct IfindTokenCache {
    token: Mutex<Option<CachedToken>>,
}

impl Default for IfindTokenCache {
    fn default() -> Self {
        Self {
            token: Mutex::new(None),
        }
    }
}

impl IfindTokenCache {
    pub fn invalidate(&self) {
        if let Ok(mut token) = self.token.lock() {
            *token = None;
        }
    }

    fn invalidate_value(&self, value: &str) {
        if let Ok(mut token) = self.token.lock()
            && token
                .as_ref()
                .is_some_and(|cached| cached.access_token.as_str() == value)
        {
            *token = None;
        }
    }

    fn get_or_fetch(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<Zeroizing<String>, ProviderErrorCode> {
        self.get_or_fetch_detailed(client, refresh_token)
            .map_err(|error| error.error)
    }

    fn get_or_fetch_detailed(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<Zeroizing<String>, IfindTokenError> {
        let mut cache = self
            .token
            .lock()
            .map_err(|_| IfindTokenError::provider_failure())?;
        if let Some(cached) = cache.as_ref()
            && cached.expires_at > Instant::now()
        {
            return Ok(Zeroizing::new(cached.access_token.as_str().to_string()));
        }

        let access_token = client.fetch_token_detailed(refresh_token, TOKEN_PATH)?;
        let copy = Zeroizing::new(access_token.as_str().to_string());
        *cache = Some(CachedToken {
            access_token,
            expires_at: Instant::now() + ACCESS_TOKEN_TTL,
        });
        Ok(copy)
    }

    pub(super) fn access_token(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<Zeroizing<String>, ProviderErrorCode> {
        self.get_or_fetch(client, refresh_token)
    }

    pub(super) fn access_token_detailed(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<Zeroizing<String>, IfindTokenError> {
        self.get_or_fetch_detailed(client, refresh_token)
    }

    /// Explicitly rotates the access token through the official update endpoint.
    /// The returned secret remains inside the native process and is never sent to IPC.
    pub(super) fn update_access_token(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<(), ProviderErrorCode> {
        self.update_access_token_detailed(client, refresh_token)
            .map_err(|error| error.error)
    }

    pub(super) fn update_access_token_detailed(
        &self,
        client: &IfindClient,
        refresh_token: &str,
    ) -> Result<(), IfindTokenError> {
        let access_token = client.fetch_token_detailed(refresh_token, UPDATE_TOKEN_PATH)?;
        let mut cache = self
            .token
            .lock()
            .map_err(|_| IfindTokenError::provider_failure())?;
        *cache = Some(CachedToken {
            access_token,
            expires_at: Instant::now() + ACCESS_TOKEN_TTL,
        });
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct IfindTokenError {
    pub(super) error: ProviderErrorCode,
    pub(super) provider_code: Option<String>,
    pub(super) message: Option<String>,
    pub(super) http_status: Option<u16>,
}

impl IfindTokenError {
    fn provider_failure() -> Self {
        Self {
            error: ProviderErrorCode::ProviderFailure,
            provider_code: None,
            message: None,
            http_status: None,
        }
    }

    fn provider_code(error: ProviderErrorCode) -> Self {
        Self {
            error,
            provider_code: None,
            message: None,
            http_status: None,
        }
    }

    fn transport(error: ProviderErrorCode) -> Self {
        Self::provider_code(error)
    }

    fn with_status(error: ProviderErrorCode, status: u16) -> Self {
        Self {
            error,
            provider_code: None,
            message: None,
            http_status: Some(status),
        }
    }

    fn from_value(errorcode: i64, value: &Value, status: u16) -> Self {
        Self {
            error: map_ifind_error(errorcode),
            provider_code: Some(errorcode.to_string()),
            message: value
                .get("errmsg")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string),
            http_status: Some(status),
        }
    }

    fn from_http(status: u16, value: &Value) -> Self {
        Self {
            error: map_ifind_http_status(status),
            provider_code: value
                .get("errorcode")
                .and_then(Value::as_i64)
                .map(|code| code.to_string())
                .or_else(|| Some(status.to_string())),
            message: value
                .get("errmsg")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string),
            http_status: Some(status),
        }
    }
}

struct CachedToken {
    access_token: Zeroizing<String>,
    expires_at: Instant,
}

pub(super) struct IfindClient {
    agent: Agent,
    base_url: String,
}

impl Default for IfindClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_url: API_BASE.to_string(),
        }
    }
}

impl IfindClient {
    #[cfg(test)]
    pub(super) fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    pub(super) fn read_quote(
        &self,
        refresh_token: &str,
        symbol: &str,
        cache: &IfindTokenCache,
    ) -> Result<IfindQuote, ProviderErrorCode> {
        validate_ifind_symbol(symbol)?;
        let token = cache.get_or_fetch(self, refresh_token)?;
        match self.request_quote(token.as_str(), symbol) {
            Err(ProviderErrorCode::InvalidCredentials) => {
                cache.invalidate_value(token.as_str());
                let refreshed = cache.get_or_fetch(self, refresh_token)?;
                self.request_quote(refreshed.as_str(), symbol)
            }
            result => result,
        }
    }

    fn fetch_token_detailed(
        &self,
        refresh_token: &str,
        path: &str,
    ) -> Result<Zeroizing<String>, IfindTokenError> {
        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let mut response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header("refresh_token", refresh_token)
            .send_empty()
            .map_err(|error| IfindTokenError::transport(request_error(error)))?;
        let status = response.status().as_u16();
        let bytes = read_bounded(&mut response, super::MAX_JSON_BYTES)
            .map_err(IfindTokenError::provider_code)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            IfindTokenError::with_status(ProviderErrorCode::InvalidResponse, status)
        })?;
        if let Some(error_code) = value.get("errorcode").and_then(Value::as_i64)
            && error_code != 0
        {
            return Err(IfindTokenError::from_value(error_code, &value, status));
        }
        if !(200..300).contains(&status) {
            return Err(IfindTokenError::from_http(status, &value));
        }
        let response: IfindTokenResponse = serde_json::from_value(value).map_err(|_| {
            IfindTokenError::with_status(ProviderErrorCode::InvalidResponse, status)
        })?;
        response
            .data
            .map(|data| Zeroizing::new(data.access_token.expose_secret().to_string()))
            .ok_or_else(|| {
                IfindTokenError::with_status(ProviderErrorCode::InvalidCredentials, status)
            })
    }

    fn request_quote(
        &self,
        access_token: &str,
        symbol: &str,
    ) -> Result<IfindQuote, ProviderErrorCode> {
        let url = format!("{}{QUOTE_PATH}", self.base_url.trim_end_matches('/'));
        let payload = json!({"codes": symbol, "indicators": "latest"});
        let mut response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header("access_token", access_token)
            .header("ifindlang", "cn")
            .send_json(payload)
            .map_err(request_error)?;
        let value = super::parse_json_response(&mut response, map_ifind_http_status)?;
        let error_code = value
            .get("errorcode")
            .and_then(Value::as_i64)
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        if error_code != 0 {
            return Err(map_ifind_error(error_code));
        }
        parse_quote(&value, symbol)
    }
}

pub(super) fn validate_ifind_symbol(symbol: &str) -> Result<(), ProviderErrorCode> {
    let bytes = symbol.as_bytes();
    let exchange = symbol.get(7..).unwrap_or_default();
    if bytes.len() != 9
        || !bytes[..6].iter().all(u8::is_ascii_digit)
        || bytes[6] != b'.'
        || !matches!(exchange, "SZ" | "SH" | "BJ")
    {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(())
}

pub(super) fn map_ifind_error(code: i64) -> ProviderErrorCode {
    match code {
        -1010 | -1300 | -1302 => ProviderErrorCode::InvalidCredentials,
        -1301 => ProviderErrorCode::InvalidCredentials,
        -1303 => ProviderErrorCode::PermissionDenied,
        -1305 => ProviderErrorCode::RateLimited,
        -1002 => ProviderErrorCode::Timeout,
        -4001 => ProviderErrorCode::DataUnavailable,
        -4299..=-4200 => ProviderErrorCode::InvalidRequest,
        _ => ProviderErrorCode::ProviderFailure,
    }
}

fn parse_quote(value: &Value, requested_symbol: &str) -> Result<IfindQuote, ProviderErrorCode> {
    let tables = value
        .get("tables")
        .and_then(Value::as_array)
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    let table = tables
        .iter()
        .find(|table| {
            table
                .get("thscode")
                .and_then(Value::as_str)
                .is_some_and(|symbol| symbol.eq_ignore_ascii_case(requested_symbol))
        })
        .ok_or(ProviderErrorCode::DataUnavailable)?;
    let latest = table
        .get("table")
        .and_then(|table| table.get("latest"))
        .and_then(first_value)
        .and_then(value_as_scalar_string)
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    let quote_time = table
        .get("time")
        .and_then(first_value)
        .and_then(value_as_scalar_string);
    Ok(IfindQuote {
        symbol: requested_symbol.to_string(),
        quote_time,
        last: latest,
    })
}

fn first_value(value: &Value) -> Option<&Value> {
    match value {
        Value::Array(values) => values.first(),
        value => Some(value),
    }
}

fn value_as_scalar_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_number().map(ToString::to_string))
}

#[derive(Deserialize)]
struct IfindTokenResponse {
    #[serde(default)]
    data: Option<IfindTokenData>,
}

#[derive(Deserialize)]
struct IfindTokenData {
    access_token: crate::connectors::model::SensitiveString,
}
