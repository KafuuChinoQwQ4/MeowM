//! Catalogue constrained Binance (and compatibility iFinD) API requests.
//!
//! The generated catalogue supplies endpoint paths and product metadata.  This
//! module still validates those values at the transport boundary so that an
//! unsupported testnet request cannot fall back to production and credentials
//! cannot cross read/trade/withdraw scopes.

use super::{default_agent, request_error};
use crate::connectors::{
    credential_store::CredentialService,
    model::{CredentialInput, ProviderErrorCode, ProviderId, SensitiveString},
    payments::{MAX_JSON_BYTES, read_bounded},
};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Sha256;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use tokio_tungstenite::tungstenite::Message;
use ureq::Agent;

static BINANCE_CATALOGUE: OnceLock<Value> = OnceLock::new();

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ApiRequest {
    pub endpoint_id: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub credentials: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
    pub request_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ApiResponse {
    pub status: &'static str,
    pub code: Option<String>,
    pub message: Option<String>,
    pub data: Value,
    pub request_id: String,
    pub http_status: Option<u16>,
}

impl ApiResponse {
    pub(crate) fn success(request_id: String, value: Value, status: u16) -> Self {
        Self {
            status: "success",
            code: Some("0".into()),
            message: None,
            data: value,
            request_id,
            http_status: Some(status),
        }
    }

    pub(crate) fn error(request_id: String, error: ProviderErrorCode) -> Self {
        let code = error_code(error).to_string();
        Self {
            status: "failed",
            code: Some(code.clone()),
            message: Some(code),
            data: Value::Null,
            request_id,
            http_status: None,
        }
    }

    pub(crate) fn unknown(
        request_id: String,
        code: Option<String>,
        message: Option<String>,
        http_status: Option<u16>,
    ) -> Self {
        Self {
            status: "unknown",
            code,
            message,
            data: Value::Null,
            request_id,
            http_status,
        }
    }
}

pub(crate) fn binance_catalogue() -> &'static Value {
    BINANCE_CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!("binance_catalogue.json"))
            .expect("generated Binance catalogue must be valid JSON")
    })
}

/// Compatibility accessor while iFinD owns its execution module.
pub(crate) fn ifind_catalogue() -> &'static Value {
    super::ifind_api::catalogue()
}

/// Compatibility wrapper retained for callers that have not switched to the
/// focused iFinD module yet.
pub(crate) fn execute_ifind(
    request: ApiRequest,
    credentials: &CredentialService,
    tokens: &super::IfindTokenCache,
) -> ApiResponse {
    super::ifind_api::execute_ifind(request, credentials, tokens)
}

