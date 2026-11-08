//! Catalogue-constrained iFinD HTTP API execution.
//!
//! Authentication is deliberately kept in this module and the token cache.  An
//! IPC caller selects a generated endpoint id and supplies only that endpoint's
//! JSON parameters; it cannot choose a host or send a refresh/access token.

use super::ifind::{IfindClient, IfindTokenCache, IfindTokenError, map_ifind_error};
use super::{default_agent, map_ifind_http_status, request_error};
use crate::connectors::credential_store::CredentialService;
use crate::connectors::markets::api::{ApiRequest, ApiResponse};
use crate::connectors::model::{CredentialInput, ProviderErrorCode, ProviderId};
use crate::connectors::payments::{MAX_JSON_BYTES, read_bounded};
use serde_json::{Map, Value, json};
use std::sync::OnceLock;
use ureq::Agent;
use zeroize::Zeroizing;

const API_BASE: &str = "https://quantapi.51ifind.com";

static IFIND_CATALOGUE: OnceLock<Value> = OnceLock::new();

pub(crate) fn catalogue() -> &'static Value {
    IFIND_CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!("ifind_catalogue.json"))
            .expect("generated iFinD catalogue must be valid JSON")
    })
}

/// Execute one generated iFinD REST operation.
///
/// Token operations return only an operation status.  The access token and
/// refresh token remain in the native process and are never put in ApiResponse.
pub(crate) fn execute_ifind(
    request: ApiRequest,
    credentials: &CredentialService,
    tokens: &IfindTokenCache,
) -> ApiResponse {
    let request_id = request.request_id.clone();
    let Some(entry) = find_entry(catalogue(), &request.endpoint_id) else {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    };
    if entry.get("transport").and_then(Value::as_str) != Some("rest")
        || entry.get("method").and_then(Value::as_str) != Some("POST")
    {
        return ApiResponse::error(request_id, ProviderErrorCode::UnsupportedProvider);
    }
    if let Some(environment) = request.environment.as_deref()
        && environment != "production"
    {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let mutating = entry
        .get("mutating")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if mutating && !request.confirmed {
        return ApiResponse::error(request_id, ProviderErrorCode::InvalidRequest);
    }
    let params = match object_params(&request.params) {
        Ok(params) => params,
        Err(error) => return ApiResponse::error(request_id, error),
    };
    let refresh_token = match credentials.load(ProviderId::Ifind) {
        Ok(Some(CredentialInput::Ifind { refresh_token })) => refresh_token,
        Ok(_) => return ApiResponse::error(request_id, ProviderErrorCode::InvalidCredentials),
        Err(_) => {
            return ApiResponse::error(request_id, ProviderErrorCode::SecureStorageUnavailable);
        }
    };

    let endpoint = entry.get("path").and_then(Value::as_str).unwrap_or("");
    let token_client = IfindClient::default();
    let token_result = match endpoint.rsplit('/').next().unwrap_or_default() {
        "get_access_token" => {
            tokens.access_token_detailed(&token_client, refresh_token.expose_secret())
        }
        "update_access_token" => {
            match tokens.update_access_token_detailed(&token_client, refresh_token.expose_secret())
            {
                Ok(()) => {
                    return ApiResponse::success(
                        request_id,
                        json!({"operation": "update_access_token", "status": "rotated"}),
                        200,
                    );
                }
                Err(error) => {
                    return token_failure_response(
                        request_id,
                        error,
                        refresh_token.expose_secret(),
                    );
                }
            }
        }
        _ => tokens.access_token_detailed(&token_client, refresh_token.expose_secret()),
    };
    if endpoint.ends_with("/get_access_token") {
        return match token_result {
            Ok(_) => ApiResponse::success(
                request_id,
                json!({"operation": "get_access_token", "status": "ready"}),
                200,
            ),
            Err(error) => token_failure_response(request_id, error, refresh_token.expose_secret()),
        };
    }
    let token = match token_result {
        Ok(token) => token,
        Err(error) => {
            return token_failure_response(request_id, error, refresh_token.expose_secret());
        }
    };
    let mut access_secrets = vec![Zeroizing::new(token.as_str().to_string())];
    let client = IfindApiClient::default();
    let first = client.request(endpoint, &params, token.as_str());
    let should_refresh = first
        .as_ref()
        .err()
        .is_some_and(|failure| failure.error == ProviderErrorCode::InvalidCredentials);
    let result = if should_refresh && !mutating {
        tokens.invalidate();
        match tokens.access_token_detailed(&token_client, refresh_token.expose_secret()) {
            Ok(refreshed) => {
                access_secrets.push(Zeroizing::new(refreshed.as_str().to_string()));
                client.request(endpoint, &params, refreshed.as_str())
            }
            Err(error) => {
                return token_failure_response(request_id, error, refresh_token.expose_secret());
            }
        }
    } else {
        first
    };
    match result {
        Ok((value, status)) => ApiResponse::success(request_id, value, status),
        Err(failure) => {
            let mut secrets = vec![refresh_token.expose_secret()];
            secrets.extend(access_secrets.iter().map(|secret| secret.as_str()));
            failure.into_response(request_id, &secrets)
        }
    }
}

fn token_failure_response(
    request_id: String,
    failure: IfindTokenError,
    refresh_token: &str,
) -> ApiResponse {
    let message = failure
        .message
        .map(|message| sanitize_message(&message, &[refresh_token]));
    ApiResponse {
        status: "failed",
        code: failure
            .provider_code
            .or_else(|| Some(provider_error_name(failure.error).to_string())),
        message,
        data: Value::Null,
        request_id,
        http_status: failure.http_status,
    }
}

fn find_entry<'a>(catalogue: &'a Value, endpoint_id: &str) -> Option<&'a Value> {
    catalogue
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(endpoint_id))
}

