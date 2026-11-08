use super::*;

#[test]
fn login_signature_uses_unix_seconds_get_and_verify_path() {
    assert_eq!(
        login_signature("1700000000", "supersecret"),
        "O8ohHd2naCXyB1GyZODNuSq7dfsHHcg4pqwko0q9/T0="
    );
}

fn fixture() -> serde_json::Value {
    serde_json::json!({"websocket": [
        {"id":"candles","service":"business","operation":"subscribe",
         "requires_login":false,"mutating":false,"channels":["candle1m","candle5m"]},
        {"id":"order","service":"private","operation":"order",
         "requires_login":true,"mutating":true,"channels":[]},
        {"id":"bogus","service":"private","operation":"login",
         "requires_login":true,"mutating":true,"channels":[]}
    ]})
}

fn candle_request() -> WsRequest {
    WsRequest {
        endpoint_id: "candles".into(),
        environment: OkxEnvironment::Demo,
        credentials: WsCredential::Read,
        args: vec![serde_json::json!({"channel":"candle1m","instId":"BTC-USDT"})],
        request_id: "123abc".into(),
        confirmed: false,
        exp_time: None,
    }
}

fn order_request() -> WsRequest {
    WsRequest {
        endpoint_id: "order".into(),
        environment: OkxEnvironment::Demo,
        credentials: WsCredential::Trade,
        args: vec![serde_json::json!({"instId":"BTC-USDT","side":"buy","sz":"1"})],
        request_id: "order123".into(),
        confirmed: true,
        exp_time: None,
    }
}

#[test]
fn supported_order_deadline_is_sent_as_top_level_field() {
    let mut catalogue = fixture();
    catalogue["websocket"][1]["parameters"] = serde_json::json!([{"name":"expTime"}]);
    let mut request = order_request();
    request.exp_time = Some("1700000000123".into());
    let plan = validate_request(&request, &catalogue).unwrap();
    let message: serde_json::Value = serde_json::from_str(&plan.payload).unwrap();
    assert_eq!(message["expTime"], "1700000000123");
    assert!(message["args"][0].get("expTime").is_none());
    request.exp_time = Some("not-a-timestamp".into());
    assert!(validate_request(&request, &catalogue).is_err());
    request = candle_request();
    request.exp_time = Some("1700000000123".into());
    assert!(validate_request(&request, &catalogue).is_err());
}

#[test]
fn demo_private_subscription_requires_demo_credentials() {
    let mut catalogue = fixture();
    catalogue["websocket"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"account", "service":"private",
        "requires_login":true, "mutating":false, "operation":"subscribe", "channels":["account"]}));
    let mut request = candle_request();
    request.endpoint_id = "account".into();
    request.args = vec![json!({"channel":"account"})];
    assert!(validate_request(&request, &catalogue).is_err());
    request.credentials = WsCredential::Trade;
    assert!(validate_request(&request, &catalogue).is_ok());
}

#[test]
fn stopping_and_starting_a_write_are_serialized_and_unknown_is_preserved() {
    let progress = Mutex::new(WriteProgress::default());
    assert!(begin_write(&progress));
    assert_eq!(stop_progress(&progress, true), "unknown");
    let cancelled = Mutex::new(WriteProgress::default());
    assert_eq!(stop_progress(&cancelled, true), "closed");
    assert!(!begin_write(&cancelled));
    let complete = Mutex::new(WriteProgress::default());
    assert!(begin_write(&complete));
    complete.lock().unwrap().acknowledged = true;
    assert_eq!(stop_progress(&complete, true), "complete");
}

#[test]
fn every_catalogued_websocket_example_passes_transport_allowlist() {
    let catalogue = advanced::catalogue();
    for endpoint in catalogue["websocket"].as_array().unwrap() {
        let mut request = candle_request();
        request.endpoint_id = endpoint["id"].as_str().unwrap().to_owned();
        request.environment = OkxEnvironment::Production;
        request.credentials = WsCredential::Trade;
        request.confirmed = true;
        request.args = endpoint["sample_args"].as_array().unwrap().clone();
        assert!(
            validate_request(&request, catalogue).is_ok(),
            "{}",
            request.endpoint_id
        );
    }
}

