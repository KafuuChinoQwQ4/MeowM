//! Catalogue-constrained OKX REST requests. Credentials never cross the IPC boundary.
use super::{API_BASE, OkxCredentials, hmac_base64};
use crate::connectors::{
    credential_store::CredentialService,
    model::{CredentialInput, ProviderId},
    payments::{MAX_JSON_BYTES, agent, read_bounded},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};
use ureq::Agent;
use zeroize::Zeroizing;

const MAX_REQUEST_BYTES: usize = 512 * 1024;
const MAX_RESPONSE_CODE: usize = 128;
const MAX_RESPONSE_MESSAGE: usize = 4096;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApiRequest {
    pub endpoint_id: String,
    pub environment: ApiEnvironment,
    pub credentials: ApiCredential,
    pub params: Value,
    pub confirmed: bool,
    pub request_id: String,
    #[serde(default)]
    pub exp_time: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiEnvironment {
    Demo,
    Production,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiCredential {
    Read,
    Trade,
    Withdraw,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiResponse {
    pub status: &'static str,
    pub code: Option<String>,
    pub message: Option<String>,
    pub data: Value,
    pub request_id: String,
    pub http_status: Option<u16>,
}

impl ApiResponse {
    fn new(status: &'static str, request_id: String, message: &'static str) -> Self {
        Self {
            status,
            code: None,
            message: Some(message.into()),
            data: Value::Null,
            request_id,
            http_status: None,
        }
    }

    pub(crate) fn session_changed(request_id: String, mutating_and_sent: bool) -> Self {
        if mutating_and_sent {
            Self::new("unknown", request_id, "session_changed_reconcile_request")
        } else {
            Self::new("rejected", request_id, "session_changed")
        }
    }
}

pub(crate) struct Execution {
    pub response: ApiResponse,
    pub sent: bool,
    pub mutating: bool,
}

impl Execution {
    fn before_send(request_id: String, message: &'static str) -> Self {
        Self {
            response: ApiResponse::new("rejected", request_id, message),
            sent: false,
            mutating: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Parameter {
    name: String,
    #[serde(rename = "type")]
    parameter_type: String,
    required: bool,
    conditional: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RestEndpoint {
    id: String,
    method: String,
    path: String,
    auth: String,
    mutating: bool,
    body_kind: String,
    parameters: Vec<Parameter>,
}

pub(crate) fn catalogue() -> &'static Value {
    static CATALOGUE: OnceLock<Value> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!("catalogue.json"))
            .expect("bundled OKX catalogue must be valid JSON")
    })
}

pub(crate) struct AdvancedClient {
    agent: Agent,
    base_url: String,
    #[cfg(test)]
    test_catalogue: Option<Value>,
}

impl Default for AdvancedClient {
    fn default() -> Self {
        Self {
            agent: agent(),
            base_url: API_BASE.to_string(),
            #[cfg(test)]
            test_catalogue: None,
        }
    }
}

impl AdvancedClient {
    #[cfg(test)]
    pub(super) fn for_test(base_url: String, catalogue: Value) -> Self {
        Self {
            agent: agent(),
            base_url,
            test_catalogue: Some(catalogue),
        }
    }

    fn endpoint(&self, id: &str) -> Option<RestEndpoint> {
        #[cfg(test)]
        let catalogue = self.test_catalogue.as_ref().unwrap_or_else(|| catalogue());
        #[cfg(not(test))]
        let catalogue = catalogue();
        let entries = catalogue.get("rest")?.as_array()?;
        let mut matches = entries
            .iter()
            .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(id));
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        let endpoint: RestEndpoint = serde_json::from_value(entry.clone()).ok()?;
        // Protect against malformed bundled metadata; the path remains fixed-host.
        if endpoint.id != id
            || !endpoint.path.starts_with("/api/v5/")
            || endpoint.path.contains(['?', '#', '\\', '\r', '\n'])
            || endpoint.path.contains("..")
        {
            return None;
        }
        if !matches!(endpoint.method.as_str(), "GET" | "POST")
            || (endpoint.method == "GET" && endpoint.mutating)
        {
            return None;
        }
        Some(endpoint)
    }

    pub(crate) fn is_mutating(&self, id: &str) -> bool {
        self.endpoint(id).is_some_and(|endpoint| endpoint.mutating)
    }

    #[cfg(test)]
    pub(crate) fn execute(&self, request: ApiRequest, service: &CredentialService) -> Execution {
        self.execute_guarded(request, service, || true)
    }

    pub(crate) fn execute_guarded(
        &self,
        request: ApiRequest,
        service: &CredentialService,
        session_current: impl FnOnce() -> bool,
    ) -> Execution {
        let ApiRequest {
            endpoint_id,
            environment,
            credentials,
            mut params,
            confirmed,
            request_id,
            exp_time,
        } = request;
        if request_id.is_empty()
            || request_id.len() > 128
            || !request_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.:".contains(&c))
        {
            return Execution::before_send(String::new(), "invalid_request_id");
        }
        let Some(endpoint) = self.endpoint(&endpoint_id) else {
            return Execution::before_send(request_id, "unknown_endpoint");
        };
        if endpoint.mutating && !confirmed {
            return Execution::before_send(request_id, "confirmation_required");
        }
        if let Some(expiry) = exp_time.as_deref() {
            let allowed = matches!(
                endpoint.path.as_str(),
                "/api/v5/trade/order"
                    | "/api/v5/trade/batch-orders"
                    | "/api/v5/trade/amend-order"
                    | "/api/v5/trade/amend-batch-orders"
                    | "/api/v5/tradingBot/signal/sub-order"
            );
            if !allowed
                || endpoint.method != "POST"
                || expiry.is_empty()
                || expiry.len() > 20
                || !expiry.bytes().all(|c| c.is_ascii_digit())
                || !expiry.parse::<u64>().is_ok_and(|n| n > 0)
            {
                return Execution::before_send(request_id, "invalid_exp_time");
            }
        }
        let provider = match (endpoint.auth.as_str(), credentials, environment) {
            ("public", _, _) => None,
            ("read", ApiCredential::Read, ApiEnvironment::Production) => Some(ProviderId::Okx),
            ("read" | "trade", ApiCredential::Trade, ApiEnvironment::Demo) => {
                Some(ProviderId::OkxTradeDemo)
            }
            ("read" | "trade", ApiCredential::Trade, ApiEnvironment::Production) => {
                Some(ProviderId::OkxTradeLive)
            }
            ("withdraw", ApiCredential::Withdraw, ApiEnvironment::Production) => {
                Some(ProviderId::OkxWithdraw)
            }
            _ => return Execution::before_send(request_id, "credential_scope_mismatch"),
        };
        if validate_parameters(&endpoint, &mut params).is_err() {
            return Execution::before_send(request_id, "invalid_parameters");
        }
        let serialized = match serde_json::to_vec(&params) {
            Ok(value) if value.len() <= MAX_REQUEST_BYTES => Zeroizing::new(value),
            _ => return Execution::before_send(request_id, "request_too_large"),
        };
        let request_path = if endpoint.method == "GET" {
            let Some(object) = params.as_object() else {
                return Execution::before_send(request_id, "invalid_parameters");
            };
            let mut pairs = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in object {
                let Some(value) = query_value(value) else {
                    return Execution::before_send(request_id, "invalid_parameters");
                };
                pairs.append_pair(key, &value);
            }
            let query = pairs.finish();
            if query.is_empty() {
                endpoint.path.clone()
            } else {
                format!("{}?{query}", endpoint.path)
            }
        } else {
            endpoint.path.clone()
        };
        if request_path.len() > MAX_REQUEST_BYTES {
            return Execution::before_send(request_id, "request_too_large");
        }
        let loaded = match provider {
            Some(provider) => match service.load(provider) {
                Ok(Some(value)) => Some(value),
                Ok(None) => return Execution::before_send(request_id, "invalid_credentials"),
                Err(_) => return Execution::before_send(request_id, "secure_storage_unavailable"),
            },
            None => None,
        };
        let auth = loaded.as_ref().and_then(okx_credentials);
        if provider.is_some() && auth.is_none() {
            return Execution::before_send(request_id, "invalid_credentials");
        }
        let url = format!("{}{request_path}", self.base_url.trim_end_matches('/'));
        let request_body = if endpoint.method == "POST" {
            serialized.as_slice()
        } else {
            &[]
        };
        let signature = if let Some(auth) = auth.as_ref() {
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            let mut prehash = Zeroizing::new(
                format!("{timestamp}{}{request_path}", endpoint.method).into_bytes(),
            );
            prehash.extend_from_slice(request_body);
            let Ok(signature) = hmac_base64(auth.secret_key.as_bytes(), &prehash) else {
                return Execution::before_send(request_id, "invalid_credentials");
            };
            Some((timestamp, signature))
        } else {
            None
        };
        if !session_current() {
            return Execution::before_send(request_id, "session_changed");
        }
        macro_rules! configure {
            ($call:expr) => {{
                let mut call = $call;
                if endpoint.method == "POST" {
                    call = call.header("Content-Type", "application/json");
                }
                if environment == ApiEnvironment::Demo {
                    call = call.header("x-simulated-trading", "1");
                }
                if let Some(expiry) = exp_time.as_deref() {
                    call = call.header("expTime", expiry);
                }
                if let (Some(auth), Some((timestamp, signature))) =
                    (auth.as_ref(), signature.as_ref())
                {
                    call = call
                        .header("OK-ACCESS-KEY", auth.api_key)
                        .header("OK-ACCESS-SIGN", signature)
                        .header("OK-ACCESS-TIMESTAMP", timestamp)
                        .header("OK-ACCESS-PASSPHRASE", auth.passphrase);
                }
                call
            }};
        }
        let sent = if endpoint.method == "POST" {
            configure!(self.agent.post(&url)).send(request_body)
        } else {
            configure!(self.agent.get(&url)).call()
        };
        let mut response = match sent {
            Ok(response) => response,
            Err(_) => {
                return Execution {
                    response: ApiResponse::new(
                        if endpoint.mutating {
                            "unknown"
                        } else {
                            "rejected"
                        },
                        request_id,
                        "network_failure",
                    ),
                    sent: true,
                    mutating: endpoint.mutating,
                };
            }
        };
        let http_status = response.status().as_u16();
        if http_status == 408 || http_status >= 500 {
            let mut result = ApiResponse::new(
                if endpoint.mutating {
                    "unknown"
                } else {
                    "rejected"
                },
                request_id,
                "http_failure",
            );
            result.http_status = Some(http_status);
            return Execution {
                response: result,
                sent: true,
                mutating: endpoint.mutating,
            };
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let bytes = read_bounded(&mut response, MAX_JSON_BYTES);
        let envelope = bytes
            .as_ref()
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
        if endpoint.path == "/api/v5/market/books-sbe"
            && (200..300).contains(&http_status)
            && envelope.is_none()
        {
            if let Ok(bytes) = bytes
                && bytes.len() >= 8
                && (content_type.contains("sbe")
                    || content_type.contains("octet-stream")
                    || (content_type.is_empty()
                        && bytes.iter().any(|byte| *byte == 0 || *byte >= 0x80)))
            {
                let decoded = crate::crypto::sbe::decode_sbe(bytes.as_slice()).ok();
                let mut data = serde_json::json!({"format":"sbe", "encoding":"base64", "decoded":decoded.is_some(), "bytes":STANDARD.encode(bytes.as_slice())});
                if let Some(decoded) = decoded {
                    data["value"] = decoded;
                }
                return Execution {
                    response: ApiResponse {
                        status: "success",
                        code: None,
                        message: None,
                        data,
                        request_id,
                        http_status: Some(http_status),
                    },
                    sent: true,
                    mutating: false,
                };
            }
        }
        let Some(mut value) = envelope
            .filter(|v| v.get("code").and_then(Value::as_str).is_some() && v.get("data").is_some())
        else {
            let mut result = ApiResponse::new(
                if endpoint.mutating {
                    "unknown"
                } else {
                    "rejected"
                },
                request_id,
                "invalid_response",
            );
            result.http_status = Some(http_status);
            return Execution {
                response: result,
                sent: true,
                mutating: endpoint.mutating,
            };
        };
        // Classification must use the original codes: a credential may itself equal "0".
        let status = if value.get("code").and_then(Value::as_str) != Some("0")
            || !(200..300).contains(&http_status)
        {
            "rejected"
        } else if value.get("data").is_some_and(has_item_failure) {
            "partial_failure"
        } else {
            "success"
        };
        let mut secrets = Vec::new();
        collect_request_secrets(&params, &mut secrets);
        if let Some(auth) = auth.as_ref() {
            secrets.extend([auth.api_key, auth.secret_key, auth.passphrase]);
        }
        redact_value(&mut value, &secrets);
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(MAX_RESPONSE_CODE)
            .collect::<String>();
        let message = value
            .get("msg")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(MAX_RESPONSE_MESSAGE).collect());
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        Execution {
            response: ApiResponse {
                status,
                code: Some(code),
                message,
                data,
                request_id,
                http_status: Some(http_status),
            },
            sent: true,
            mutating: endpoint.mutating,
        }
    }
}

fn okx_credentials(input: &CredentialInput) -> Option<OkxCredentials<'_>> {
    let (api_key, secret_key, passphrase) = match input {
        CredentialInput::Okx {
            api_key,
            secret_key,
            passphrase,
        }
        | CredentialInput::OkxTradeDemo {
            api_key,
            secret_key,
            passphrase,
        }
        | CredentialInput::OkxTradeLive {
            api_key,
            secret_key,
            passphrase,
        }
        | CredentialInput::OkxWithdraw {
            api_key,
            secret_key,
            passphrase,
        } => (api_key, secret_key, passphrase),
        _ => return None,
    };
    Some(OkxCredentials {
        api_key: api_key.expose_secret(),
        secret_key: secret_key.expose_secret(),
        passphrase: passphrase.expose_secret(),
    })
}

fn query_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(_) | Value::Number(_) => Some(value.to_string()),
        _ => None,
    }
}