fn object_params(value: &Value) -> Result<Map<String, Value>, ProviderErrorCode> {
    let Some(map) = value.as_object() else {
        return Err(ProviderErrorCode::InvalidRequest);
    };
    if map.len() > 128 {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(map.clone())
}

struct IfindApiClient {
    agent: Agent,
    base_url: String,
}

impl Default for IfindApiClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_url: API_BASE.to_string(),
        }
    }
}

impl IfindApiClient {
    #[cfg(test)]
    fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    fn request(
        &self,
        path: &str,
        params: &Map<String, Value>,
        access_token: &str,
    ) -> Result<(Value, u16), IfindFailure> {
        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let mut response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header("access_token", access_token)
            .header("ifindlang", "cn")
            .send_json(Value::Object(params.clone()))
            .map_err(|error| IfindFailure::transport(request_error(error)))?;
        let status = response.status().as_u16();
        let response_limit = if path == "/api/v1/smart_stock_picking" {
            8 * 1024 * 1024
        } else {
            MAX_JSON_BYTES
        };
        let bytes = read_bounded(&mut response, response_limit).map_err(IfindFailure::transport)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| IfindFailure::with_status(ProviderErrorCode::InvalidResponse, status))?;
        if let Some(errorcode) = value.get("errorcode").and_then(Value::as_i64)
            && errorcode != 0
        {
            return Err(IfindFailure::from_value(errorcode, &value, status));
        }
        if !(200..300).contains(&status) {
            return Err(IfindFailure::from_http(status, &value));
        }
        Ok((value, status))
    }
}

#[derive(Debug)]
struct IfindFailure {
    error: ProviderErrorCode,
    provider_code: Option<String>,
    message: Option<String>,
    http_status: Option<u16>,
}

impl IfindFailure {
    fn transport(error: ProviderErrorCode) -> Self {
        Self {
            error,
            provider_code: None,
            message: None,
            http_status: None,
        }
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
        let error = map_ifind_error(errorcode);
        let message = value
            .get("errmsg")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Self {
            error,
            provider_code: Some(errorcode.to_string()),
            message,
            http_status: Some(status),
        }
    }

    fn from_http(status: u16, value: &Value) -> Self {
        let message = value
            .get("errmsg")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Self {
            error: map_ifind_http_status(status),
            provider_code: value
                .get("errorcode")
                .and_then(Value::as_i64)
                .map(|code| code.to_string())
                .or_else(|| Some(status.to_string())),
            message,
            http_status: Some(status),
        }
    }

    fn into_response(self, request_id: String, secrets: &[&str]) -> ApiResponse {
        let message = self
            .message
            .map(|message| sanitize_message(&message, secrets));
        ApiResponse {
            status: "failed",
            code: self
                .provider_code
                .or_else(|| Some(provider_error_name(self.error).to_string())),
            message,
            data: Value::Null,
            request_id,
            http_status: self.http_status,
        }
    }
}

fn sanitize_message(message: &str, secrets: &[&str]) -> String {
    let mut clean = message.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            clean = clean.replace(secret, "[REDACTED_SECRET]");
        }
    }
    clean.chars().take(1024).collect()
}