#[test]
fn a_full_event_queue_causes_a_visible_overflow_instead_of_unbounded_buffering() {
    let (events, _received) = mpsc::channel(1);
    assert!(emit(&events, "message", json!({"value":1})));
    assert!(!emit(&events, "message", json!({"value":2})));
}

#[test]
fn catalog_controls_service_operation_channel_and_trade_confirmation() {
    let mut request = candle_request();
    let plan = validate_request(&request, &fixture()).unwrap();
    assert_eq!(
        endpoint_url(plan.environment, &plan.service).unwrap(),
        "wss://wspap.okx.com/ws/v5/business"
    );
    request.args[0]["channel"] = serde_json::json!("candle42m");
    assert!(validate_request(&request, &fixture()).is_err());
    request.endpoint_id = "does-not-exist".into();
    assert!(validate_request(&request, &fixture()).is_err());
    request.endpoint_id = "bogus".into();
    assert!(validate_request(&request, &fixture()).is_err());
    let mut request = order_request();
    assert!(validate_request(&request, &fixture()).is_ok());
    request.credentials = WsCredential::Read;
    assert!(validate_request(&request, &fixture()).is_err());
    request.credentials = WsCredential::Trade;
    request.confirmed = false;
    assert!(validate_request(&request, &fixture()).is_err());
    request.confirmed = true;
    request.request_id = "invalid-id".into();
    assert!(validate_request(&request, &fixture()).is_err());
    assert!(endpoint_url(OkxEnvironment::Production, "arbitrary").is_err());
    assert_eq!(
        endpoint_url(OkxEnvironment::Production, "public").unwrap(),
        "wss://ws.okx.com/ws/v5/public"
    );
}

fn fast_timings() -> Timings {
    Timings {
        connect: Duration::from_secs(2),
        login: Duration::from_secs(1),
        idle: Duration::from_millis(80),
        pong: Duration::from_millis(150),
        tick: Duration::from_millis(10),
        write: Duration::from_secs(1),
    }
}

fn fake_credentials() -> StreamCredentials {
    StreamCredentials {
        api_key: SensitiveString::new("demo-api-key".into()).unwrap(),
        secret_key: SensitiveString::new("demo-secret-key".into()).unwrap(),
        passphrase: SensitiveString::new("demo-passphrase".into()).unwrap(),
    }
}

#[tokio::test]
#[ignore = "requires a live public OKX WebSocket endpoint; no private credentials"]
async fn live_public_ticker_subscription_uses_configured_proxy() {
    for (environment, url) in [
        (OkxEnvironment::Production, "wss://ws.okx.com/ws/v5/public"),
        (OkxEnvironment::Demo, "wss://wspap.okx.com/ws/v5/public"),
    ] {
        let mut request = candle_request();
        request.endpoint_id = "order-book-trading-market-data-ws-tickers-channel".into();
        request.environment = environment;
        request.args = vec![json!({"channel":"tickers","instId":"BTC-USDT"})];
        let plan = validate_request(&request, advanced::catalogue()).unwrap();
        let (cancel, cancellation) = watch::channel(false);
        let (events, mut received) = mpsc::channel(MAX_QUEUE);
        let mut task = tokio::spawn(run_connection(
            url,
            plan,
            None,
            cancellation,
            Arc::new(|| true),
            events,
            Timings::default(),
            Arc::new(Mutex::new(WriteProgress::default())),
        ));
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                tokio::select! {
                    event = received.recv() => {
                        if let Some(event) = event {
                            if event.data["arg"]["channel"] == "tickers"
                                && event.data["data"].as_array().is_some_and(|items| !items.is_empty()) {
                                break;
                            }
                        }
                    }
                    outcome = &mut task => panic!("live OKX stream ended: {:?}", outcome.unwrap()),
                }
            }
        })
        .await
        .expect("live OKX ticker timed out");
        cancel.send(true).unwrap();
        assert_eq!(task.await.unwrap().reason, "cancelled");
    }
}