pub(crate) fn execute_binance(request: ApiRequest, credentials: &CredentialService) -> ApiResponse {
    let request_id = request.request_id.clone();
    let Some(entry) = find_entry(binance_catalogue(), &request.endpoint_id) else {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    };
    if entry.get("transport").and_then(Value::as_str) != Some("rest") {
        return ApiResponse::error(request_id, ProviderErrorCode::UnsupportedProvider);
    }
    let environment = match validate_binance_environment(entry, request.environment.as_deref()) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let method = entry.get("method").and_then(Value::as_str).unwrap_or("GET");
    let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
    let auth = entry
        .get("auth")
        .and_then(Value::as_str)
        .unwrap_or("public");
    let mutating = entry
        .get("mutating")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if mutating && !request.confirmed {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let params = match object_params(&request.params) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let scope = match credential_scope(request.credentials.as_deref()) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let selected =
        match load_binance_credentials(credentials, scope, environment, auth, mutating, path) {
            Ok(value) => value,
            Err(error) => return ApiResponse::error(request_id, error),
        };
    let (api_key, secret_key) = selected.as_ref().map_or((None, None), |value| {
        (
            Some(value.api_key.expose_secret()),
            Some(value.secret_key.expose_secret()),
        )
    });
    let client = BinanceApiClient::default();
    match client.request(
        method,
        path,
        entry.get("host").and_then(Value::as_str).unwrap_or("api"),
        auth,
        &params,
        api_key,
        secret_key,
        environment,
        mutating,
    ) {
        Ok((value, status)) => ApiResponse::success(request_id, value, status),
        Err(failure) => failure.into_response(request_id),
    }
}

/// Guard a request at both sides of the network operation. A changed ledger
/// session must never deliver stale private data; an in-flight write is kept
/// as `unknown` because Binance may have accepted it before the change.
pub(crate) fn execute_binance_guarded(
    request: ApiRequest,
    credentials: &CredentialService,
    app: &tauri::AppHandle,
    generation: u64,
) -> ApiResponse {
    let request_id = request.request_id.clone();
    if !binance_session_valid(app, generation) {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let mutating = find_entry(binance_catalogue(), &request.endpoint_id)
        .and_then(|entry| entry.get("mutating"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let response = execute_binance(request, credentials);
    if binance_session_valid(app, generation) {
        return response;
    }
    if mutating && response.status == "success" {
        ApiResponse::unknown(
            response.request_id,
            response.code,
            response.message,
            response.http_status,
        )
    } else {
        ApiResponse::error(response.request_id, ProviderErrorCode::InvalidRequest)
    }
}

fn binance_session_valid(app: &tauri::AppHandle, generation: u64) -> bool {
    let Some(state) = app.try_state::<crate::LedgerState>() else {
        return false;
    };
    let Ok(ledger) = state.0.lock() else {
        return false;
    };
    ledger.require_unlocked().is_ok() && ledger.import_generation() == generation
}

/// Kept for older IPC callers. Continuous sessions use
/// `crypto::binance_streams`; this command returns only the first response.
pub(crate) async fn execute_binance_socket(
    request: ApiRequest,
    credentials: &CredentialService,
) -> ApiResponse {
    let request_id = request.request_id.clone();
    let Some(entry) = find_entry(binance_catalogue(), &request.endpoint_id) else {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    };
    let transport = entry.get("transport").and_then(Value::as_str).unwrap_or("");
    if !matches!(transport, "stream" | "ws_api") {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let environment = match validate_binance_environment(entry, request.environment.as_deref()) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let auth = entry
        .get("auth")
        .and_then(Value::as_str)
        .unwrap_or("public");
    let mutating = entry
        .get("mutating")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if mutating && !request.confirmed {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let scope = match credential_scope(request.credentials.as_deref()) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let raw_path = entry.get("path").and_then(Value::as_str).unwrap_or("");
    let selected =
        match load_binance_credentials(credentials, scope, environment, auth, mutating, raw_path) {
            Ok(value) => value,
            Err(error) => return ApiResponse::error(request_id, error),
        };
    if transport == "stream" && auth != "public" {
        return ApiResponse::error(request_id, ProviderErrorCode::UnsupportedProvider);
    }
    let mut params = match object_params(&request.params) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let path = match substitute_path(raw_path, &mut params) {
        Ok(value) => value,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let product = entry.get("product").and_then(Value::as_str).unwrap_or("");
    let url = if transport == "stream" {
        binance_stream_url(product, environment, path.trim_start_matches('/'))
    } else {
        binance_ws_api_url(product, environment)
    };
    let (mut socket, _) = match crate::crypto::streams::connect_url_with_proxy(&url).await {
        Ok(value) => value,
        Err(_) => return ApiResponse::error(request_id, ProviderErrorCode::NetworkFailure),
    };
    let payload = if transport == "stream" {
        serde_json::json!({
            "method": "SUBSCRIBE",
            "params": [path.trim_start_matches('/')],
            "id": 1,
        })
    } else {
        if let Some(value) = selected.as_ref() {
            params.insert(
                "apiKey".into(),
                Value::String(value.api_key.expose_secret().into()),
            );
            params
                .entry("timestamp")
                .or_insert_with(|| Value::Number(serde_json::Number::from(now_millis())));
            params
                .entry("recvWindow")
                .or_insert_with(|| Value::Number(serde_json::Number::from(5000u64)));
            let mut pairs = match scalar_pairs(&params) {
                Ok(value) => value,
                Err(error) => return ApiResponse::error(request_id, error),
            };
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            let signature = match hmac_hex(
                value.secret_key.expose_secret().as_bytes(),
                encode_signature_pairs(&pairs).as_bytes(),
            ) {
                Ok(value) => value,
                Err(error) => return ApiResponse::error(request_id, error),
            };
            params.insert("signature".into(), Value::String(signature));
        }
        serde_json::json!({
            "id": request.request_id,
            "method": path.trim_start_matches('/'),
            "params": params,
        })
    };
    if socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .is_err()
    {
        return if mutating {
            ApiResponse::unknown(request_id, None, None, None)
        } else {
            ApiResponse::error(request_id, ProviderErrorCode::NetworkFailure)
        };
    }
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next()).await;
        let Some(message) = message.ok().flatten() else {
            return if mutating {
                ApiResponse::unknown(request_id, None, None, None)
            } else {
                ApiResponse::error(request_id, ProviderErrorCode::Timeout)
            };
        };
        match message {
            Ok(Message::Text(text)) => {
                let value = match serde_json::from_str::<Value>(&text) {
                    Ok(value) => value,
                    Err(_) => {
                        return if mutating {
                            ApiResponse::unknown(request_id, None, None, None)
                        } else {
                            ApiResponse::error(request_id, ProviderErrorCode::InvalidResponse)
                        };
                    }
                };
                let response = websocket_response(request_id, value);
                let _ = socket.close(None).await;
                return response;
            }
            Ok(Message::Binary(bytes)) => {
                let value = match serde_json::from_slice::<Value>(&bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        return if mutating {
                            ApiResponse::unknown(request_id, None, None, None)
                        } else {
                            ApiResponse::error(request_id, ProviderErrorCode::InvalidResponse)
                        };
                    }
                };
                let response = websocket_response(request_id, value);
                let _ = socket.close(None).await;
                return response;
            }
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) => {
                return if mutating {
                    ApiResponse::unknown(request_id, None, None, None)
                } else {
                    ApiResponse::error(request_id, ProviderErrorCode::NetworkFailure)
                };
            }
            Ok(_) => {}
            Err(_) => {
                return if mutating {
                    ApiResponse::unknown(request_id, None, None, None)
                } else {
                    ApiResponse::error(request_id, ProviderErrorCode::NetworkFailure)
                };
            }
        }
    }
}

fn websocket_response(request_id: String, value: Value) -> ApiResponse {
    let status = value.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
    if value.get("error").is_some()
        || value
            .get("code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code < 0)
        || status >= 400
    {
        let code = value.get("code").map(value_code);
        let message = value
            .get("msg")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        return ApiResponse {
            status: "failed",
            code,
            message,
            data: value,
            request_id,
            http_status: Some(status),
        };
    }
    ApiResponse::success(request_id, value, 101)
}

pub(crate) struct BinanceCredentials {
    pub(crate) api_key: SensitiveString,
    pub(crate) secret_key: SensitiveString,
}

/// Load exactly one Binance credential group.  The read group is never used
/// for writes; demo/live groups are selected from the explicit environment.
pub(crate) fn load_binance_credentials(
    credentials: &CredentialService,
    scope: &str,
    environment: &str,
    auth: &str,
    mutating: bool,
    path: &str,
) -> Result<Option<BinanceCredentials>, ProviderErrorCode> {
    if auth == "public" {
        if scope != "read" {
            return Err(ProviderErrorCode::InvalidCredentials);
        }
        return Ok(None);
    }
    if !matches!(
        auth,
        "api_key" | "signed" | "trade" | "user_data" | "user_stream"
    ) {
        return Err(ProviderErrorCode::UnsupportedProvider);
    }
    if mutating && scope == "read" {
        return Err(ProviderErrorCode::InvalidCredentials);
    }
    if scope == "withdraw" && !path.to_ascii_lowercase().contains("withdraw") {
        return Err(ProviderErrorCode::PermissionDenied);
    }
    let provider = match scope {
        "read" => ProviderId::Binance,
        "trade" if environment == "spot_testnet" => ProviderId::BinanceTradeDemo,
        "trade" => ProviderId::BinanceTradeLive,
        "withdraw" => ProviderId::BinanceWithdraw,
        _ => return Err(ProviderErrorCode::InvalidRequest),
    };
    let value = credentials
        .load(provider)
        .map_err(|_| ProviderErrorCode::SecureStorageUnavailable)?
        .ok_or(ProviderErrorCode::InvalidCredentials)?;
    match value {
        CredentialInput::Binance {
            api_key,
            secret_key,
        }
        | CredentialInput::BinanceTradeDemo {
            api_key,
            secret_key,
        }
        | CredentialInput::BinanceTradeLive {
            api_key,
            secret_key,
        }
        | CredentialInput::BinanceWithdraw {
            api_key,
            secret_key,
        } => Ok(Some(BinanceCredentials {
            api_key,
            secret_key,
        })),
        _ => Err(ProviderErrorCode::InvalidCredentials),
    }
}

fn credential_scope(scope: Option<&str>) -> Result<&str, ProviderErrorCode> {
    let scope = scope.unwrap_or("read");
    if matches!(scope, "read" | "trade" | "withdraw") {
        Ok(scope)
    } else {
        Err(ProviderErrorCode::InvalidRequest)
    }
}

pub(crate) fn validate_binance_environment<'a>(
    entry: &Value,
    requested: Option<&'a str>,
) -> Result<&'a str, ProviderErrorCode> {
    let environment = requested.unwrap_or("production");
    if !matches!(environment, "production" | "spot_testnet") {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    let allowed = entry
        .get("environments")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .any(|item| item == environment)
        })
        .unwrap_or(environment == "production");
    if allowed {
        Ok(environment)
    } else {
        Err(ProviderErrorCode::UnsupportedProvider)
    }
}

fn find_entry<'a>(catalogue: &'a Value, endpoint_id: &str) -> Option<&'a Value> {
    catalogue
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(endpoint_id))
}