#[derive(Default)]
struct Node<'a> {
    variants: Vec<&'a Parameter>,
    children: BTreeMap<String, Node<'a>>,
}

fn insert_parameter<'a>(
    nodes: &mut BTreeMap<String, Node<'a>>,
    path: &[String],
    parameter: &'a Parameter,
) {
    let node = nodes.entry(path[0].clone()).or_default();
    if path.len() == 1 {
        node.variants.push(parameter);
    } else {
        insert_parameter(&mut node.children, &path[1..], parameter);
    }
}

fn validate_parameters(endpoint: &RestEndpoint, params: &mut Value) -> Result<(), ()> {
    let mut nodes = BTreeMap::new();
    let mut ancestors: Vec<String> = Vec::new();
    for parameter in &endpoint.parameters {
        let name = parameter.name.trim();
        let depth = name.chars().take_while(|c| *c == '>').count();
        let name = name[depth..].trim();
        if name.is_empty() || depth > ancestors.len() {
            return Err(());
        }
        ancestors.truncate(depth);
        ancestors.push(name.to_owned());
        insert_parameter(&mut nodes, &ancestors, parameter);
    }
    match endpoint.body_kind.as_str() {
        "object" => validate_object(params.as_object_mut().ok_or(())?, &nodes, true),
        "array" if endpoint.method == "POST" => {
            let array = params.as_array_mut().filter(|a| !a.is_empty()).ok_or(())?;
            for value in array {
                validate_object(value.as_object_mut().ok_or(())?, &nodes, true)?;
            }
            Ok(())
        }
        _ => Err(()),
    }
}