#[tokio::test]
async fn websocket_uses_http_connect_proxy_without_resolving_the_target_locally() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            headers.push(stream.read_u8().await.unwrap());
        }
        let headers = String::from_utf8(headers).unwrap();
        assert!(headers.starts_with("CONNECT ws.okx.com:80 HTTP/1.1\r\n"));
        assert!(headers.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "hello"
        );
        socket.send(Message::Text("world".into())).await.unwrap();
    });
    let proxy = ureq::Proxy::new(&format!("http://user:pass@{address}")).unwrap();
    let request = "ws://ws.okx.com/ws/v5/public"
        .into_client_request()
        .unwrap();
    let (mut socket, _) =
        super::connect_with_proxy(request, WebSocketConfig::default(), Some(proxy))
            .await
            .unwrap();
    socket.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap().into_text().unwrap(),
        "world"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn websocket_does_not_fall_back_to_direct_connection_when_proxy_rejects_tunnel() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            headers.push(stream.read_u8().await.unwrap());
        }
        stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
            .await
            .unwrap();
    });
    let proxy = ureq::Proxy::new(&format!("http://{address}")).unwrap();
    let request = "ws://ws.okx.com/ws/v5/public"
        .into_client_request()
        .unwrap();
    assert!(
        super::connect_with_proxy(request, WebSocketConfig::default(), Some(proxy))
            .await
            .is_err()
    );
    server.await.unwrap();
}

#[tokio::test]
async fn websocket_socks5h_proxy_receives_remote_hostname() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut greeting = [0; 3];
        stream.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting, [5, 1, 0]);
        stream.write_all(&[5, 0]).await.unwrap();
        let mut prefix = [0; 5];
        stream.read_exact(&mut prefix).await.unwrap();
        assert_eq!(&prefix[..4], &[5, 1, 0, 3]);
        let mut host = vec![0; usize::from(prefix[4])];
        stream.read_exact(&mut host).await.unwrap();
        assert_eq!(&host, b"ws.okx.com");
        let mut port = [0; 2];
        stream.read_exact(&mut port).await.unwrap();
        assert_eq!(u16::from_be_bytes(port), 80);
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
            .await
            .unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "hello"
        );
    });
    let proxy = ureq::Proxy::new(&format!("socks5h://{address}")).unwrap();
    let request = "ws://ws.okx.com/ws/v5/public"
        .into_client_request()
        .unwrap();
    let (mut socket, _) =
        super::connect_with_proxy(request, WebSocketConfig::default(), Some(proxy))
            .await
            .unwrap();
    socket.send(Message::Text("hello".into())).await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn private_socket_authenticates_then_reports_unknown_if_trade_disconnects() {
    use tokio::net::TcpListener;
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(stream, |request: &Request, response: Response| {
            assert_eq!(request.headers().get("x-simulated-trading").unwrap(), "1");
            Ok(response)
        })
        .await
        .unwrap();
        let login = socket.next().await.unwrap().unwrap().into_text().unwrap();
        let login: Value = serde_json::from_str(&login).unwrap();
        assert_eq!(login["op"], "login");
        assert_eq!(login["args"][0]["apiKey"], "demo-api-key");
        assert_eq!(login["args"][0]["passphrase"], "demo-passphrase");
        let timestamp = login["args"][0]["timestamp"].as_str().unwrap();
        assert_eq!(
            login["args"][0]["sign"],
            login_signature(timestamp, "demo-secret-key")
        );
        socket
            .send(Message::Text(r#"{"event":"login","code":"0"}"#.into()))
            .await
            .unwrap();
        let sent: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        assert_eq!(sent["id"], "order123");
        assert_eq!(sent["op"], "order");
        socket
            .send(Message::Text(
                r#"{"msg":"demo-secret-key demo-passphrase demo-api-key"}"#.into(),
            ))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
    });
    let plan = validate_request(&order_request(), &fixture()).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let result = run_connection(
        &format!("ws://{address}/ws/v5/private"),
        plan,
        Some(fake_credentials()),
        cancel,
        Arc::new(|| true),
        events,
        fast_timings(),
        Arc::new(Mutex::new(WriteProgress::default())),
    )
    .await;
    server.await.unwrap();
    assert_eq!(result.status, "unknown");
    let mut delivered = Vec::new();
    while let Ok(event) = received.try_recv() {
        delivered.push(event);
    }
    let serialized = serde_json::to_string(&delivered).unwrap();
    assert!(!serialized.contains("demo-secret-key"));
    assert!(!serialized.contains("demo-api-key"));
    assert!(!serialized.contains("demo-passphrase"));
    assert!(serialized.contains("[REDACTED]"));
}