pub(crate) fn object_params(value: &Value) -> Result<Map<String, Value>, ProviderErrorCode> {
    let Some(map) = value.as_object() else {
        return Err(ProviderErrorCode::InvalidRequest);
    };
    if map.len() > 128 {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(map.clone())
}

pub(crate) fn substitute_path(
    path: &str,
    params: &mut Map<String, Value>,
) -> Result<String, ProviderErrorCode> {
    let mut output = path.to_owned();
    let mut names = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(ProviderErrorCode::InvalidRequest);
        };
        names.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    for name in names {
        let Some(value) = params.remove(&name) else {
            return Err(ProviderErrorCode::InvalidRequest);
        };
        output = output.replace(&format!("{{{name}}}"), &scalar(&value)?);
    }
    if output.contains('{') || output.contains('}') {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(output)
}

pub(crate) fn scalar(value: &Value) -> Result<String, ProviderErrorCode> {
    match value {
        Value::String(value) if value.len() <= 4096 => Ok(value.clone()),
        Value::String(_) => Err(ProviderErrorCode::InvalidRequest),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value)
            .map_err(|_| ProviderErrorCode::InvalidRequest)
            .and_then(|value| {
                if value.len() <= 4096 {
                    Ok(value)
                } else {
                    Err(ProviderErrorCode::InvalidRequest)
                }
            }),
        Value::Null => Ok(String::new()),
    }
}