fn provider_error_name(error: ProviderErrorCode) -> &'static str {
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
    use crate::connectors::payments::agent;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn full_stock_search_supports_large_tables_without_relaxing_other_response_bounds() {
        for (path, accepted) in [
            ("/api/v1/smart_stock_picking", true),
            ("/api/v1/basic_data_service", false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                let body = format!(
                    "{{\"errorcode\":0,\"tables\":[{{\"payload\":\"{}\"}}]}}",
                    "x".repeat(MAX_JSON_BYTES as usize + 1024)
                );
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            });
            let client = IfindApiClient::for_test(agent(), endpoint);
            assert_eq!(
                client.request(path, &Map::new(), "test-token").is_ok(),
                accepted,
                "response bound for {path}",
            );
            server.join().unwrap();
        }
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes);
                let length = headers
                    .split("\r\n")
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then_some(value.trim())
                    })
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                let header_end = bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                if bytes.len() >= header_end + length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn respond(stream: &mut TcpStream, status: &str, body: &str) {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn catalogue_includes_pdf_helpers_and_marks_mutating_operations() {
        let entries = catalogue().as_array().unwrap();
        for path in [
            "/api/v1/portfolio_manage",
            "/api/v1/final_fund_valuation",
            "/api/v1/get_data_volume",
            "/api/v1/get_error_message",
            "/api/v1/get_thscode",
            "/api/v1/update_access_token",
        ] {
            let entry = entries
                .iter()
                .find(|entry| entry.get("path") == Some(&Value::String(path.into())))
                .unwrap();
            assert_eq!(entry.get("method").and_then(Value::as_str), Some("POST"));
            assert_eq!(
                entry
                    .get("environments")
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(1)
            );
        }
        let portfolio = entries
            .iter()
            .find(|entry| {
                entry.get("path") == Some(&Value::String("/api/v1/portfolio_manage".into()))
            })
            .unwrap();
        assert_eq!(
            portfolio.get("mutating").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn local_server_preserves_nested_success_and_official_error_without_secret() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut success, _) = listener.accept().unwrap();
            let request = read_request(&mut success);
            assert_eq!(
                request.lines().next().unwrap(),
                "POST /api/v1/basic_data_service HTTP/1.1"
            );
            assert!(request.contains("access_token: access-secret"));
            assert!(request.contains("nested"));
            respond(
                &mut success,
                "200 OK",
                r#"{"errorcode":0,"tables":[{"table":{"nested":[{"value":"x}"}]}}]}"#,
            );

            let (mut failure, _) = listener.accept().unwrap();
            let request = read_request(&mut failure);
            assert!(request.contains("access_token: access-secret"));
            respond(
                &mut failure,
                "200 OK",
                r#"{"errorcode":-1303,"errmsg":"denied access-secret"}"#,
            );
        });
        let client = IfindApiClient::for_test(agent(), endpoint);
        let params =
            serde_json::from_value::<Map<String, Value>>(json!({"nested":{"key":"value"}}))
                .unwrap();
        let success = client
            .request("/api/v1/basic_data_service", &params, "access-secret")
            .unwrap();
        assert_eq!(success.0["tables"][0]["table"]["nested"][0]["value"], "x}");
        let error = client
            .request("/api/v1/basic_data_service", &params, "access-secret")
            .unwrap_err();
        let response = error.into_response("request-1".into(), &["access-secret"]);
        assert_eq!(response.code.as_deref(), Some("-1303"));
        assert_eq!(response.http_status, Some(200));
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("access-secret"));
        assert!(serialized.contains("REDACTED_SECRET"));
        server.join().unwrap();
    }

    #[test]
    fn token_cache_update_rotates_without_returning_the_new_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert_eq!(
                request.lines().next().unwrap(),
                "POST /api/v1/update_access_token HTTP/1.1"
            );
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("refresh_token: refresh-secret")
            );
            respond(
                &mut stream,
                "200 OK",
                r#"{"data":{"access_token":"new-access-secret"}}"#,
            );
        });
        let client = IfindClient::for_test(agent(), endpoint);
        let cache = IfindTokenCache::default();
        cache
            .update_access_token(&client, "refresh-secret")
            .unwrap();
        let cached = cache.access_token(&client, "refresh-secret").unwrap();
        assert_eq!(cached.as_str(), "new-access-secret");
        server.join().unwrap();
    }

    #[test]
    fn token_error_keeps_official_code_message_and_http_status_without_secret() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert_eq!(
                request.lines().next().unwrap(),
                "POST /api/v1/get_access_token HTTP/1.1"
            );
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("refresh_token: refresh-secret")
            );
            respond(
                &mut stream,
                "200 OK",
                r#"{"errorcode":-1301,"errmsg":"invalid refresh-secret"}"#,
            );
        });
        let client = IfindClient::for_test(agent(), endpoint);
        let cache = IfindTokenCache::default();
        let failure = cache
            .access_token_detailed(&client, "refresh-secret")
            .unwrap_err();
        let response = token_failure_response("request-1".into(), failure, "refresh-secret");
        assert_eq!(response.code.as_deref(), Some("-1301"));
        assert_eq!(response.http_status, Some(200));
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains("REDACTED_SECRET"));
        assert!(!serialized.contains("refresh-secret"));
        server.join().unwrap();
    }
}