fn validate_object(
    object: &mut Map<String, Value>,
    nodes: &BTreeMap<String, Node<'_>>,
    reject_unknown: bool,
) -> Result<(), ()> {
    if reject_unknown && object.keys().any(|key| !nodes.contains_key(key)) {
        return Err(());
    }
    for (name, node) in nodes {
        // Duplicate names may describe different strategies. Enforce a field only when every variant requires it.
        let required =
            node.variants.iter().all(|p| p.required && !p.conditional) && !node.variants.is_empty();
        let Some(value) = object.get_mut(name) else {
            if required {
                return Err(());
            }
            continue;
        };
        if value.is_null() || (required && value.as_str().is_some_and(|v| v.is_empty())) {
            return Err(());
        }
        let types: HashSet<String> = node
            .variants
            .iter()
            .map(|p| p.parameter_type.to_ascii_lowercase())
            .collect();
        if types.len() == 1 {
            let ty = types.iter().next().unwrap().as_str();
            match ty {
                "array of objects" => {
                    let items = value.as_array_mut().ok_or(())?;
                    for item in items {
                        validate_object(
                            item.as_object_mut().ok_or(())?,
                            &node.children,
                            !node.children.is_empty(),
                        )?;
                    }
                }
                "array of strings" => {
                    if !value
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string))
                    {
                        return Err(());
                    }
                }
                "array" => {
                    if !value.is_array() {
                        return Err(());
                    }
                }
                "object" => validate_object(
                    value.as_object_mut().ok_or(())?,
                    &node.children,
                    !node.children.is_empty(),
                )?,
                "string" => {
                    if !value.is_string() {
                        return Err(());
                    }
                }
                "boolean" => {
                    if !value.is_boolean() {
                        return Err(());
                    }
                }
                "integer" => {
                    *value = Value::Number(exact_integer(value)?);
                }
                // Unknown future type names are left to OKX rather than imposing guessed rules.
                _ => (),
            }
        }
    }
    Ok(())
}