fn scalar_pairs(params: &Map<String, Value>) -> Result<Vec<(String, String)>, ProviderErrorCode> {
    params
        .iter()
        .map(|(key, value)| Ok((key.clone(), scalar(value)?)))
        .collect()
}

pub(crate) fn encode_pairs(pairs: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    // Binance signs the percent-encoded query string. `url` uses `+` for a
    // space in form encoding; use the RFC percent form so the bytes signed are
    // exactly the bytes sent on the wire.
    serializer.finish().replace('+', "%20")
}

pub(crate) fn encode_signature_pairs(pairs: &[(String, String)]) -> String {
    encode_pairs(pairs)
}

pub(crate) fn hmac_hex(key: &[u8], message: &[u8]) -> Result<String, ProviderErrorCode> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| ProviderErrorCode::InvalidCredentials)?;
    mac.update(message);
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

pub(crate) fn binance_rest_base(
    host: &str,
    environment: &str,
    path: &str,
    auth: &str,
) -> Result<&'static str, ProviderErrorCode> {
    if environment == "spot_testnet" && !(host == "api" && path.starts_with("/api/")) {
        return Err(ProviderErrorCode::UnsupportedProvider);
    }
    match host {
        "alpha" => Ok("https://www.binance.com"),
        "papi" => Ok("https://papi.binance.com"),
        "fapi" => Ok("https://fapi.binance.com"),
        "dapi" => Ok("https://dapi.binance.com"),
        "eapi" => Ok("https://eapi.binance.com"),
        "api" if environment == "spot_testnet" && path.starts_with("/api/") => {
            Ok("https://testnet.binance.vision")
        }
        "api" if path.starts_with("/api/") && auth == "public" => {
            Ok("https://data-api.binance.vision")
        }
        "api" => Ok("https://api.binance.com"),
        _ => Err(ProviderErrorCode::UnsupportedProvider),
    }
}