#[tokio::test]
async fn heartbeat_uses_text_ping_and_cancellation_closes_socket() {
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, seen_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let subscribe = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&subscribe).unwrap()["op"],
            "subscribe"
        );
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "ping"
        );
        socket.send(Message::Text("pong".into())).await.unwrap();
        seen.send(()).unwrap();
        match socket.next().await {
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {}
            other => panic!("unexpected: {other:?}"),
        }
    });
    let plan = validate_request(&candle_request(), &fixture()).unwrap();
    let (stop, cancel) = watch::channel(false);
    let (events, _received) = mpsc::channel(16);
    let worker = tokio::spawn(async move {
        run_connection(
            &format!("ws://{address}/ws/v5/business"),
            plan,
            None,
            cancel,
            Arc::new(|| true),
            events,
            fast_timings(),
            Arc::new(Mutex::new(WriteProgress::default())),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), seen_rx)
        .await
        .unwrap()
        .unwrap();
    stop.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap()
            .reason,
        "cancelled"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn invalidated_session_stops_before_delivering_a_private_message() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (subscribed_tx, subscribed_rx) = tokio::sync::oneshot::channel();
    let (continue_tx, continue_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket.next().await.unwrap().unwrap();
        subscribed_tx.send(()).unwrap();
        continue_rx.await.unwrap();
        let _ = socket
            .send(Message::Text(
                r#"{"private":"not-for-a-locked-session"}"#.into(),
            ))
            .await;
    });
    let valid = Arc::new(AtomicBool::new(true));
    let check = valid.clone();
    let plan = validate_request(&candle_request(), &fixture()).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let worker = tokio::spawn(async move {
        run_connection(
            &format!("ws://{address}/ws/v5/business"),
            plan,
            None,
            cancel,
            Arc::new(move || check.load(Ordering::SeqCst)),
            events,
            fast_timings(),
            Arc::new(Mutex::new(WriteProgress::default())),
        )
        .await
    });
    subscribed_rx.await.unwrap();
    valid.store(false, Ordering::SeqCst);
    continue_tx.send(()).unwrap();
    assert_eq!(worker.await.unwrap().reason, "session_changed");
    while let Ok(event) = received.try_recv() {
        assert_ne!(event.kind, "message");
    }
    server.await.unwrap();
}

#[tokio::test]
async fn invalidated_session_cancels_a_stalled_websocket_handshake() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted, accepted_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        accepted.send(()).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        drop(stream);
    });
    let valid = Arc::new(AtomicBool::new(true));
    let check = valid.clone();
    let plan = validate_request(&candle_request(), &fixture()).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let (events, _received) = mpsc::channel(16);
    let worker = tokio::spawn(async move {
        run_connection(
            &format!("ws://{address}/ws/v5/business"),
            plan,
            None,
            cancel,
            Arc::new(move || check.load(Ordering::SeqCst)),
            events,
            fast_timings(),
            Arc::new(Mutex::new(WriteProgress::default())),
        )
        .await
    });
    accepted_rx.await.unwrap();
    valid.store(false, Ordering::SeqCst);
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(300), worker)
            .await
            .unwrap()
            .unwrap()
            .reason,
        "session_changed"
    );
    server.abort();
}