fn exact_integer(value: &Value) -> Result<serde_json::Number, ()> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return Err(()),
    };
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text.as_str()),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return Err(());
    }
    let trimmed = digits.trim_start_matches('0');
    let canonical = if trimmed.is_empty() {
        "0".to_owned()
    } else if negative {
        format!("-{trimmed}")
    } else {
        trimmed.to_owned()
    };
    canonical.parse().map_err(|_| ())
}

fn has_item_failure(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_item_failure),
        Value::Object(item) => {
            item.get("sCode")
                .and_then(Value::as_str)
                .is_some_and(|code| !code.is_empty() && code != "0")
                || item.values().any(has_item_failure)
        }
        _ => false,
    }
}

fn sensitive_field(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        key.as_str(),
        "apikey"
            | "secretkey"
            | "secret"
            | "passphrase"
            | "password"
            | "pwd"
            | "privatekey"
            | "accesskey"
            | "accesstoken"
            | "refreshtoken"
            | "token"
            | "signature"
    )
}

fn collect_request_secrets<'a>(value: &'a Value, secrets: &mut Vec<&'a str>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if sensitive_field(key) {
                    if let Some(secret) = value.as_str() {
                        secrets.push(secret);
                    }
                } else {
                    collect_request_secrets(value, secrets);
                }
            }
        }
        Value::Array(array) => {
            for item in array {
                collect_request_secrets(item, secrets);
            }
        }
        _ => (),
    }
}

fn redact_value(value: &mut Value, secrets: &[&str]) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if sensitive_field(key) {
                    *value = Value::String("[REDACTED]".into());
                } else {
                    redact_value(value, secrets);
                }
            }
        }
        Value::Array(array) => {
            for item in array {
                redact_value(item, secrets);
            }
        }
        Value::String(text) => {
            for secret in secrets {
                if !secret.is_empty() {
                    *text = text.replace(secret, "[REDACTED]");
                }
            }
        }
        _ => (),
    }
}