pub(crate) fn binance_stream_url(product: &str, environment: &str, stream: &str) -> String {
    let product = product.to_ascii_lowercase();
    let stream = stream.trim_start_matches('/');
    if product.contains("alpha") {
        return format!("wss://nbstream.binance.com/w3w/wsa/stream/ws/{stream}");
    }
    if product.contains("option") {
        return format!("wss://nbstream.binance.com/eoptions/ws/{stream}");
    }
    if product.contains("coin") {
        return format!("wss://dstream.binance.com/ws/{stream}");
    }
    if product.contains("usd") || product.contains("futures") {
        return format!("wss://fstream.binance.com/market/ws/{stream}");
    }
    if environment == "spot_testnet" {
        format!("wss://data-stream.binance.vision/ws/{stream}")
    } else {
        format!("wss://stream.binance.com:9443/ws/{stream}")
    }
}

pub(crate) fn binance_ws_api_url(product: &str, environment: &str) -> String {
    let product = product.to_ascii_lowercase();
    if product.contains("coin") {
        return "wss://ws-dapi.binance.com/ws-dapi/v1".into();
    }
    if product.contains("usd") || product.contains("futures") {
        return if environment == "spot_testnet" {
            "wss://testnet.binancefuture.com/ws-fapi/v1".into()
        } else {
            "wss://ws-fapi.binance.com/ws-fapi/v1".into()
        };
    }
    if environment == "spot_testnet" {
        "wss://ws-api.testnet.binance.vision/ws-api/v3".into()
    } else {
        "wss://ws-api.binance.com:443/ws-api/v3".into()
    }
}

pub(crate) fn requires_signature(auth: &str) -> bool {
    matches!(auth, "signed" | "trade" | "user_data")
}

struct BinanceApiClient {
    agent: Agent,
    base_override: Option<String>,
}

impl Default for BinanceApiClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_override: None,
        }
    }
}

impl BinanceApiClient {
    #[cfg(test)]
    fn for_test(agent: Agent, base: String) -> Self {
        Self {
            agent,
            base_override: Some(base),
        }
    }

