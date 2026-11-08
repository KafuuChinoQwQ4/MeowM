//! Cancellable Binance WebSocket sessions with per-event acknowledgement.
//! Catalogue metadata selects only fixed Binance hosts. The stream is closed
//! when the ledger locks or its import generation changes.

use crate::connectors::{
    credentials::ConnectorState,
    markets::api::{
        self, ApiRequest, BinanceCredentials, encode_signature_pairs, hmac_hex,
        load_binance_credentials, now_millis, object_params, substitute_path,
    },
    model::ProviderErrorCode,
};
use crate::{LedgerState, access};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, State, ipc::Channel};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

const MAX_SESSIONS: usize = 4;
const MAX_QUEUE: usize = 32;
const MAX_FINISHED: usize = 64;
const ACK_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BinanceStreamEvent {
    pub(crate) kind: &'static str,
    pub(crate) sequence: u64,
    pub(crate) data: Value,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BinanceStreamStopResult {
    pub(crate) status: &'static str,
    pub(crate) request_id: String,
}

pub(crate) type StreamEvent = BinanceStreamEvent;
pub(crate) type StreamStopResult = BinanceStreamStopResult;

#[derive(Clone)]
struct SessionControl {
    cancel: watch::Sender<bool>,
    ack: mpsc::Sender<u64>,
    progress: Arc<Mutex<WriteProgress>>,
    request_id: String,
    mutating: bool,
}

#[derive(Default)]
pub(crate) struct BinanceStreamState {
    sessions: Arc<Mutex<HashMap<String, SessionControl>>>,
    finished: Arc<Mutex<VecDeque<(String, BinanceStreamStopResult)>>>,
}

#[derive(Default)]
struct WriteProgress {
    attempted: bool,
    answered: bool,
    cancelled: bool,
}

#[derive(Clone)]
struct StreamPlan {
    endpoint_id: String,
    transport: String,
    product: String,
    path: String,
    method: String,
    auth: String,
    environment: String,
    scope: String,
    request_id: String,
    params: Map<String, Value>,
    mutating: bool,
    confirmed: bool,
}

struct Finish {
    status: &'static str,
    reason: &'static str,
}

fn validate_request(request: &ApiRequest) -> Result<StreamPlan, String> {
    let invalid = || "invalid_binance_stream_request".to_string();
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return Err(invalid());
    }
    let entry = api::binance_catalogue()
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&request.endpoint_id))
        })
        .ok_or_else(invalid)?;
    let transport = entry
        .get("transport")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if !matches!(transport, "stream" | "ws_api") {
        return Err(invalid());
    }
    let environment = match api::validate_binance_environment(entry, request.environment.as_deref())
    {
        Ok(environment) => environment.to_string(),
        Err(_) => return Err("unsupported_binance_environment".into()),
    };
    let auth = entry
        .get("auth")
        .and_then(Value::as_str)
        .unwrap_or("public")
        .to_string();
    let mutating = entry
        .get("mutating")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if mutating && !request.confirmed {
        return Err("binance_write_confirmation_required".into());
    }
    if transport == "stream" && auth != "public" {
        return Err("unsupported_binance_stream_auth".into());
    }
    let scope = request.credentials.as_deref().unwrap_or("read");
    if !matches!(scope, "read" | "trade" | "withdraw") {
        return Err(invalid());
    }
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if path.len() > 4096 || path.starts_with("http:") || path.starts_with("https:") {
        return Err(invalid());
    }
    let mut params = object_params(&request.params).map_err(|_| invalid())?;
    let path = substitute_path(path, &mut params).map_err(|_| invalid())?;
    Ok(StreamPlan {
        endpoint_id: request.endpoint_id.clone(),
        transport: transport.into(),
        product: entry
            .get("product")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        path,
        method: entry
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("POST")
            .to_string(),
        auth,
        environment,
        scope: scope.into(),
        request_id: request.request_id.clone(),
        params,
        mutating,
        confirmed: request.confirmed,
    })
}

fn load_credentials(
    service: &crate::connectors::credential_store::CredentialService,
    plan: &StreamPlan,
) -> Result<Option<BinanceCredentials>, String> {
    load_binance_credentials(
        service,
        &plan.scope,
        &plan.environment,
        &plan.auth,
        plan.mutating,
        &plan.path,
    )
    .map_err(|error| match error {
        ProviderErrorCode::SecureStorageUnavailable => "secure_storage_unavailable".into(),
        ProviderErrorCode::PermissionDenied => "binance_credential_scope_rejected".into(),
        _ => "invalid_binance_credentials".into(),
    })
}