#[tokio::test]
async fn sbe_authenticates_in_upgrade_and_delivers_binary_frame_as_base64() {
    use tokio::net::TcpListener;
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(stream, |request: &Request, response: Response| {
            let headers = request.headers();
            assert_eq!(headers["OK-ACCESS-KEY"], "demo-api-key");
            assert_eq!(headers["OK-ACCESS-PASSPHRASE"], "demo-passphrase");
            let timestamp = headers["OK-ACCESS-TIMESTAMP"].to_str().unwrap();
            assert_eq!(
                headers["OK-ACCESS-SIGN"],
                login_signature(timestamp, "demo-secret-key")
            );
            Ok(response)
        })
        .await
        .unwrap();
        let subscribe: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        assert_eq!(subscribe["op"], "subscribe");
        assert_eq!(subscribe["args"][0]["instIdCode"], 211874);
        socket
            .send(Message::Ping(b"12345".to_vec().into()))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Pong(b"12345".to_vec().into())
        );
        socket
            .send(Message::Binary(vec![0, 1, 254, 255].into()))
            .await
            .unwrap();
        let mut valid = Vec::new();
        for part in [42u16, 1002, 1, 0] {
            valid.extend(part.to_le_bytes());
        }
        for value in [211_874i64, 1000, 1001, 5, 4] {
            valid.extend(value.to_le_bytes());
        }
        valid.extend([(-2i8) as u8, (-3i8) as u8]);
        socket.send(Message::Binary(valid.into())).await.unwrap();
        socket.close(None).await.unwrap();
    });
    let mut catalogue = fixture();
    catalogue["websocket"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"sbe", "service":"public-sbe",
        "requires_login":true, "mutating":false, "operation":"subscribe", "channels":["trades"]}));
    let mut request = candle_request();
    request.endpoint_id = "sbe".into();
    request.credentials = WsCredential::Trade;
    request.args = vec![json!({"channel":"trades","instIdCode":211874})];
    let plan = validate_request(&request, &catalogue).unwrap();
    assert_eq!(
        endpoint_url(OkxEnvironment::Demo, "public-sbe").unwrap(),
        "wss://wspap.okx.com/ws/v5/public-sbe"
    );
    let (_stop, cancel) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let result = run_connection(
        &format!("ws://{address}/ws/v5/public-sbe"),
        plan,
        Some(fake_credentials()),
        cancel,
        Arc::new(|| true),
        events,
        fast_timings(),
        Arc::new(Mutex::new(WriteProgress::default())),
    )
    .await;
    server.await.unwrap();
    assert_eq!(result.status, "closed");
    let mut found = false;
    let mut decoded = false;
    while let Ok(event) = received.try_recv() {
        if event.kind == "message" && event.data["format"] == "sbe-base64" {
            if event.data["decoded"] == false {
                assert_eq!(event.data["data"], "AAH+/w==");
                found = true;
            } else if event.data["decoded"] == true {
                assert_eq!(
                    event.data["value"]["event"],
                    "BooksL2TbtExponentUpdateEvent"
                );
                assert_eq!(event.data["value"]["instIdCode"], "211874");
                assert!(event.data["data"].as_str().is_some());
                decoded = true;
            }
        }
    }
    assert!(found);
    assert!(decoded);
}

#[tokio::test]
async fn event_delivery_waits_for_ack_before_sending_the_next_frame() {
    let (event_tx, event_rx) = mpsc::channel(2);
    let (ack_tx, ack_rx) = mpsc::channel(2);
    let (stop, cancel) = watch::channel(false);
    let (output, mut received) = mpsc::unbounded_channel();
    let worker = tokio::spawn(pump_events(
        event_rx,
        ack_rx,
        cancel,
        Arc::new(move |event| output.send(event).is_ok()),
        Arc::new(Mutex::new(WriteProgress::default())),
    ));
    assert!(emit(&event_tx, "message", json!({"value":1})));
    assert!(emit(&event_tx, "message", json!({"value":2})));
    assert_eq!(received.recv().await.unwrap().sequence, 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), received.recv())
            .await
            .is_err()
    );
    ack_tx.send(1).await.unwrap();
    assert_eq!(received.recv().await.unwrap().sequence, 2);
    stop.send(true).unwrap();
    worker.await.unwrap();
}