    fn request(
        &self,
        method: &str,
        raw_path: &str,
        host: &str,
        auth: &str,
        params: &Map<String, Value>,
        api_key: Option<&str>,
        secret_key: Option<&str>,
        environment: &str,
        mutating: bool,
    ) -> Result<(Value, u16), BinanceFailure> {
        let mut params = params.clone();
        let path = substitute_path(raw_path, &mut params)
            .map_err(|error| BinanceFailure::provider(error, None, None, None, mutating))?;
        let mut pairs = scalar_pairs(&params)
            .map_err(|error| BinanceFailure::provider(error, None, None, None, mutating))?;
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        let signed = requires_signature(auth);
        if signed {
            if !pairs.iter().any(|(key, _)| key == "timestamp") {
                pairs.push(("timestamp".into(), now_millis().to_string()));
            }
            if !pairs.iter().any(|(key, _)| key == "recvWindow") {
                pairs.push(("recvWindow".into(), "5000".into()));
            }
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
        }
        let encoded = encode_pairs(&pairs);
        let signature = if signed {
            let secret = secret_key.ok_or_else(|| {
                BinanceFailure::provider(
                    ProviderErrorCode::InvalidCredentials,
                    None,
                    None,
                    None,
                    false,
                )
            })?;
            Some(
                hmac_hex(secret.as_bytes(), encode_signature_pairs(&pairs).as_bytes())
                    .map_err(|error| BinanceFailure::provider(error, None, None, None, mutating))?,
            )
        } else {
            None
        };
        let query = signature
            .as_ref()
            .map(|value| {
                if encoded.is_empty() {
                    format!("signature={value}")
                } else {
                    format!("{encoded}&signature={value}")
                }
            })
            .unwrap_or(encoded);
        let base = self.base_override.as_deref().map(str::to_owned).unwrap_or(
            binance_rest_base(host, environment, &path, auth)
                .map_err(|error| BinanceFailure::provider(error, None, None, None, mutating))?
                .to_owned(),
        );
        let url = if query.is_empty() {
            format!("{}{path}", base.trim_end_matches('/'))
        } else {
            format!("{}{path}?{query}", base.trim_end_matches('/'))
        };
        let mut response = match method {
            "GET" => {
                let request = self.agent.get(&url);
                match api_key {
                    Some(value) => request.header("X-MBX-APIKEY", value).call(),
                    None => request.call(),
                }
            }
            "DELETE" => {
                let request = self.agent.delete(&url);
                match api_key {
                    Some(value) => request.header("X-MBX-APIKEY", value).call(),
                    None => request.call(),
                }
            }
            "PUT" => {
                let request = self
                    .agent
                    .put(&url)
                    .header("Content-Type", "application/x-www-form-urlencoded");
                match api_key {
                    Some(value) => request.header("X-MBX-APIKEY", value).send_empty(),
                    None => request.send_empty(),
                }
            }
            _ => {
                let request = self
                    .agent
                    .post(&url)
                    .header("Content-Type", "application/x-www-form-urlencoded");
                match api_key {
                    Some(value) => request.header("X-MBX-APIKEY", value).send_empty(),
                    None => request.send_empty(),
                }
            }
        }
        .map_err(|error| BinanceFailure::transport(request_error(error), mutating))?;
        let status = response.status().as_u16();
        // Full spot metadata and tickers exceed the payment/account response bound.
        // Keep the larger bounded allowance exclusive to these public GETs.
        let response_limit = if method == "GET"
            && auth == "public"
            && matches!(raw_path, "/api/v3/exchangeInfo" | "/api/v3/ticker/24hr")
        {
            16 * 1024 * 1024
        } else {
            MAX_JSON_BYTES
        };
        let bytes = read_bounded(&mut response, response_limit)
            .map_err(|error| BinanceFailure::transport(error, mutating))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            BinanceFailure::provider(
                ProviderErrorCode::InvalidResponse,
                None,
                None,
                Some(status),
                mutating,
            )
        })?;
        if let Some(code) = value
            .get("code")
            .and_then(Value::as_i64)
            .filter(|code| *code < 0)
        {
            return Err(BinanceFailure::from_value(
                super::binance::map_binance_code(code),
                &value,
                Some(status),
                mutating,
            ));
        }
        if !(200..300).contains(&status) {
            return Err(BinanceFailure::from_value(
                super::binance::map_binance_code(
                    value
                        .get("code")
                        .and_then(Value::as_i64)
                        .unwrap_or(status as i64),
                ),
                &value,
                Some(status),
                mutating,
            ));
        }
        Ok((value, status))
    }
}

struct BinanceFailure {
    error: ProviderErrorCode,
    code: Option<String>,
    message: Option<String>,
    status: Option<u16>,
    ambiguous: bool,
    data: Value,
}

impl BinanceFailure {
    fn provider(
        error: ProviderErrorCode,
        code: Option<String>,
        message: Option<String>,
        status: Option<u16>,
        mutating: bool,
    ) -> Self {
        Self {
            error,
            code,
            message,
            status,
            ambiguous: mutating && status.is_some_and(|value| value >= 500),
            data: Value::Null,
        }
    }

    fn transport(error: ProviderErrorCode, mutating: bool) -> Self {
        Self {
            error,
            code: None,
            message: None,
            status: None,
            ambiguous: mutating,
            data: Value::Null,
        }
    }

    fn from_value(
        error: ProviderErrorCode,
        value: &Value,
        status: Option<u16>,
        mutating: bool,
    ) -> Self {
        Self {
            error,
            code: value.get("code").map(value_code),
            message: value
                .get("msg")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            status,
            ambiguous: mutating && status.is_some_and(|value| value >= 500),
            data: value.clone(),
        }
    }

    fn into_response(self, request_id: String) -> ApiResponse {
        if self.ambiguous {
            return ApiResponse::unknown(request_id, self.code, self.message, self.status);
        }
        ApiResponse {
            status: "failed",
            code: self.code.or_else(|| Some(error_code(self.error).into())),
            message: self.message.or_else(|| Some(error_code(self.error).into())),
            data: self.data,
            request_id,
            http_status: self.status,
        }
    }
}

