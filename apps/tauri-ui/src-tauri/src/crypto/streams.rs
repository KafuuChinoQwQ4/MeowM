//! Bounded, catalogue-constrained OKX JSON WebSocket sessions.
use crate::connectors::{
    credential_store::CredentialService,
    credentials::ConnectorState,
    markets::okx::{advanced, model::OkxEnvironment},
    model::{CredentialInput, ProviderId, SensitiveString},
    payments::configured_proxy,
};
use crate::{LedgerState, access};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Manager, State, ipc::Channel};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, watch},
};
use tokio_socks::{TargetAddr, tcp::Socks5Stream};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    handshake::client::{Request, Response},
    protocol::{Message, WebSocketConfig},
};
use zeroize::Zeroizing;

const MAX_SESSIONS: usize = 4;
const MAX_QUEUE: usize = 32;
const MAX_MESSAGE: usize = 1024 * 1024;
const MAX_REQUEST: usize = 64 * 1024;
const MAX_FINISHED: usize = 64;
const ACK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WsCredential {
    Read,
    Trade,
    Withdraw,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WsRequest {
    endpoint_id: String,
    environment: OkxEnvironment,
    credentials: WsCredential,
    args: Vec<Value>,
    request_id: String,
    confirmed: bool,
    #[serde(default)]
    exp_time: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StreamEvent {
    kind: &'static str,
    data: Value,
    sequence: u64,
    #[serde(skip)]
    completes_write: bool,
}

#[derive(Clone)]
struct StreamPlan {
    environment: OkxEnvironment,
    service: String,
    operation: String,
    requires_login: bool,
    mutating: bool,
    request_id: String,
    payload: String,
}

#[derive(Clone)]
struct SessionControl {
    cancel: watch::Sender<bool>,
    ack: mpsc::Sender<u64>,
    progress: Arc<Mutex<WriteProgress>>,
    request_id: String,
    mutating: bool,
}

#[derive(Default)]
pub(crate) struct OkxStreamState {
    sessions: Arc<Mutex<HashMap<String, SessionControl>>>,
    finished: Arc<Mutex<VecDeque<(String, StreamStopResult)>>>,
}

#[derive(Default)]
struct WriteProgress {
    cancelled: bool,
    attempted: bool,
    acknowledged: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct StreamStopResult {
    status: &'static str,
    request_id: String,
}

fn begin_write(progress: &Mutex<WriteProgress>) -> bool {
    let Ok(mut state) = progress.lock() else {
        return false;
    };
    if state.cancelled {
        return false;
    }
    state.attempted = true;
    true
}

fn progress_status(state: &WriteProgress, mutating: bool) -> &'static str {
    if mutating && state.attempted && !state.acknowledged {
        "unknown"
    } else if mutating && state.acknowledged {
        "complete"
    } else {
        "closed"
    }
}

fn stop_progress(progress: &Mutex<WriteProgress>, mutating: bool) -> &'static str {
    match progress.lock() {
        Ok(mut state) => {
            state.cancelled = true;
            progress_status(&state, mutating)
        }
        Err(_) if mutating => "unknown",
        Err(_) => "closed",
    }
}

fn login_signature(timestamp: &str, secret: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(format!("{timestamp}GET/users/self/verify").as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

fn endpoint_url(environment: OkxEnvironment, service: &str) -> Result<String, String> {
    if !matches!(service, "public" | "private" | "business" | "public-sbe") {
        return Err("invalid_stream_service".into());
    }
    let host = match environment {
        OkxEnvironment::Demo => "wspap.okx.com",
        OkxEnvironment::Production => "ws.okx.com",
    };
    Ok(format!("wss://{host}/ws/v5/{service}"))
}

// Use the REST proxy configuration for WebSockets as well. Read CONNECT
// headers without consuming subsequent TLS bytes and never fall back to direct
// connections after a proxy failure.
async fn connect_with_proxy(
    request: Request,
    config: WebSocketConfig,
    proxy: Option<ureq::Proxy>,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    use tokio_tungstenite::tungstenite::Error;

    let Some(proxy) = proxy.filter(|value| !value.is_no_proxy(request.uri())) else {
        return tokio_tungstenite::connect_async_with_config(request, Some(config), false).await;
    };
    let fail = || Error::Io(std::io::Error::other("proxy connection failed"));
    let uri = request.uri();
    let host = uri
        .host()
        .ok_or_else(fail)?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
        Some("wss") => 443,
        Some("ws") => 80,
        _ => return Err(fail()),
    });
    let proxy_host = proxy.host().trim_matches(['[', ']']);
    let proxy_address = (proxy_host, proxy.port());
    let stream = match proxy.protocol() {
        ureq::ProxyProtocol::Http => {
            let mut stream = TcpStream::connect(proxy_address)
                .await
                .map_err(|_| fail())?;
            let target = if host.contains(':') {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            };
            let mut header =
                Zeroizing::new(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n"));
            if proxy.username().is_some() || proxy.password().is_some() {
                let credentials = Zeroizing::new(format!(
                    "{}:{}",
                    proxy.username().unwrap_or_default(),
                    proxy.password().unwrap_or_default()
                ));
                let encoded = Zeroizing::new(STANDARD.encode(credentials.as_bytes()));
                header.push_str(&format!(
                    "Proxy-Authorization: Basic {}\r\n",
                    encoded.as_str()
                ));
            }
            header.push_str("\r\n");
            stream
                .write_all(header.as_bytes())
                .await
                .map_err(|_| fail())?;
            let mut response = Vec::new();
            while !response.ends_with(b"\r\n\r\n") {
                if response.len() >= 8192 {
                    return Err(fail());
                }
                response.push(stream.read_u8().await.map_err(|_| fail())?);
            }
            let status = std::str::from_utf8(&response).map_err(|_| fail())?;
            let mut first_line = status.lines().next().ok_or_else(fail)?.split_whitespace();
            if !matches!(first_line.next(), Some("HTTP/1.0" | "HTTP/1.1"))
                || first_line.next() != Some("200")
            {
                return Err(fail());
            }
            stream
        }
        ureq::ProxyProtocol::Socks5 | ureq::ProxyProtocol::Socks5h => {
            let target = if proxy.resolve_target() {
                TargetAddr::Ip(
                    tokio::net::lookup_host((host.as_str(), port))
                        .await
                        .map_err(|_| fail())?
                        .next()
                        .ok_or_else(fail)?,
                )
            } else {
                TargetAddr::Domain(host.as_str().into(), port)
            };
            let socket = if proxy.username().is_some() || proxy.password().is_some() {
                Socks5Stream::connect_with_password(
                    proxy_address,
                    target,
                    proxy.username().unwrap_or_default(),
                    proxy.password().unwrap_or_default(),
                )
                .await
            } else {
                Socks5Stream::connect(proxy_address, target).await
            };
            socket.map_err(|_| fail())?.into_inner()
        }
        _ => return Err(fail()),
    };
    tokio_tungstenite::client_async_tls_with_config(request, stream, Some(config), None).await
}

/// Open a market WebSocket with the same HTTP/SOCKS proxy used by REST calls.
pub(crate) async fn connect_url_with_proxy(
    url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
        Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    let request = url.into_client_request()?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    connect_with_proxy(request, config, configured_proxy()).await
}

fn validate_request(request: &WsRequest, catalogue: &Value) -> Result<StreamPlan, String> {
    let invalid = || "invalid_stream_request".to_string();
    if request.request_id.is_empty()
        || request.request_id.len() > 32
        || !request
            .request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric())
        || request.args.is_empty()
        || request.args.iter().any(|v| !v.is_object())
    {
        return Err(invalid());
    }
    let endpoint = catalogue
        .get("websocket")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(&request.endpoint_id))
        })
        .ok_or_else(invalid)?;
    let service = endpoint
        .get("service")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    endpoint_url(request.environment, service)?;
    let operation = endpoint
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let requires_login = endpoint
        .get("requires_login")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    let mutating = endpoint
        .get("mutating")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    if service == "public-sbe" && (!requires_login || operation != "subscribe" || mutating) {
        return Err(invalid());
    }
    if operation == "subscribe" {
        if mutating {
            return Err(invalid());
        }
        let channels = endpoint
            .get("channels")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        for arg in &request.args {
            let channel = arg
                .get("channel")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !channels
                .iter()
                .any(|allowed| allowed.as_str() == Some(channel))
            {
                return Err(invalid());
            }
        }
    } else {
        // These are the documented JSON trade operations, including spread trading.
        if !matches!(
            operation,
            "order"
                | "batch-orders"
                | "cancel-order"
                | "batch-cancel-orders"
                | "amend-order"
                | "batch-amend-orders"
                | "mass-cancel"
                | "sprd-order"
                | "sprd-amend-order"
                | "sprd-cancel-order"
                | "sprd-mass-cancel"
        ) || !mutating
            || !requires_login
            || !matches!(service, "private" | "business")
            || request.credentials != WsCredential::Trade
            || !request.confirmed
        {
            return Err(invalid());
        }
    }
    // Withdrawal credentials are never loaded into a WebSocket session.
    if requires_login
        && (request.credentials == WsCredential::Withdraw
            || (request.environment == OkxEnvironment::Demo
                && request.credentials == WsCredential::Read))
    {
        return Err(invalid());
    }
    let mut message = json!({"id":request.request_id,"op":operation,"args":request.args});
    if let Some(deadline) = &request.exp_time {
        let supported = endpoint
            .get("parameters")
            .and_then(Value::as_array)
            .is_some_and(|params| {
                params
                    .iter()
                    .any(|param| param.get("name").and_then(Value::as_str) == Some("expTime"))
            });
        if !supported
            || deadline.is_empty()
            || deadline.len() > 20
            || !deadline.bytes().all(|c| c.is_ascii_digit())
            || deadline.parse::<u64>().is_err()
        {
            return Err(invalid());
        }
        message["expTime"] = json!(deadline);
    }
    let payload = serde_json::to_string(&message).map_err(|_| invalid())?;
    if payload.len() > MAX_REQUEST {
        return Err(invalid());
    }
    Ok(StreamPlan {
        environment: request.environment,
        service: service.into(),
        operation: operation.into(),
        requires_login,
        mutating,
        request_id: request.request_id.clone(),
        payload,
    })
}