fn endpoint_url(plan: &StreamPlan) -> String {
    if plan.transport == "stream" {
        api::binance_stream_url(&plan.product, &plan.environment, &plan.path)
    } else {
        api::binance_ws_api_url(&plan.product, &plan.environment)
    }
}

fn public_session_valid(app: &tauri::AppHandle, generation: u64) -> bool {
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
    channel: &Channel<BinanceStreamEvent>,
    event: BinanceStreamEvent,
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

fn emit(sender: &mpsc::Sender<BinanceStreamEvent>, kind: &'static str, data: Value) -> bool {
    sender
        .try_send(BinanceStreamEvent {
            kind,
            sequence: 0,
            data,
        })
        .is_ok()
}

fn stop_status(progress: &Mutex<WriteProgress>, mutating: bool) -> &'static str {
    let Ok(mut state) = progress.lock() else {
        return if mutating { "unknown" } else { "closed" };
    };
    state.cancelled = true;
    if mutating && state.attempted && !state.answered {
        "unknown"
    } else if mutating && state.answered {
        "complete"
    } else {
        "closed"
    }
}

async fn pump_events(
    mut events: mpsc::Receiver<BinanceStreamEvent>,
    mut acks: mpsc::Receiver<u64>,
    mut cancel: watch::Receiver<bool>,
    send: Arc<dyn Fn(BinanceStreamEvent) -> bool + Send + Sync>,
) {
    let mut sequence = 0u64;
    loop {
        let mut event = tokio::select! {
            _ = cancel.changed() => return,
            next = events.recv() => match next { Some(value) => value, None => return },
        };
        sequence = sequence.saturating_add(1);
        event.sequence = sequence;
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
        tokio::select! {
            _ = cancel.changed() => return,
            result = acknowledged => if result != Ok(true) { return; },
        }
    }
}

async fn execute_ws_api(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    plan: &StreamPlan,
    credentials: Option<&BinanceCredentials>,
    mut cancel: watch::Receiver<bool>,
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
    events: &mpsc::Sender<BinanceStreamEvent>,
    progress: &Mutex<WriteProgress>,
) -> Finish {
    let mut params = plan.params.clone();
    if let Some(credentials) = credentials.filter(|_| plan.auth != "public") {
        params.insert(
            "apiKey".into(),
            Value::String(credentials.api_key.expose_secret().into()),
        );
        params
            .entry("timestamp")
            .or_insert_with(|| Value::Number(now_millis().into()));
        params
            .entry("recvWindow")
            .or_insert_with(|| Value::Number(5000u64.into()));
        let mut pairs = match params
            .iter()
            .map(|(key, value)| api::scalar(value).map(|v| (key.clone(), v)))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(pairs) => pairs,
            Err(_) => {
                return Finish {
                    status: if plan.mutating { "unknown" } else { "closed" },
                    reason: "invalid_params",
                };
            }
        };
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        let signature = match hmac_hex(
            credentials.secret_key.expose_secret().as_bytes(),
            encode_signature_pairs(&pairs).as_bytes(),
        ) {
            Ok(value) => value,
            Err(_) => {
                return Finish {
                    status: "closed",
                    reason: "signing_failed",
                };
            }
        };
        params.insert("signature".into(), Value::String(signature));
    }
    let payload =
        json!({"id":plan.request_id,"method":plan.path.trim_start_matches('/'),"params":params});
    if !valid() || *cancel.borrow() {
        return Finish {
            status: "closed",
            reason: "session_changed",
        };
    }
    if plan.mutating {
        if let Ok(mut state) = progress.lock() {
            state.attempted = true;
        }
    }
    if !matches!(
        tokio::time::timeout(
            IO_TIMEOUT,
            socket.send(Message::Text(payload.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    ) {
        return Finish {
            status: if plan.mutating { "unknown" } else { "closed" },
            reason: "send_failed",
        };
    }
    loop {
        tokio::select! {
            _ = cancel.changed() => { let _ = socket.close(None).await; return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "cancelled" }; },
            incoming = socket.next() => {
                let message = match incoming { Some(Ok(value)) => value, _ => return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "disconnected" } };
                if !valid() { let _ = socket.close(None).await; return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "session_changed" }; }
                let text = match message {
                    Message::Text(value) => value.to_string(),
                    Message::Binary(value) => match String::from_utf8(value.to_vec()) { Ok(value) => value, Err(_) => return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "invalid_response" } },
                    Message::Ping(value) => { let _ = socket.send(Message::Pong(value)).await; continue; },
                    Message::Close(_) => return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "disconnected" },
                    _ => continue,
                };
                let value: Value = match serde_json::from_str(&text) { Ok(value) => value, Err(_) => return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "invalid_response" } };
                let failed = value.get("error").is_some() || value.get("status").and_then(Value::as_u64).is_some_and(|status| status >= 400);
                if !emit(events, if failed { "error" } else { "response" }, value.clone()) { return Finish { status: if plan.mutating { "unknown" } else { "closed" }, reason: "consumer_overflow" }; }
                if !failed && plan.mutating {
                    if let Ok(mut state) = progress.lock() { state.answered = true; }
                }
                return Finish { status: if failed { "failed" } else if plan.mutating { "complete" } else { "closed" }, reason: if failed { "provider_error" } else { "response" } };
            }
        }
    }
}