fn value_code(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn error_code(error: ProviderErrorCode) -> &'static str {
    match error {
        ProviderErrorCode::InvalidCredentials => "invalid_credentials",
        ProviderErrorCode::PermissionDenied => "permission_denied",
        ProviderErrorCode::BillUnavailable => "bill_unavailable",
        ProviderErrorCode::DataUnavailable => "data_unavailable",
        ProviderErrorCode::RateLimited => "rate_limited",
        ProviderErrorCode::Timeout => "timeout",
        ProviderErrorCode::NetworkFailure => "network_failure",
        ProviderErrorCode::InvalidResponse => "invalid_response",
        ProviderErrorCode::HashMismatch => "hash_mismatch",
        ProviderErrorCode::SecureStorageUnavailable => "secure_storage_unavailable",
        ProviderErrorCode::UnsupportedProvider => "unsupported_provider",
        ProviderErrorCode::InvalidRequest => "invalid_request",
        ProviderErrorCode::ProviderFailure => "provider_failure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn bulk_spot_market_data_exceeds_payment_limit_but_private_responses_remain_bounded() {
        for (path, auth, accepted) in [
            ("/api/v3/exchangeInfo", "public", true),
            ("/api/v3/ticker/24hr", "public", true),
            ("/api/v3/account", "signed", false),
            ("/api/v3/exchangeInfo", "signed", false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 4096];
                stream.read(&mut request).unwrap();
                let body = format!(
                    "{{\"payload\":\"{}\"}}",
                    "x".repeat(MAX_JSON_BYTES as usize + 1024)
                );
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            });
            let client = BinanceApiClient::for_test(default_agent(), endpoint);
            let result = client.request(
                "GET",
                path,
                "api",
                auth,
                &Map::new(),
                Some("key"),
                Some("secret"),
                "production",
                false,
            );
            assert_eq!(result.is_ok(), accepted, "response bound for {path} {auth}");
            server.join().unwrap();
        }
    }

    #[test]
    fn request_defaults_to_read_scope_and_rejects_unknown_scope() {
        assert_eq!(credential_scope(None).unwrap(), "read");
        assert_eq!(credential_scope(Some("trade")).unwrap(), "trade");
        assert!(credential_scope(Some("secret")).is_err());
    }

    #[test]
    fn unsupported_product_testnet_never_falls_back_to_production() {
        assert_eq!(
            binance_rest_base("api", "spot_testnet", "/api/v3/time", "public").unwrap(),
            "https://testnet.binance.vision"
        );
        assert!(binance_rest_base("papi", "spot_testnet", "/papi/v1/account", "signed").is_err());
        assert_eq!(
            binance_rest_base("papi", "production", "/papi/v1/account", "signed").unwrap(),
            "https://papi.binance.com"
        );
        assert!(
            binance_stream_url("USDⓈ-M Futures", "production", "btcusdt@markPrice")
                .contains("/market/ws/")
        );
    }

    #[test]
    fn signed_payload_is_deterministic_and_percent_encoded() {
        let pairs = vec![("symbol".into(), "A B".into()), ("x".into(), "a&b".into())];
        assert_eq!(encode_pairs(&pairs), "symbol=A%20B&x=a%26b");
        assert_eq!(encode_signature_pairs(&pairs), "symbol=A%20B&x=a%26b");
    }

    #[test]
    fn catalogue_has_environment_and_mutating_metadata() {
        let entries = binance_catalogue().as_array().unwrap();
        assert!(entries.iter().all(|entry| {
            entry
                .get("environments")
                .and_then(Value::as_array)
                .is_some()
        }));
        assert!(entries.iter().any(|entry| {
            entry.get("path").and_then(Value::as_str) == Some("/orderList.place.oco")
                && entry.get("mutating") == Some(&Value::Bool(true))
        }));
    }

    #[test]
    fn local_http_preserves_binance_error_and_marks_sent_write_unknown() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            let body = r#"{"code":-1000,"msg":"internal error"}"#;
            write!(
                stream,
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            stream.flush().unwrap();
        });
        let client = BinanceApiClient::for_test(default_agent(), endpoint);
        let failure = client
            .request(
                "POST",
                "/api/v3/order",
                "api",
                "trade",
                &Map::new(),
                Some("key"),
                Some("secret"),
                "production",
                true,
            )
            .unwrap_err();
        let response = failure.into_response("request-1".into());
        assert_eq!(response.status, "unknown");
        assert_eq!(response.code.as_deref(), Some("-1000"));
        assert_eq!(response.message.as_deref(), Some("internal error"));
        assert_eq!(response.http_status, Some(500));
        server.join().unwrap();
    }
}