struct StreamCredentials {
    api_key: SensitiveString,
    secret_key: SensitiveString,
    passphrase: SensitiveString,
}

fn load_credentials(
    service: &CredentialService,
    request: &WsRequest,
    plan: &StreamPlan,
) -> Result<Option<StreamCredentials>, String> {
    if !plan.requires_login {
        return Ok(None);
    }
    let provider = match (request.credentials, request.environment) {
        (WsCredential::Read, _) => ProviderId::Okx,
        (WsCredential::Trade, OkxEnvironment::Demo) => ProviderId::OkxTradeDemo,
        (WsCredential::Trade, OkxEnvironment::Production) => ProviderId::OkxTradeLive,
        (WsCredential::Withdraw, _) => return Err("invalid_stream_credentials".into()),
    };
    let value = service
        .load(provider)
        .map_err(|_| "secure_storage_unavailable".to_string())?
        .ok_or_else(|| "invalid_credentials".to_string())?;
    let (api_key, secret_key, passphrase) = match value {
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
        } => (api_key, secret_key, passphrase),
        _ => return Err("invalid_credentials".into()),
    };
    Ok(Some(StreamCredentials {
        api_key,
        secret_key,
        passphrase,
    }))
}

fn redact(value: &mut Value, credentials: Option<&StreamCredentials>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "apikey"
                        | "api_key"
                        | "secretkey"
                        | "secret_key"
                        | "passphrase"
                        | "sign"
                        | "signature"
                ) {
                    *value = json!("[REDACTED]");
                } else {
                    redact(value, credentials);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact(item, credentials);
            }
        }
        Value::String(text) => {
            if let Some(secret) = credentials {
                for sensitive in [
                    secret.api_key.expose_secret(),
                    secret.secret_key.expose_secret(),
                    secret.passphrase.expose_secret(),
                ] {
                    if !sensitive.is_empty() && text.contains(sensitive) {
                        *text = text.replace(sensitive, "[REDACTED]");
                    }
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
struct Timings {
    connect: Duration,
    login: Duration,
    idle: Duration,
    pong: Duration,
    tick: Duration,
    write: Duration,
}
impl Default for Timings {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(12),
            login: Duration::from_secs(12),
            idle: Duration::from_secs(20),
            pong: Duration::from_secs(9),
            tick: Duration::from_millis(500),
            write: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
struct Finish {
    reason: &'static str,
    status: &'static str,
}

fn finish(plan: &StreamPlan, attempted: bool, answered: bool, reason: &'static str) -> Finish {
    Finish {
        reason,
        status: if plan.mutating && attempted && !answered {
            "unknown"
        } else if answered {
            "complete"
        } else {
            "closed"
        },
    }
}

fn emit(sender: &mpsc::Sender<StreamEvent>, kind: &'static str, data: Value) -> bool {
    emit_with_completion(sender, kind, data, false)
}

fn emit_with_completion(
    sender: &mpsc::Sender<StreamEvent>,
    kind: &'static str,
    data: Value,
    completes_write: bool,
) -> bool {
    sender
        .try_send(StreamEvent {
            kind,
            data,
            sequence: 0,
            completes_write,
        })
        .is_ok()
}

async fn run_connection(
    url: &str,
    plan: StreamPlan,
    credentials: Option<StreamCredentials>,
    mut cancel: watch::Receiver<bool>,
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
    events: mpsc::Sender<StreamEvent>,
    timing: Timings,
    progress: Arc<Mutex<WriteProgress>>,
) -> Finish {
    let mut attempted = false;
    let mut answered = false;
    if *cancel.borrow() || !valid() {
        return finish(&plan, false, false, "session_changed");
    }
    let mut request = match url.into_client_request() {
        Ok(v) => v,
        Err(_) => return finish(&plan, false, false, "connect_failed"),
    };
    if plan.environment == OkxEnvironment::Demo {
        request
            .headers_mut()
            .insert("x-simulated-trading", "1".parse().unwrap());
    }
    let sbe = plan.service == "public-sbe";
    if sbe {
        let Some(secret) = credentials.as_ref() else {
            return finish(&plan, false, false, "invalid_credentials");
        };
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = Zeroizing::new(login_signature(
            &timestamp,
            secret.secret_key.expose_secret(),
        ));
        for (name, value) in [
            ("ok-access-key", secret.api_key.expose_secret()),
            ("ok-access-passphrase", secret.passphrase.expose_secret()),
            ("ok-access-timestamp", timestamp.as_str()),
            ("ok-access-sign", signature.as_str()),
        ] {
            let parsed = match value.parse() {
                Ok(v) => v,
                Err(_) => return finish(&plan, false, false, "invalid_credentials"),
            };
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::header::HeaderName::from_static(name),
                parsed,
            );
        }
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    let connect = tokio::time::timeout(
        timing.connect,
        connect_with_proxy(request, config, configured_proxy()),
    );
    tokio::pin!(connect);
    let mut check = tokio::time::interval(timing.tick);
    let connected = loop {
        tokio::select! {
            _ = cancel.changed() => return finish(&plan, false, false, "cancelled"),
            _ = check.tick() => if !valid() { return finish(&plan, false, false, "session_changed"); },
            result = &mut connect => break result,
        }
    };
    let (mut socket, _) = match connected {
        Ok(Ok(v)) => v,
        _ => return finish(&plan, false, false, "connect_failed"),
    };
    if *cancel.borrow() || !valid() {
        return finish(&plan, false, false, "session_changed");
    }
    if !emit(&events, "status", json!({"state":"connected"})) {
        return finish(&plan, false, false, "consumer_overflow");
    }
    let mut logged_in = !plan.requires_login || sbe;
    let login_deadline = Instant::now() + timing.login;
    if let Some(secret) = credentials.as_ref().filter(|_| !sbe) {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = Zeroizing::new(login_signature(
            &timestamp,
            secret.secret_key.expose_secret(),
        ));
        let login = Zeroizing::new(json!({"op":"login","args":[{"apiKey":secret.api_key.expose_secret(),
            "passphrase":secret.passphrase.expose_secret(),"timestamp":timestamp,"sign":signature.as_str()}]}).to_string());
        if !valid() {
            return finish(&plan, false, false, "session_changed");
        }
        if !matches!(
            tokio::time::timeout(
                timing.write,
                socket.send(Message::Text(login.as_str().into()))
            )
            .await,
            Ok(Ok(()))
        ) {
            return finish(&plan, false, false, "login_send_failed");
        }
    } else if plan.requires_login && !sbe {
        return finish(&plan, false, false, "invalid_credentials");
    }
    let mut submitted = false;
    let mut last_received = Instant::now();
    let mut ping_sent: Option<Instant> = None;
    let mut tick = tokio::time::interval(timing.tick);
    loop {
        if logged_in && !submitted {
            if *cancel.borrow() || !valid() {
                return finish(&plan, attempted, answered, "session_changed");
            }
            // Once a write starts, a lost connection cannot establish whether OKX accepted it.
            if plan.mutating && !begin_write(&progress) {
                return finish(&plan, false, false, "cancelled");
            }
            attempted = plan.mutating;
            submitted = true;
            if !matches!(
                tokio::time::timeout(
                    timing.write,
                    socket.send(Message::Text(plan.payload.clone().into()))
                )
                .await,
                Ok(Ok(()))
            ) {
                return finish(&plan, attempted, answered, "send_failed");
            }
        }
        tokio::select! {
            _ = cancel.changed() => { let _ = tokio::time::timeout(timing.write, socket.close(None)).await; return finish(&plan, attempted, answered, "cancelled"); }
            _ = tick.tick() => {
                if !valid() { let _ = tokio::time::timeout(timing.write, socket.close(None)).await; return finish(&plan, attempted, answered, "session_changed"); }
                if !logged_in && Instant::now() >= login_deadline { return finish(&plan, attempted, answered, "login_timeout"); }
                if (sbe && last_received.elapsed() >= timing.idle * 3)
                    || ping_sent.is_some_and(|at| at.elapsed() >= timing.pong) { return finish(&plan, attempted, answered, "heartbeat_timeout"); }
                if !sbe && ping_sent.is_none() && last_received.elapsed() >= timing.idle {
                    if !matches!(tokio::time::timeout(timing.write, socket.send(Message::Text("ping".into()))).await, Ok(Ok(()))) {
                        return finish(&plan, attempted, answered, "heartbeat_failed");
                    }
                    ping_sent = Some(Instant::now());
                }
            }
            incoming = socket.next() => {
                let message = match incoming { Some(Ok(v)) => v, _ => return finish(&plan, attempted, answered, "disconnected") };
                last_received = Instant::now();
                match message {
                    Message::Text(text) => {
                        if text == "pong" { ping_sent = None; continue; }
                        let mut value: Value = match serde_json::from_str(&text) { Ok(v) => v, Err(_) => return finish(&plan, attempted, answered, "invalid_message") };
                        if !logged_in {
                            if value.get("event").and_then(Value::as_str) == Some("login")
                                && value.get("code").and_then(Value::as_str) == Some("0") {
                                logged_in = true;
                                if !emit(&events, "status", json!({"state":"authenticated"})) { return finish(&plan, attempted, answered, "consumer_overflow"); }
                            } else if matches!(value.get("event").and_then(Value::as_str), Some("login" | "error")) {
                                return finish(&plan, attempted, answered, "login_rejected");
                            }
                            continue;
                        }
                        if value.get("event").and_then(Value::as_str) == Some("login") { continue; }
                        let is_answer = plan.mutating && value.get("id").and_then(Value::as_str) == Some(&plan.request_id)
                            && (value.get("op").and_then(Value::as_str) == Some(&plan.operation)
                                || value.get("event").and_then(Value::as_str) == Some("error"));
                        redact(&mut value, credentials.as_ref());
                        if !valid() { return finish(&plan, attempted, answered, "session_changed"); }
                        if !emit_with_completion(&events, "message", value, is_answer) { return finish(&plan, attempted, answered, "consumer_overflow"); }
                        if is_answer { answered = true; }
                    }
                    Message::Ping(_) => { if !matches!(tokio::time::timeout(timing.write, socket.flush()).await, Ok(Ok(()))) { return finish(&plan, attempted, answered, "disconnected"); } }
                    Message::Pong(_) => ping_sent = None,
                    Message::Close(_) => return finish(&plan, attempted, answered, "disconnected"),
                    Message::Binary(bytes) => {
                        if !sbe { return finish(&plan, attempted, answered, "unsupported_binary_message"); }
                        if !valid() { return finish(&plan, attempted, answered, "session_changed"); }
                        let raw = STANDARD.encode(&bytes);
                        let data = match crate::crypto::sbe::decode_sbe(&bytes) {
                            Ok(value) => json!({"format":"sbe-base64", "data":raw, "decoded":true, "value":value}),
                            Err(_) => json!({"format":"sbe-base64", "data":raw, "decoded":false}),
                        };
                        if !emit(&events, "message", data) {
                            return finish(&plan, attempted, answered, "consumer_overflow");
                        }
                    }
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

fn valid_session(app: &tauri::AppHandle, generation: u64) -> bool {
    let Some(state) = app.try_state::<LedgerState>() else {
        return false;
    };
    let Ok(ledger) = state.0.lock() else {
        return false;
    };
    ledger.require_unlocked().is_ok() && ledger.import_generation() == generation
}

fn deliver(
    app: &tauri::AppHandle,
    generation: u64,
    channel: &Channel<StreamEvent>,
    event: StreamEvent,
) -> bool {
    let Some(state) = app.try_state::<LedgerState>() else {
        return false;
    };
    let Ok(ledger) = state.0.lock() else {
        return false;
    };
    if ledger.require_unlocked().is_err() || ledger.import_generation() != generation {
        return false;
    }
    channel.send(event).is_ok()
}

async fn pump_events(
    mut events: mpsc::Receiver<StreamEvent>,
    mut acks: mpsc::Receiver<u64>,
    mut cancel: watch::Receiver<bool>,
    send: Arc<dyn Fn(StreamEvent) -> bool + Send + Sync>,
    progress: Arc<Mutex<WriteProgress>>,
) {
    let mut sequence = 0u64;
    loop {
        let mut event = tokio::select! {
            _ = cancel.changed() => return,
            event = events.recv() => match event { Some(v) => v, None => return },
        };
        sequence += 1;
        event.sequence = sequence;
        let completes_write = event.completes_write;
        if !send(event) {
            return;
        }
        let acknowledged = tokio::time::timeout(ACK_TIMEOUT, async {
            loop {
                match acks.recv().await {
                    Some(value) if value == sequence => return true,
                    Some(_) => {}
                    None => return false,
                }
            }
        });
        tokio::select! { _ = cancel.changed() => return, result = acknowledged => if result != Ok(true) { return; } }
        if completes_write {
            if let Ok(mut state) = progress.lock() {
                state.acknowledged = true;
            }
        }
    }
}

#[tauri::command]
pub(crate) fn okx_ws_start(
    app: tauri::AppHandle,
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    streams: State<'_, OkxStreamState>,
    request: WsRequest,
    on_event: Channel<StreamEvent>,
) -> Result<String, String> {
    let generation = access(ledger, |ledger| Ok(ledger.import_generation()))?;
    let plan = validate_request(&request, advanced::catalogue())?;
    let credentials = load_credentials(&connectors.credentials(), &request, &plan)?;
    if !valid_session(&app, generation) {
        return Err("crypto_session_changed".into());
    }
    let url = endpoint_url(plan.environment, &plan.service)?;
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (ack_tx, ack_rx) = mpsc::channel(8);
    let (events_tx, events_rx) = mpsc::channel(MAX_QUEUE);
    let sessions = streams.sessions.clone();
    let finished = streams.finished.clone();
    let progress = Arc::new(Mutex::new(WriteProgress::default()));
    {
        let mut active = sessions
            .lock()
            .map_err(|_| "stream_state_unavailable".to_string())?;
        if active.len() >= MAX_SESSIONS {
            return Err("too_many_streams".into());
        }
        active.insert(
            session_id.clone(),
            SessionControl {
                cancel: cancel_tx.clone(),
                ack: ack_tx,
                progress: progress.clone(),
                request_id: plan.request_id.clone(),
                mutating: plan.mutating,
            },
        );
    }
    let id = session_id.clone();
    tauri::async_runtime::spawn(async move {
        let check_app = app.clone();
        let valid: Arc<dyn Fn() -> bool + Send + Sync> =
            Arc::new(move || valid_session(&check_app, generation));
        let deliver_app = app.clone();
        let send: Arc<dyn Fn(StreamEvent) -> bool + Send + Sync> =
            Arc::new(move |event| deliver(&deliver_app, generation, &on_event, event));
        let pump_cancel = cancel_tx.clone();
        let pump_progress = progress.clone();
        let mutating = plan.mutating;
        let pump = async move {
            pump_events(
                events_rx,
                ack_rx,
                cancel_rx.clone(),
                send,
                pump_progress.clone(),
            )
            .await;
            stop_progress(&pump_progress, mutating);
            let _ = pump_cancel.send(true);
        };
        let network_cancel = cancel_tx.subscribe();
        let stream = async move {
            let request_id = plan.request_id.clone();
            let end = run_connection(
                &url,
                plan,
                credentials,
                network_cancel.clone(),
                valid,
                events_tx.clone(),
                Timings::default(),
                progress,
            )
            .await;
            if !*network_cancel.borrow() {
                let _ = events_tx.send(StreamEvent { kind:"closed", data:json!({"status":end.status,"reason":end.reason,"request_id":request_id}), sequence:0, completes_write:false }).await;
            }
        };
        tokio::join!(pump, stream);
        if let Ok(mut active) = sessions.lock() {
            if let Some(session) = active.remove(&id) {
                let result = StreamStopResult {
                    status: stop_progress(&session.progress, session.mutating),
                    request_id: session.request_id,
                };
                if let Ok(mut history) = finished.lock() {
                    history.push_back((id, result));
                    if history.len() > MAX_FINISHED {
                        history.pop_front();
                    }
                }
            }
        }
    });
    Ok(session_id)
}

#[tauri::command]
pub(crate) fn okx_ws_stop(
    streams: State<'_, OkxStreamState>,
    session_id: String,
) -> Result<StreamStopResult, String> {
    let sessions = streams
        .sessions
        .lock()
        .map_err(|_| "stream_state_unavailable".to_string())?;
    if let Some(session) = sessions.get(&session_id) {
        let result = StreamStopResult {
            status: stop_progress(&session.progress, session.mutating),
            request_id: session.request_id.clone(),
        };
        let _ = session.cancel.send(true);
        return Ok(result);
    }
    let finished = streams
        .finished
        .lock()
        .map_err(|_| "stream_state_unavailable".to_string())?;
    finished
        .iter()
        .find(|(id, _)| id == &session_id)
        .map(|(_, result)| result.clone())
        .ok_or_else(|| "stream_not_found".to_string())
}

#[tauri::command]
pub(crate) fn okx_ws_ack(
    streams: State<'_, OkxStreamState>,
    session_id: String,
    sequence: u64,
) -> Result<(), String> {
    let sessions = streams
        .sessions
        .lock()
        .map_err(|_| "stream_state_unavailable".to_string())?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| "stream_closed".to_string())?;
    session
        .ack
        .try_send(sequence)
        .map_err(|_| "stream_ack_rejected".to_string())
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod streams_tests;