async fn run_connection(
    url: String,
    plan: StreamPlan,
    credentials: Option<BinanceCredentials>,
    mut cancel: watch::Receiver<bool>,
    valid: Arc<dyn Fn() -> bool + Send + Sync>,
    events: mpsc::Sender<BinanceStreamEvent>,
    progress: Arc<Mutex<WriteProgress>>,
) -> Finish {
    if *cancel.borrow() || !valid() {
        return Finish {
            status: "closed",
            reason: "session_changed",
        };
    }
    let connected = tokio::select! {
        _ = cancel.changed() => return Finish { status: "closed", reason: "cancelled" },
        result = tokio::time::timeout(IO_TIMEOUT, crate::crypto::streams::connect_url_with_proxy(&url)) => result,
    };
    let (mut socket, _) = match connected {
        Ok(Ok(value)) => value,
        _ => {
            return Finish {
                status: "closed",
                reason: "connect_failed",
            };
        }
    };
    if !valid() {
        let _ = socket.close(None).await;
        return Finish {
            status: "closed",
            reason: "session_changed",
        };
    }
    if !emit(
        &events,
        "status",
        json!({"state":"connected","endpoint_id":plan.endpoint_id}),
    ) {
        return Finish {
            status: "closed",
            reason: "consumer_overflow",
        };
    }
    if plan.transport == "ws_api" {
        return execute_ws_api(
            &mut socket,
            &plan,
            credentials.as_ref(),
            cancel,
            valid,
            &events,
            &progress,
        )
        .await;
    }
    let payload = json!({"method":"SUBSCRIBE","params":[plan.path.trim_start_matches('/')],"id":1});
    if !valid() || *cancel.borrow() {
        let _ = socket.close(None).await;
        return Finish {
            status: "closed",
            reason: "session_changed",
        };
    }
    if !matches!(
        tokio::time::timeout(
            IO_TIMEOUT,
            socket.send(Message::Text(payload.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    ) {
        let _ = socket.close(None).await;
        return Finish {
            status: "closed",
            reason: "subscribe_failed",
        };
    }
    loop {
        tokio::select! {
            _ = cancel.changed() => { let _ = tokio::time::timeout(IO_TIMEOUT, socket.close(None)).await; return Finish { status: "closed", reason: "cancelled" }; },
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                if !valid() { let _ = tokio::time::timeout(IO_TIMEOUT, socket.close(None)).await; return Finish { status: "closed", reason: "session_changed" }; }
            }
            incoming = socket.next() => {
                let message = match incoming { Some(Ok(value)) => value, _ => return Finish { status: "closed", reason: "disconnected" } };
                if !valid() { let _ = socket.close(None).await; return Finish { status: "closed", reason: "session_changed" }; }
                let text = match message {
                    Message::Text(value) => value.to_string(),
                    Message::Binary(value) => match String::from_utf8(value.to_vec()) { Ok(value) => value, Err(_) => { let _ = emit(&events,"error",json!({"message":"invalid_utf8_frame"})); continue; } },
                    Message::Ping(value) => { if socket.send(Message::Pong(value)).await.is_err() { return Finish { status:"closed",reason:"pong_failed" }; } continue; },
                    Message::Close(_) => return Finish { status:"closed",reason:"server_closed" },
                    _ => continue,
                };
                let value: Value = match serde_json::from_str(&text) { Ok(value) => value, Err(_) => { if !emit(&events,"error",json!({"message":"invalid_json_frame"})) { return Finish{status:"closed",reason:"consumer_overflow"}; } continue; } };
                let (kind, data) = if value.get("code").is_some() && value.get("msg").is_some() {
                    ("error", value)
                } else if value.get("result").is_some() || value.get("id").is_some() {
                    ("ack", value)
                } else {
                    ("market", value)
                };
                if !emit(&events, kind, data) { return Finish { status:"closed", reason:"consumer_overflow" }; }
            }
        }
    }
}

#[tauri::command]
pub(crate) fn binance_ws_start(
    app: tauri::AppHandle,
    ledger: State<'_, LedgerState>,
    connectors: State<'_, ConnectorState>,
    state: State<'_, BinanceStreamState>,
    request: ApiRequest,
    on_event: Channel<BinanceStreamEvent>,
) -> Result<String, String> {
    let generation = access(ledger, |ledger| Ok(ledger.import_generation()))?;
    let plan = validate_request(&request)?;
    let credentials = load_credentials(&connectors.credentials(), &plan)?;
    if !public_session_valid(&app, generation) {
        return Err("crypto_session_changed".into());
    }
    let url = endpoint_url(&plan);
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (ack_tx, ack_rx) = mpsc::channel(8);
    let (events_tx, events_rx) = mpsc::channel(MAX_QUEUE);
    let progress = Arc::new(Mutex::new(WriteProgress::default()));
    let sessions = state.sessions.clone();
    let finished = state.finished.clone();
    {
        let mut sessions = sessions
            .lock()
            .map_err(|_| "stream_state_unavailable".to_string())?;
        if sessions.len() >= MAX_SESSIONS {
            return Err("too_many_streams".into());
        }
        sessions.insert(
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
            Arc::new(move || public_session_valid(&check_app, generation));
        let deliver_app = app.clone();
        let send: Arc<dyn Fn(BinanceStreamEvent) -> bool + Send + Sync> =
            Arc::new(move |event| deliver(&deliver_app, generation, &on_event, event));
        let pump_cancel = cancel_tx.clone();
        let pump = async move {
            pump_events(events_rx, ack_rx, cancel_rx, send).await;
            let _ = pump_cancel.send(true);
        };
        let network_cancel = cancel_tx.subscribe();
        let stream = async move {
            let request_id = plan.request_id.clone();
            let end = run_connection(
                url,
                plan.clone(),
                credentials,
                network_cancel.clone(),
                valid,
                events_tx.clone(),
                progress.clone(),
            )
            .await;
            if !*network_cancel.borrow() {
                let _ = events_tx.send(BinanceStreamEvent { kind:"closed", sequence:0, data:json!({"status":end.status,"reason":end.reason,"request_id":request_id}) }).await;
            }
        };
        tokio::join!(pump, stream);
        if let Ok(mut active) = sessions.lock() {
            if let Some(session) = active.remove(&id) {
                let result = BinanceStreamStopResult {
                    status: stop_status(&session.progress, session.mutating),
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
pub(crate) fn binance_ws_stop(
    state: State<'_, BinanceStreamState>,
    session_id: String,
) -> Result<BinanceStreamStopResult, String> {
    let sessions = state
        .sessions
        .lock()
        .map_err(|_| "stream_state_unavailable".to_string())?;
    if let Some(session) = sessions.get(&session_id) {
        let result = BinanceStreamStopResult {
            status: stop_status(&session.progress, session.mutating),
            request_id: session.request_id.clone(),
        };
        let _ = session.cancel.send(true);
        return Ok(result);
    }
    state
        .finished
        .lock()
        .map_err(|_| "stream_state_unavailable".to_string())?
        .iter()
        .find(|(id, _)| id == &session_id)
        .map(|(_, result)| result.clone())
        .ok_or_else(|| "stream_not_found".into())
}

#[tauri::command]
pub(crate) fn binance_ws_ack(
    state: State<'_, BinanceStreamState>,
    session_id: String,
    sequence: u64,
) -> Result<(), String> {
    let sessions = state
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
mod tests {
    use super::*;

    #[test]
    fn stream_url_uses_fixed_product_and_environment_hosts() {
        assert!(
            api::binance_stream_url(
                "Alpha WebSocket Market Streams",
                "production",
                "btcusdt@trade"
            )
            .starts_with("wss://nbstream.binance.com/w3w/wsa/stream/ws/")
        );
        assert!(
            api::binance_stream_url("USDⓈ-M Futures", "production", "btcusdt@markPrice")
                .contains("fstream.binance.com/market/ws/")
        );
        assert!(
            api::binance_stream_url("Spot WebSocket Streams", "spot_testnet", "btcusdt@trade")
                .starts_with("wss://data-stream.binance.vision/")
        );
    }
}
