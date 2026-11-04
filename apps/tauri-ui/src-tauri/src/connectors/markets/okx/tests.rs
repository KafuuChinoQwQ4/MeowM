use super::model::{
    AccountTradeContext, InstrumentSpec, OrderDraft, OrderInstrumentType, OrderSide, OrderSnapshot,
    OrderType, PendingOrder, PositionAction, PositionMode, PositionSide, TradeMode, TradePosition,
};
use super::model::{OkxEnvironment, OrderAcknowledgement};
use super::*;
use crate::connectors::payments::{MAX_JSON_BYTES, agent};
use crate::connectors::{
    credential_store::{CredentialService, MemoryCredentialStore},
    model::{CredentialInput, SensitiveString},
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
};

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let mut header_bytes = Vec::new();
    let mut byte = [0u8];
    while !header_bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        header_bytes.push(byte[0]);
    }
    let header = String::from_utf8(header_bytes).unwrap();
    let content_length = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        })
        .unwrap_or_default();
    let mut body = vec![0; content_length];
    stream.read_exact(&mut body).unwrap();
    (header, body)
}

fn server(
    responses: Vec<(&'static str, serde_json::Value)>,
    authenticated: bool,
) -> (OkxClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for (path, value) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0u8];
            while !bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(
                request.starts_with(&format!("GET {path} HTTP/1.1\r\n")),
                "{request}"
            );
            if authenticated {
                let header = |name: &str| {
                    request
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case(name))
                                .map(|(_, v)| v.trim().to_string())
                        })
                        .unwrap()
                };
                assert_eq!(header("OK-ACCESS-KEY"), "test-key");
                assert_eq!(
                    header("OK-ACCESS-SIGN"),
                    hmac_base64(
                        b"secret",
                        format!("{}GET{path}", header("OK-ACCESS-TIMESTAMP")).as_bytes()
                    )
                    .unwrap()
                );
            } else {
                assert!(!request.to_ascii_lowercase().contains("ok-access-"));
            }
            let body = value.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    (OkxClient::for_test(agent(), endpoint), handle)
}

fn demo_server(
    responses: Vec<(&'static str, serde_json::Value)>,
) -> (OkxClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for (path, value) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut stream);
            assert!(headers.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            let timestamp = header_value(&headers, "OK-ACCESS-TIMESTAMP");
            assert_eq!(
                header_value(&headers, "OK-ACCESS-SIGN"),
                hmac_base64(b"secret", format!("{timestamp}GET{path}").as_bytes()).unwrap()
            );
            assert_eq!(header_value(&headers, "OK-ACCESS-KEY"), "test-key");
            assert_eq!(header_value(&headers, "x-simulated-trading"), "1");
            let body = value.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    (OkxClient::for_test(agent(), endpoint), handle)
}
fn credentials() -> OkxCredentials<'static> {
    OkxCredentials {
        api_key: "test-key",
        secret_key: "secret",
        passphrase: "test-passphrase",
    }
}

struct ServiceReply {
    request_line: &'static str,
    response: Option<serde_json::Value>,
    api_key: Option<&'static str>,
    secret: Option<&'static [u8]>,
    demo: Option<bool>,
    body: Option<serde_json::Value>,
}

fn service_server(replies: Vec<ServiceReply>) -> (OkxClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for expected in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut stream);
            assert!(headers.starts_with(expected.request_line), "{headers}");
            if let Some(api_key) = expected.api_key {
                let timestamp = header_value(&headers, "OK-ACCESS-TIMESTAMP");
                assert_eq!(header_value(&headers, "OK-ACCESS-KEY"), api_key);
                let method_and_path = expected
                    .request_line
                    .split_once(' ')
                    .map(|(_, rest)| rest.trim_end_matches(" HTTP/1.1\r\n"))
                    .unwrap();
                let method = expected.request_line.split_whitespace().next().unwrap();
                let mut prehash = format!("{timestamp}{method}{method_and_path}").into_bytes();
                prehash.extend_from_slice(&body);
                assert_eq!(
                    header_value(&headers, "OK-ACCESS-SIGN"),
                    hmac_base64(expected.secret.unwrap(), &prehash).unwrap()
                );
            } else {
                assert!(!headers.to_ascii_lowercase().contains("ok-access-"));
            }
            if let Some(demo) = expected.demo {
                assert_eq!(
                    headers
                        .to_ascii_lowercase()
                        .contains("x-simulated-trading: 1"),
                    demo
                );
            }
            if let Some(expected_body) = expected.body {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                    expected_body
                );
            }
            if let Some(response) = expected.response {
                let response = response.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .unwrap();
            }
        }
        listener.set_nonblocking(true).unwrap();
        assert!(matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    });
    (OkxClient::for_test(agent(), endpoint), handle)
}

fn service_credential(api_key: &str, secret_key: &str, passphrase: &str) -> CredentialInput {
    CredentialInput::OkxTradeDemo {
        api_key: SensitiveString::new(api_key.to_string()).unwrap(),
        secret_key: SensitiveString::new(secret_key.to_string()).unwrap(),
        passphrase: SensitiveString::new(passphrase.to_string()).unwrap(),
    }
}

fn test_order_credentials(include_demo: bool) -> Arc<CredentialService> {
    let service = Arc::new(CredentialService::new(Arc::new(
        MemoryCredentialStore::default(),
    )));
    service
        .save(CredentialInput::Okx {
            api_key: SensitiveString::new("read-key".into()).unwrap(),
            secret_key: SensitiveString::new("read-secret".into()).unwrap(),
            passphrase: SensitiveString::new("read-pass".into()).unwrap(),
        })
        .unwrap();
    service
        .save(CredentialInput::OkxTradeLive {
            api_key: SensitiveString::new("live-key".into()).unwrap(),
            secret_key: SensitiveString::new("live-secret".into()).unwrap(),
            passphrase: SensitiveString::new("live-pass".into()).unwrap(),
        })
        .unwrap();
    if include_demo {
        service
            .save(service_credential("demo-key", "demo-secret", "demo-pass"))
            .unwrap();
    }
    service
}

fn demo_spot_place_replies(
    post_response: Option<serde_json::Value>,
    post_body: serde_json::Value,
) -> Vec<ServiceReply> {
    vec![
        ServiceReply {
            request_line: "GET /api/v5/public/instruments?instType=SPOT&instId=BTC-USDT HTTP/1.1\r\n",
            response: Some(serde_json::json!({"code":"0","data":[{
                "instType":"SPOT","instId":"BTC-USDT","state":"live",
                "tickSz":"0.1","lotSz":"0.0001","minSz":"0.0005",
                "baseCcy":"BTC","quoteCcy":"USDT"
            }]})),
            api_key: None,
            secret: None,
            demo: None,
            body: None,
        },
        ServiceReply {
            request_line: "GET /api/v5/account/config HTTP/1.1\r\n",
            response: Some(
                serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
            ),
            api_key: Some("demo-key"),
            secret: Some(b"demo-secret"),
            demo: Some(true),
            body: None,
        },
        ServiceReply {
            request_line: "POST /api/v5/trade/order HTTP/1.1\r\n",
            response: post_response,
            api_key: Some("demo-key"),
            secret: Some(b"demo-secret"),
            demo: Some(true),
            body: Some(post_body),
        },
    ]
}

#[test]
fn order_service_uses_only_demo_credentials_and_posts_once_after_fresh_validation() {
    let order_body = serde_json::json!({
        "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
        "side":"buy", "ordType":"market", "sz":"0.001",
        "tgtCcy":"base_ccy", "slippagePct":"0.01"
    });
    let replies = demo_spot_place_replies(
        Some(serde_json::json!({"code":"0","data":[{
            "ordId":"123456","clOrdId":"client123","sCode":"0","sMsg":""
        }]})),
        order_body,
    );
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));

    let result = order_service
        .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0.001"))
        .unwrap();
    assert_eq!(
        result.status,
        super::orders::OrderSubmissionStatus::Accepted
    );
    let acknowledgement = result.acknowledgement.unwrap();
    assert_eq!(acknowledgement.order_id, "123456");
    assert_eq!(acknowledgement.client_order_id, "client123");
    // Acceptance is not reported as an order fill.
    assert_eq!(
        result.status,
        super::orders::OrderSubmissionStatus::Accepted
    );
    server.join().unwrap();
}

#[test]
fn production_order_uses_only_live_credentials_and_omits_demo_header() {
    let order_body = serde_json::json!({
        "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
        "side":"sell", "ordType":"limit", "sz":"0.001", "px":"50000"
    });
    let mut replies = demo_spot_place_replies(
        Some(serde_json::json!({"code":"0","data":[{
            "ordId":"123457","clOrdId":"client123","sCode":"0","sMsg":""
        }]})),
        order_body,
    );
    for reply in &mut replies {
        if reply.api_key.is_some() {
            reply.api_key = Some("live-key");
            reply.secret = Some(b"live-secret");
            reply.demo = Some(false);
        }
    }
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let mut draft = spot_draft(OrderSide::Sell, OrderType::Limit, "0.001");
    draft.environment = OkxEnvironment::Production;
    draft.price = Some("50000".into());
    let result = order_service.place_order(draft).unwrap();
    assert_eq!(
        result.status,
        super::orders::OrderSubmissionStatus::Accepted
    );
    assert_eq!(result.acknowledgement.unwrap().order_id, "123457");
    server.join().unwrap();
}

#[test]
fn demo_write_does_not_fall_back_to_read_only_or_live_credentials() {
    let (client, server) = service_server(Vec::new());
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(false));
    assert_eq!(
        order_service
            .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0.001"))
            .unwrap_err(),
        ProviderErrorCode::InvalidCredentials
    );
    server.join().unwrap();
}

#[test]
fn invalid_size_is_rejected_before_any_order_post() {
    let (client, server) = service_server(Vec::new());
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    assert_eq!(
        order_service
            .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0"))
            .unwrap_err(),
        ProviderErrorCode::InvalidRequest
    );
    server.join().unwrap();
}

#[test]
fn order_service_rejects_client_ids_outside_okx_alphanumeric_format_before_network_use() {
    let (client, server) = service_server(Vec::new());
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let mut draft = spot_draft(OrderSide::Buy, OrderType::Market, "0.001");
    draft.cl_ord_id = "client-id_1".into();
    assert_eq!(
        order_service.place_order(draft).unwrap_err(),
        ProviderErrorCode::InvalidRequest
    );
    server.join().unwrap();
}

#[test]
fn per_order_exchange_error_is_returned_as_rejected_with_code_and_message() {
    let order_body = serde_json::json!({
        "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
        "side":"buy", "ordType":"market", "sz":"0.001",
        "tgtCcy":"base_ccy", "slippagePct":"0.01"
    });
    let replies = demo_spot_place_replies(
        Some(serde_json::json!({"code":"0","data":[{
            "ordId":"","clOrdId":"client123","sCode":"51008","sMsg":"Insufficient balance"
        }]})),
        order_body,
    );
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let result = order_service
        .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0.001"))
        .unwrap();
    assert_eq!(
        result.status,
        super::orders::OrderSubmissionStatus::Rejected
    );
    assert_eq!(result.error_code, None);
    let acknowledgement = result.acknowledgement.unwrap();
    assert_eq!(acknowledgement.code, "51008");
    assert_eq!(acknowledgement.message, "Insufficient balance");
    server.join().unwrap();
}

#[test]
fn unclear_submit_keeps_client_id_for_order_detail_reconciliation() {
    let mut replies = demo_spot_place_replies(
        None,
        serde_json::json!({
            "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
            "side":"buy", "ordType":"market", "sz":"0.001",
            "tgtCcy":"base_ccy", "slippagePct":"0.01"
        }),
    );
    replies.push(ServiceReply {
        request_line: "GET /api/v5/trade/order?instId=BTC-USDT&clOrdId=client123 HTTP/1.1\r\n",
        response: Some(serde_json::json!({"code":"0","data":[{
            "instId":"BTC-USDT","ordId":"123456","clOrdId":"client123",
            "instType":"SPOT","side":"buy","ordType":"market","tdMode":"cash",
            "sz":"0.001","px":"","avgPx":"50000","accFillSz":"0.001",
            "state":"filled","posSide":"net","reduceOnly":"false","tgtCcy":"base_ccy"
        }]})),
        api_key: Some("demo-key"),
        secret: Some(b"demo-secret"),
        demo: Some(true),
        body: None,
    });
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let submit = order_service
        .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0.001"))
        .unwrap();
    assert_eq!(submit.status, super::orders::OrderSubmissionStatus::Unknown);
    assert_eq!(submit.client_order_id, "client123");
    let detail = order_service
        .order_detail("BTC-USDT", &submit.client_order_id, OkxEnvironment::Demo)
        .unwrap()
        .unwrap();
    assert_eq!(detail.client_order_id, submit.client_order_id);
    assert_eq!(detail.state, "filled");
    server.join().unwrap();
}

#[test]
fn ambiguous_exchange_server_error_keeps_client_id_and_is_reconciled_without_resubmit() {
    let mut replies = demo_spot_place_replies(
        Some(serde_json::json!({"code":"50000","msg":"exchange internal error"})),
        serde_json::json!({
            "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
            "side":"buy", "ordType":"market", "sz":"0.001",
            "tgtCcy":"base_ccy", "slippagePct":"0.01"
        }),
    );
    replies.push(ServiceReply {
        request_line: "GET /api/v5/trade/order?instId=BTC-USDT&clOrdId=client123 HTTP/1.1\r\n",
        response: Some(serde_json::json!({"code":"0","data":[{
            "instId":"BTC-USDT","ordId":"123456","clOrdId":"client123",
            "instType":"SPOT","side":"buy","ordType":"market","tdMode":"cash",
            "sz":"0.001","px":"","avgPx":"50000","accFillSz":"0.001",
            "state":"filled","posSide":"net","reduceOnly":"false","tgtCcy":"base_ccy"
        }]})),
        api_key: Some("demo-key"),
        secret: Some(b"demo-secret"),
        demo: Some(true),
        body: None,
    });
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let submit = order_service
        .place_order(spot_draft(OrderSide::Buy, OrderType::Market, "0.001"))
        .unwrap();
    assert_eq!(submit.status, super::orders::OrderSubmissionStatus::Unknown);
    assert_eq!(submit.error_code, Some(ProviderErrorCode::ProviderFailure));
    assert_eq!(submit.client_order_id, "client123");
    let detail = order_service
        .order_detail("BTC-USDT", &submit.client_order_id, OkxEnvironment::Demo)
        .unwrap()
        .unwrap();
    assert_eq!(detail.state, "filled");
    server.join().unwrap();
}

#[test]
fn order_history_and_single_order_cancel_use_fixed_paths_and_explicit_ids() {
    let replies = vec![
        ServiceReply {
            request_line: "GET /api/v5/trade/orders-history?instType=SPOT&limit=100 HTTP/1.1\r\n",
            response: Some(serde_json::json!({"code":"0","data":[{
                "instId":"BTC-USDT","ordId":"123456","clOrdId":"client123",
                "instType":"SPOT","side":"buy","ordType":"limit","tdMode":"cash",
                "sz":"0.001","px":"50000","avgPx":"","accFillSz":"0",
                "state":"canceled","posSide":"net","reduceOnly":"false","tgtCcy":""
            }]})),
            api_key: Some("demo-key"),
            secret: Some(b"demo-secret"),
            demo: Some(true),
            body: None,
        },
        ServiceReply {
            request_line: "POST /api/v5/trade/cancel-order HTTP/1.1\r\n",
            response: Some(serde_json::json!({"code":"0","data":[{
                "instId":"BTC-USDT","ordId":"123456","clOrdId":"",
                "sCode":"0","sMsg":""
            }]})),
            api_key: Some("demo-key"),
            secret: Some(b"demo-secret"),
            demo: Some(true),
            body: Some(serde_json::json!({"instId":"BTC-USDT","ordId":"123456"})),
        },
    ];
    let (client, server) = service_server(replies);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    let history = order_service
        .order_history(OrderInstrumentType::Spot, OkxEnvironment::Demo)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].state, "canceled");
    let cancel = order_service
        .cancel_order("BTC-USDT", "123456", OkxEnvironment::Demo)
        .unwrap();
    assert_eq!(
        cancel.status,
        super::orders::OrderSubmissionStatus::Accepted
    );
    assert_eq!(cancel.order_id, "123456");
    server.join().unwrap();
}

#[test]
fn order_history_rejects_rows_that_do_not_match_the_requested_instrument_type() {
    let (client, server) = service_server(vec![ServiceReply {
        request_line: "GET /api/v5/trade/orders-history?instType=SPOT&limit=100 HTTP/1.1\r\n",
        response: Some(serde_json::json!({"code":"0","data":[{
            "instId":"BTC-USDT","ordId":"123456","clOrdId":"client123",
            "instType":"SWAP","side":"buy","ordType":"limit","tdMode":"cash",
            "sz":"0.001","px":"50000","avgPx":"","accFillSz":"0",
            "state":"canceled","posSide":"net","reduceOnly":"false","tgtCcy":""
        }]})),
        api_key: Some("demo-key"),
        secret: Some(b"demo-secret"),
        demo: Some(true),
        body: None,
    }]);
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    assert_eq!(
        order_service
            .order_history(OrderInstrumentType::Spot, OkxEnvironment::Demo)
            .unwrap_err(),
        ProviderErrorCode::InvalidResponse
    );
    server.join().unwrap();
}

#[test]
fn malicious_detail_and_cancel_identifiers_fail_before_network_use() {
    let (client, server) = service_server(Vec::new());
    let order_service = super::orders::OkxOrderService::new(client, test_order_credentials(true));
    assert_eq!(
        order_service
            .order_detail("BTC-USDT/../trade", "123", OkxEnvironment::Demo)
            .unwrap_err(),
        ProviderErrorCode::InvalidRequest
    );
    assert_eq!(
        order_service
            .order_detail("BTC-USDT", "abc&instType=FUTURES", OkxEnvironment::Demo)
            .unwrap_err(),
        ProviderErrorCode::InvalidRequest
    );
    assert_eq!(
        order_service
            .cancel_order("BTC-USDT", "123/../cancel-all", OkxEnvironment::Demo)
            .unwrap_err(),
        ProviderErrorCode::InvalidRequest
    );
    server.join().unwrap();
}

fn pending_order_row(ord_id: &str) -> serde_json::Value {
    serde_json::json!({
        "ordId":ord_id, "state":"live", "instId":"BTC-USDT-SWAP", "side":"sell", "posSide":"net",
        "tdMode":"cross", "reduceOnly":"true", "sz":"1", "accFillSz":"0"
    })
}

#[test]
fn pending_order_parser_rejects_missing_or_unsafe_exchange_order_ids() {
    let mut missing = pending_order_row("123");
    missing.as_object_mut().unwrap().remove("ordId");
    for row in [
        missing,
        pending_order_row(""),
        pending_order_row("123x"),
        pending_order_row("../cancel-all"),
    ] {
        assert_eq!(
            super::order_validation::parse_pending_order(&row),
            Err(ProviderErrorCode::InvalidResponse)
        );
    }
}

#[test]
fn pending_order_parser_accepts_only_live_or_partially_filled_state() {
    let mut row = pending_order_row("123");
    assert_eq!(
        super::order_validation::parse_pending_order(&row)
            .unwrap()
            .state,
        "live"
    );
    row["state"] = serde_json::json!("partially_filled");
    assert_eq!(
        super::order_validation::parse_pending_order(&row)
            .unwrap()
            .state,
        "partially_filled"
    );
    for state in ["filled", "canceled", "mystery"] {
        row["state"] = serde_json::json!(state);
        assert_eq!(
            super::order_validation::parse_pending_order(&row),
            Err(ProviderErrorCode::InvalidResponse)
        );
    }
    row.as_object_mut().unwrap().remove("state");
    assert_eq!(
        super::order_validation::parse_pending_order(&row),
        Err(ProviderErrorCode::InvalidResponse)
    );
}

fn pending_cap_responses(
    probe_rows: Vec<serde_json::Value>,
    include_leverage: bool,
) -> Vec<(&'static str, serde_json::Value)> {
    let mut responses = vec![
        (
            "/api/v5/account/config",
            serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
        ),
        (
            "/api/v5/account/positions?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[]}),
        ),
    ];
    for (start, path) in [
        (
            1500,
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100",
        ),
        (
            1400,
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1401",
        ),
        (
            1300,
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1301",
        ),
        (
            1200,
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1201",
        ),
        (
            1100,
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1101",
        ),
    ] {
        let rows = (0..100)
            .map(|index| pending_order_row(&(start - index).to_string()))
            .collect::<Vec<_>>();
        responses.push((path, serde_json::json!({"code":"0","data":rows})));
    }
    responses.push((
        "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1001",
        serde_json::json!({"code":"0","data":probe_rows}),
    ));
    if include_leverage {
        responses.extend([
            (
                "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
                serde_json::json!({"code":"0","data":[]}),
            ),
            (
                "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=isolated",
                serde_json::json!({"code":"0","data":[]}),
            ),
        ]);
    }
    responses
}

fn spot_spec() -> InstrumentSpec {
    InstrumentSpec {
        inst_type: OrderInstrumentType::Spot,
        inst_id: "BTC-USDT".into(),
        state: "live".into(),
        tick_sz: "0.1".into(),
        lot_sz: "0.0001".into(),
        min_sz: "0.0005".into(),
        ct_val: None,
        ct_val_ccy: None,
        ct_type: None,
        settle_ccy: None,
        base_ccy: Some("BTC".into()),
        quote_ccy: Some("USDT".into()),
    }
}

fn swap_spec() -> InstrumentSpec {
    InstrumentSpec {
        inst_type: OrderInstrumentType::Swap,
        inst_id: "BTC-USDT-SWAP".into(),
        state: "live".into(),
        tick_sz: "0.1".into(),
        lot_sz: "1".into(),
        min_sz: "1".into(),
        ct_val: Some("0.01".into()),
        ct_val_ccy: Some("BTC".into()),
        ct_type: Some("linear".into()),
        settle_ccy: Some("USDT".into()),
        base_ccy: Some("BTC".into()),
        quote_ccy: Some("USDT".into()),
    }
}

fn futures_spec() -> InstrumentSpec {
    InstrumentSpec {
        inst_type: OrderInstrumentType::Futures,
        inst_id: "BTC-USDT-241227".into(),
        state: "live".into(),
        tick_sz: "0.1".into(),
        lot_sz: "1".into(),
        min_sz: "1".into(),
        ct_val: Some("0.01".into()),
        ct_val_ccy: Some("BTC".into()),
        ct_type: Some("linear".into()),
        settle_ccy: Some("USDT".into()),
        base_ccy: None,
        quote_ccy: None,
    }
}

fn trade_context() -> AccountTradeContext {
    AccountTradeContext {
        acct_lv: "2".into(),
        pos_mode: PositionMode::Net,
        positions: Vec::new(),
        pending_orders: Vec::new(),
        leverage: Vec::new(),
    }
}

fn spot_draft(side: OrderSide, ord_type: OrderType, size: &str) -> OrderDraft {
    OrderDraft {
        environment: OkxEnvironment::Demo,
        inst_type: OrderInstrumentType::Spot,
        inst_id: "BTC-USDT".into(),
        side,
        ord_type,
        size: size.into(),
        price: None,
        slippage_pct: None,
        td_mode: Some(TradeMode::Cash),
        position_action: None,
        cl_ord_id: "client123".into(),
    }
}

fn swap_draft(action: PositionAction, size: &str) -> OrderDraft {
    let side = match action {
        PositionAction::OpenLong | PositionAction::CloseShort => OrderSide::Buy,
        PositionAction::OpenShort | PositionAction::CloseLong => OrderSide::Sell,
    };
    OrderDraft {
        environment: OkxEnvironment::Demo,
        inst_type: OrderInstrumentType::Swap,
        inst_id: "BTC-USDT-SWAP".into(),
        side,
        ord_type: OrderType::Market,
        size: size.into(),
        price: None,
        slippage_pct: None,
        td_mode: Some(TradeMode::Isolated),
        position_action: Some(action),
        cl_ord_id: "client123".into(),
    }
}

fn build_order(
    draft: OrderDraft,
    spec: &InstrumentSpec,
    context: &AccountTradeContext,
) -> Result<super::order_validation::ValidatedOrder, ProviderErrorCode> {
    super::order_validation::ValidatedOrder::from_draft(draft, spec, context)
}

#[test]
fn spot_market_orders_use_the_received_currency_and_one_percent_default_slippage() {
    let context = trade_context();
    let buy = build_order(
        spot_draft(OrderSide::Buy, OrderType::Market, "0.001"),
        &spot_spec(),
        &context,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(buy.body()).unwrap(),
        serde_json::json!({
            "instId":"BTC-USDT", "tdMode":"cash", "clOrdId":"client123",
            "side":"buy", "ordType":"market", "sz":"0.001",
            "tgtCcy":"base_ccy", "slippagePct":"0.01"
        })
    );

    let sell = build_order(
        spot_draft(OrderSide::Sell, OrderType::Market, "100.01"),
        &spot_spec(),
        &context,
    )
    .unwrap();
    assert_eq!(sell.body().tgt_ccy(), Some("quote_ccy"));
    assert_eq!(sell.body().slippage_pct(), Some("0.01"));
}

#[test]
fn spot_limit_order_requires_a_tick_aligned_price_and_uses_base_size() {
    let mut draft = spot_draft(OrderSide::Buy, OrderType::Limit, "0.001");
    draft.price = Some("100.2".into());
    let order = build_order(draft, &spot_spec(), &trade_context()).unwrap();
    let body = serde_json::to_value(order.body()).unwrap();
    assert_eq!(body["sz"], "0.001");
    assert_eq!(body["px"], "100.2");
    assert!(body.get("tgtCcy").is_none());
    assert!(body.get("slippagePct").is_none());
}

#[test]
fn net_mode_closes_are_reduce_only_and_cannot_exceed_remaining_position() {
    let mut context = trade_context();
    context.positions.push(TradePosition {
        inst_id: "BTC-USDT-SWAP".into(),
        pos_side: PositionSide::Net,
        quantity: "3".into(),
        margin_mode: TradeMode::Isolated,
    });
    context.pending_orders.push(PendingOrder {
        ord_id: "200".into(),
        state: "live".into(),
        inst_id: "BTC-USDT-SWAP".into(),
        side: OrderSide::Sell,
        pos_side: Some(PositionSide::Net),
        margin_mode: TradeMode::Isolated,
        reduce_only: true,
        remaining_size: "1".into(),
    });
    let close = build_order(
        swap_draft(PositionAction::CloseLong, "2"),
        &swap_spec(),
        &context,
    )
    .unwrap();
    assert_eq!(close.body().side(), OrderSide::Sell);
    assert_eq!(close.body().reduce_only(), Some(true));
    assert!(
        build_order(
            swap_draft(PositionAction::CloseLong, "3"),
            &swap_spec(),
            &context
        )
        .is_err()
    );
}

#[test]
fn hedge_mode_maps_open_and_close_actions_to_side_and_position_side() {
    let context = AccountTradeContext {
        pos_mode: PositionMode::LongShort,
        ..trade_context()
    };
    let cases = [
        (
            PositionAction::OpenLong,
            OrderSide::Buy,
            Some(PositionSide::Long),
        ),
        (
            PositionAction::OpenShort,
            OrderSide::Sell,
            Some(PositionSide::Short),
        ),
        (
            PositionAction::CloseLong,
            OrderSide::Sell,
            Some(PositionSide::Long),
        ),
        (
            PositionAction::CloseShort,
            OrderSide::Buy,
            Some(PositionSide::Short),
        ),
    ];
    for (action, side, pos_side) in cases {
        let mut context = context.clone();
        if matches!(
            action,
            PositionAction::CloseLong | PositionAction::CloseShort
        ) {
            context.positions.push(TradePosition {
                inst_id: "BTC-USDT-SWAP".into(),
                pos_side: pos_side.unwrap(),
                quantity: "2".into(),
                margin_mode: TradeMode::Isolated,
            });
        }
        let order = build_order(swap_draft(action, "1"), &swap_spec(), &context).unwrap();
        assert_eq!(order.body().side(), side);
        assert_eq!(order.body().pos_side(), pos_side);
        assert_eq!(order.body().reduce_only(), None);
    }
}

#[test]
fn futures_contract_order_uses_contract_lot_size() {
    let mut draft = swap_draft(PositionAction::OpenShort, "2");
    draft.inst_type = OrderInstrumentType::Futures;
    draft.inst_id = "BTC-USDT-241227".into();
    let order = build_order(draft, &futures_spec(), &trade_context()).unwrap();
    assert_eq!(order.body().side(), OrderSide::Sell);
    assert_eq!(serde_json::to_value(order.body()).unwrap()["sz"], "2");
}

#[test]
fn order_draft_rejects_unknown_fields() {
    let value = serde_json::json!({
        "environment":"demo", "inst_type":"SPOT", "inst_id":"BTC-USDT",
        "side":"buy", "ord_type":"market", "size":"0.001",
        "cl_ord_id":"client123", "leverage":"100"
    });
    assert!(serde_json::from_value::<OrderDraft>(value).is_err());
}

#[test]
fn order_validation_rejects_invalid_type_state_precision_size_price_and_slippage() {
    let context = trade_context();
    let mut cases = Vec::new();

    let mut wrong_type = spot_spec();
    wrong_type.inst_type = OrderInstrumentType::Swap;
    cases.push((
        "wrong instrument type",
        spot_draft(OrderSide::Buy, OrderType::Market, "0.001"),
        wrong_type,
    ));

    let mut suspended = spot_spec();
    suspended.state = "suspend".into();
    cases.push((
        "suspended instrument",
        spot_draft(OrderSide::Buy, OrderType::Market, "0.001"),
        suspended,
    ));

    let post_only = swap_draft(PositionAction::OpenLong, "1");
    let mut post_only_spec = swap_spec();
    post_only_spec.state = "post_only".into();
    cases.push(("post-only instrument", post_only, post_only_spec));

    let mut off_tick = spot_draft(OrderSide::Buy, OrderType::Limit, "0.001");
    off_tick.price = Some("100.25".into());
    cases.push(("off-tick price", off_tick, spot_spec()));

    cases.push((
        "off-lot size",
        spot_draft(OrderSide::Buy, OrderType::Market, "0.00101"),
        spot_spec(),
    ));
    cases.push((
        "below minimum",
        spot_draft(OrderSide::Buy, OrderType::Market, "0.0004"),
        spot_spec(),
    ));
    cases.push((
        "contract off-lot",
        swap_draft(PositionAction::OpenLong, "1.5"),
        swap_spec(),
    ));

    let missing_limit_price = spot_draft(OrderSide::Buy, OrderType::Limit, "0.001");
    cases.push(("missing limit price", missing_limit_price, spot_spec()));

    let mut invalid_slippage = spot_draft(OrderSide::Buy, OrderType::Market, "0.001");
    invalid_slippage.slippage_pct = Some("0.01234".into());
    cases.push(("slippage precision", invalid_slippage, spot_spec()));

    let mut negative_zero_slippage = spot_draft(OrderSide::Buy, OrderType::Market, "0.001");
    negative_zero_slippage.slippage_pct = Some("-0".into());
    cases.push((
        "negative zero slippage",
        negative_zero_slippage,
        spot_spec(),
    ));

    let mut excessive_slippage = spot_draft(OrderSide::Buy, OrderType::Market, "0.001");
    excessive_slippage.slippage_pct = Some("0.0501".into());
    cases.push((
        "slippage above five percent",
        excessive_slippage,
        spot_spec(),
    ));

    for (name, draft, spec) in cases {
        assert!(
            build_order(draft, &spec, &context).is_err(),
            "accepted {name}"
        );
    }
}

#[test]
fn public_instrument_reader_returns_validated_contract_rules_without_credentials() {
    let (client, handle) = server(
        vec![(
            "/api/v5/public/instruments?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[{
                "instType":"SWAP","instId":"BTC-USDT-SWAP","state":"live",
                "tickSz":"0.1","lotSz":"1","minSz":"1","ctVal":"0.01",
                "ctValCcy":"BTC","ctType":"linear","settleCcy":"USDT","baseCcy":"BTC","quoteCcy":"USDT"
            }]}),
        )],
        false,
    );
    let spec = client
        .read_instrument_spec(OrderInstrumentType::Swap, "BTC-USDT-SWAP")
        .unwrap();
    assert_eq!(spec.inst_id, "BTC-USDT-SWAP");
    assert_eq!(spec.ct_val.as_deref(), Some("0.01"));
    assert_eq!(spec.lot_sz, "1");
    handle.join().unwrap();
}

#[test]
fn demo_trade_context_reads_account_mode_positions_pending_closes_and_leverage() {
    let (client, handle) = demo_server(vec![
        (
            "/api/v5/account/config",
            serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
        ),
        (
            "/api/v5/account/positions?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","pos":"2","posSide":"net","mgnMode":"cross"}]}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100",
            serde_json::json!({"code":"0","data":[{"ordId":"200","state":"partially_filled","instId":"BTC-USDT-SWAP","side":"sell","posSide":"net","tdMode":"cross","reduceOnly":"true","sz":"3","accFillSz":"1.5"}]}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","mgnMode":"cross","posSide":"net","lever":"5"}]}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=isolated",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","mgnMode":"isolated","posSide":"net","lever":"3"}]}),
        ),
    ]);
    let context = client
        .read_trade_context(
            &credentials(),
            OrderInstrumentType::Swap,
            "BTC-USDT-SWAP",
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert_eq!(context.acct_lv, "2");
    assert_eq!(context.pos_mode, PositionMode::Net);
    assert_eq!(context.positions[0].quantity, "2");
    assert_eq!(context.pending_orders[0].ord_id, "200");
    assert_eq!(context.pending_orders[0].state, "partially_filled");
    assert_eq!(context.pending_orders[0].remaining_size, "1.5");
    assert_eq!(context.leverage.len(), 2);
    assert_eq!(context.leverage[0].pos_side, Some(PositionSide::Net));
    let serialized = serde_json::to_value(&context).unwrap();
    assert_eq!(serialized["pos_mode"], "net_mode");
    assert_eq!(serialized["leverage"][0]["pos_side"], "net");
    handle.join().unwrap();
}

#[test]
fn spot_trade_context_skips_contract_pending_size_accounting() {
    let (client, handle) = demo_server(vec![(
        "/api/v5/account/config",
        serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
    )]);
    let context = client
        .read_trade_context(
            &credentials(),
            OrderInstrumentType::Spot,
            "BTC-USDT",
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert!(context.pending_orders.is_empty());
    handle.join().unwrap();
}

#[test]
fn unavailable_optional_cross_leverage_does_not_block_trade_context() {
    let (client, handle) = demo_server(vec![
        (
            "/api/v5/account/config",
            serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
        ),
        (
            "/api/v5/account/positions?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[]}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100",
            serde_json::json!({"code":"0","data":[]}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
            serde_json::json!({"code":"50000","msg":"unavailable under portfolio margin"}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=isolated",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","mgnMode":"isolated","posSide":"net","lever":"3"}]}),
        ),
    ]);
    let context = client
        .read_trade_context(
            &credentials(),
            OrderInstrumentType::Swap,
            "BTC-USDT-SWAP",
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert_eq!(context.leverage.len(), 1);
    assert_eq!(context.leverage[0].margin_mode, TradeMode::Isolated);
    handle.join().unwrap();
}

#[test]
fn pending_orders_are_paginated_through_all_120_rows_for_close_validation() {
    let first_page = (0..100)
        .map(|index| pending_order_row(&(1099 - index).to_string()))
        .collect::<Vec<_>>();
    let second_page = (0..20)
        .map(|index| pending_order_row(&(999 - index).to_string()))
        .collect::<Vec<_>>();
    let (client, handle) = demo_server(vec![
        (
            "/api/v5/account/config",
            serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
        ),
        (
            "/api/v5/account/positions?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","pos":"200","posSide":"net","mgnMode":"cross"}]}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100",
            serde_json::json!({"code":"0","data":first_page}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=1000",
            serde_json::json!({"code":"0","data":second_page}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=cross",
            serde_json::json!({"code":"0","data":[]}),
        ),
        (
            "/api/v5/account/leverage-info?instId=BTC-USDT-SWAP&mgnMode=isolated",
            serde_json::json!({"code":"0","data":[]}),
        ),
    ]);
    let context = client
        .read_trade_context(
            &credentials(),
            OrderInstrumentType::Swap,
            "BTC-USDT-SWAP",
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert_eq!(context.pending_orders.len(), 120);
    let mut boundary_close = swap_draft(PositionAction::CloseLong, "80");
    boundary_close.td_mode = Some(TradeMode::Cross);
    assert!(build_order(boundary_close, &swap_spec(), &context).is_ok());
    let mut close = swap_draft(PositionAction::CloseLong, "81");
    close.td_mode = Some(TradeMode::Cross);
    assert!(build_order(close, &swap_spec(), &context).is_err());
    handle.join().unwrap();
}

#[test]
fn pending_order_pagination_rejects_a_repeated_cursor_page() {
    let first_page = (0..100)
        .map(|index| pending_order_row(&(1000 - index).to_string()))
        .collect::<Vec<_>>();
    let (client, handle) = demo_server(vec![
        (
            "/api/v5/account/config",
            serde_json::json!({"code":"0","data":[{"acctLv":"2","posMode":"net_mode"}]}),
        ),
        (
            "/api/v5/account/positions?instType=SWAP&instId=BTC-USDT-SWAP",
            serde_json::json!({"code":"0","data":[]}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100",
            serde_json::json!({"code":"0","data":first_page}),
        ),
        (
            "/api/v5/trade/orders-pending?instType=SWAP&instId=BTC-USDT-SWAP&limit=100&after=901",
            serde_json::json!({"code":"0","data":[pending_order_row("901")]}),
        ),
    ]);
    let result = client.read_trade_context(
        &credentials(),
        OrderInstrumentType::Swap,
        "BTC-USDT-SWAP",
        OkxEnvironment::Demo,
    );
    assert_eq!(result.unwrap_err(), ProviderErrorCode::InvalidResponse);
    handle.join().unwrap();
}

#[test]
fn exactly_500_pending_orders_require_an_empty_probe_and_are_complete() {
    let (client, handle) = demo_server(pending_cap_responses(Vec::new(), true));
    let context = client
        .read_trade_context(
            &credentials(),
            OrderInstrumentType::Swap,
            "BTC-USDT-SWAP",
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert_eq!(context.pending_orders.len(), 500);
    handle.join().unwrap();
}

#[test]
fn pending_order_beyond_500_cap_fails_closed_after_probe() {
    let (client, handle) = demo_server(pending_cap_responses(
        vec![pending_order_row("1000")],
        false,
    ));
    let result = client.read_trade_context(
        &credentials(),
        OrderInstrumentType::Swap,
        "BTC-USDT-SWAP",
        OkxEnvironment::Demo,
    );
    assert_eq!(result.unwrap_err(), ProviderErrorCode::InvalidResponse);
    handle.join().unwrap();
}

#[derive(serde::Serialize)]
struct TestOrder {
    #[serde(rename = "instId")]
    inst_id: &'static str,
    #[serde(rename = "tdMode")]
    trade_mode: &'static str,
    #[serde(rename = "clOrdId")]
    client_order_id: &'static str,
    side: &'static str,
    #[serde(rename = "ordType")]
    order_type: &'static str,
    sz: &'static str,
}

fn post_server(responses: Vec<Vec<u8>>) -> (OkxClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, _) = read_request(&mut stream);
            assert!(headers.starts_with("POST /api/v5/trade/order HTTP/1.1\r\n"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (OkxClient::for_test(agent(), endpoint), handle)
}

#[test]
fn signed_order_post_uses_exact_body_and_demo_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let order = TestOrder {
        inst_id: "BTC-USDT",
        trade_mode: "cash",
        client_order_id: "client123",
        side: "buy",
        order_type: "market",
        sz: "100.00",
    };
    let expected_body = serde_json::to_vec(&order).unwrap();
    let server = std::thread::spawn(move || {
        for (environment, response_body) in [
            (
                OkxEnvironment::Demo,
                r#"{"code":"0","data":[{"ordId":"42","clOrdId":"client123","sCode":"0","sMsg":""}],"msg":""}"#,
            ),
            (
                OkxEnvironment::Production,
                r#"{"code":"0","data":[{"ordId":"43","clOrdId":"client123","sCode":"51008","sMsg":"Order rejected"}],"msg":""}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut stream);
            assert!(headers.starts_with("POST /api/v5/trade/order HTTP/1.1\r\n"));
            assert!(
                headers
                    .lines()
                    .any(|line| { line.eq_ignore_ascii_case("content-type: application/json") })
            );
            assert_eq!(body, expected_body);
            let timestamp = header_value(&headers, "OK-ACCESS-TIMESTAMP");
            let mut prehash = format!("{timestamp}POST/api/v5/trade/order").into_bytes();
            prehash.extend_from_slice(&body);
            assert_eq!(
                header_value(&headers, "OK-ACCESS-SIGN"),
                hmac_base64(b"secret", &prehash).unwrap()
            );
            assert_eq!(header_value(&headers, "OK-ACCESS-KEY"), "test-key");
            match environment {
                OkxEnvironment::Demo => {
                    assert_eq!(header_value(&headers, "x-simulated-trading"), "1")
                }
                OkxEnvironment::Production => {
                    assert!(
                        !headers
                            .to_ascii_lowercase()
                            .contains("x-simulated-trading:")
                    )
                }
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
        }
    });
    let client = OkxClient::for_test(agent(), endpoint);
    let demo_ack: Vec<OrderAcknowledgement> = client
        .post(
            "/api/v5/trade/order",
            &order,
            &credentials(),
            OkxEnvironment::Demo,
        )
        .unwrap();
    assert_eq!(demo_ack[0].order_id, "42");
    assert_eq!(demo_ack[0].client_order_id, "client123");
    assert_eq!(demo_ack[0].code, "0");
    assert_eq!(demo_ack[0].message, "");

    let live_ack: Vec<OrderAcknowledgement> = client
        .post(
            "/api/v5/trade/order",
            &order,
            &credentials(),
            OkxEnvironment::Production,
        )
        .unwrap();
    assert_eq!(live_ack[0].code, "51008");
    assert_eq!(live_ack[0].message, "Order rejected");
    // A successful request envelope is only an acknowledgment, never a fill.
    assert!(server.join().is_ok());
}

#[test]
fn signed_post_fails_closed_for_exchange_errors_malformed_and_oversized_responses() {
    let oversized = vec![b' '; MAX_JSON_BYTES as usize + 1];
    let (client, server) = post_server(vec![
        br#"{"code":"50113","msg":"invalid signature"}"#.to_vec(),
        b"{invalid json".to_vec(),
        oversized,
    ]);
    let order = TestOrder {
        inst_id: "BTC-USDT",
        trade_mode: "cash",
        client_order_id: "client123",
        side: "buy",
        order_type: "market",
        sz: "100.00",
    };

    for expected in [
        ProviderErrorCode::InvalidCredentials,
        ProviderErrorCode::InvalidResponse,
        ProviderErrorCode::InvalidResponse,
    ] {
        let result = client.post::<Vec<OrderAcknowledgement>, _>(
            "/api/v5/trade/order",
            &order,
            &credentials(),
            OkxEnvironment::Production,
        );
        assert_eq!(result.unwrap_err(), expected);
    }
    server.join().unwrap();
}

fn header_value<'a>(headers: &'a str, name: &str) -> &'a str {
    headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(header, _)| header.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim())
        })
        .unwrap()
}

#[test]
fn public_market_data_and_history_candles_need_no_credentials() {
    let (client, handle) = server(
        vec![
            (
                "/api/v5/market/tickers?instType=SPOT",
                serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT","last":"99999.12345678","open24h":"99000","high24h":"100000","low24h":"98000","vol24h":"100","ts":"1708587373361"}]}),
            ),
            (
                "/api/v5/market/history-candles?instId=BTC-USDT&bar=1H&limit=100&after=1708587373361",
                serde_json::json!({"code":"0","data":[["1708583773361","1","3","1","2","5","10","10","1"]]}),
            ),
        ],
        false,
    );
    let tickers = client.read_tickers().unwrap();
    assert_eq!(tickers[0].last, "99999.12345678");
    let candles = client
        .read_candles("BTC-USDT", "1H", Some("1708587373361"))
        .unwrap();
    assert_eq!(candles[0].close, "2");
    assert!(candles[0].confirmed);
    assert!(client.read_candles("BTC-USDT&ccy=USD", "1H", None).is_err());
    handle.join().unwrap();
}

#[test]
fn signed_fills_use_bill_id_and_two_assets_with_exact_quote_and_fee() {
    let (client, handle) = server(
        vec![(
            "/api/v5/trade/fills-history?instType=SPOT&begin=1708500000000&end=1708600000000&limit=100&after=999",
            serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT","billId":"456","side":"buy","fillSz":"0.00192834","fillPx":"51858","fee":"-0.00000192834","feeCcy":"BTC","ts":"1708587373361"}]}),
        )],
        true,
    );
    let request = model::HistoryRequest {
        source: model::HistorySource::Fills,
        begin: 1708500000000,
        end: 1708600000000,
        after: Some("999".into()),
        account_uid: None,
    };
    let page = client
        .read_history(&credentials(), "123", &request)
        .unwrap();
    assert_eq!(page.records[0].key, "okx:123:fill:456");
    assert_eq!(page.records[0].legs[1].quantity, "-99.99985572");
    assert_eq!(page.records[0].fee.as_ref().unwrap().asset, "BTC");
    assert!(page.records[0].validate());
    handle.join().unwrap();
}

#[test]
fn funding_history_uses_timestamp_pagination_and_keeps_unsuccessful_states() {
    let (client, handle) = server(
        vec![(
            "/api/v5/asset/withdrawal-history?after=1708600000001&before=1708499999999&limit=100",
            serde_json::json!({"code":"0","data":[{"wdId":"456","ccy":"ETH","amt":"0.029809","fee":"0.007","feeCcy":"ETH","state":"-1","ts":"1708587373361"}]}),
        )],
        true,
    );
    let request = model::HistoryRequest {
        source: model::HistorySource::Withdrawals,
        begin: 1708500000000,
        end: 1708600000000,
        after: None,
        account_uid: None,
    };
    let page = client
        .read_history(&credentials(), "123", &request)
        .unwrap();
    assert!(!page.records[0].confirmable());
    assert_eq!(page.records[0].legs[0].quantity, "-0.029809");
    assert_eq!(page.records[0].fee.as_ref().unwrap().quantity, "-0.007");
    handle.join().unwrap();
}

#[test]
fn provider_errors_do_not_require_a_data_array() {
    let (client, handle) = server(
        vec![(
            "/api/v5/market/tickers?instType=SPOT",
            serde_json::json!({"code":"50011","msg":"limited"}),
        )],
        false,
    );
    assert_eq!(
        client.read_tickers().unwrap_err(),
        ProviderErrorCode::RateLimited
    );
    handle.join().unwrap();
}

#[test]
fn assets_keep_funding_and_trading_separate_and_missing_values_unknown() {
    let (client, handle) = server(
        vec![
            (
                "/api/v5/account/balance",
                serde_json::json!({"code":"0","data":[{"details":[{"ccy":"BTC","cashBal":"0.00000001","availBal":"","frozenBal":"0","eq":"0.00000001","eqUsd":"0.000987654321"}]}]}),
            ),
            (
                "/api/v5/asset/balances",
                serde_json::json!({"code":"0","data":[{"ccy":"USDT","bal":"12.34000000","availBal":"10.34","frozenBal":"2"}]}),
            ),
            (
                "/api/v5/asset/asset-valuation?ccy=USD",
                serde_json::json!({"code":"0","data":[{"totalBal":"12.340987654321","details":{"trading":"0.000987654321","funding":"12.34","earn":"0"},"ts":"1708587373361"}]}),
            ),
        ],
        true,
    );
    let assets = client.read_assets(&credentials()).unwrap();
    assert_eq!(assets.balances.len(), 2);
    assert_eq!(assets.balances[0].available, None);
    assert_eq!(assets.balances[1].account, "funding");
    assert_eq!(assets.valuation.total_usd, "12.340987654321");
    handle.join().unwrap();
}

#[test]
fn archived_and_recent_bills_share_a_stable_deduplication_key() {
    let row = serde_json::json!({"billId":"456","ccy":"USDT","balChg":"-0.25","type":"2","subType":"1","ts":"1708587373361","instId":"BTC-USDT"});
    let (client, handle) = server(
        vec![
            (
                "/api/v5/account/bills?begin=1708500000000&end=1708600000000&limit=100",
                serde_json::json!({"code":"0","data":[row.clone()]}),
            ),
            (
                "/api/v5/account/bills-archive?begin=1708500000000&end=1708600000000&limit=100",
                serde_json::json!({"code":"0","data":[row]}),
            ),
        ],
        true,
    );
    let mut request = model::HistoryRequest {
        source: model::HistorySource::Bills,
        begin: 1708500000000,
        end: 1708600000000,
        after: None,
        account_uid: None,
    };
    let recent = client
        .read_history(&credentials(), "123", &request)
        .unwrap();
    request.source = model::HistorySource::BillsArchive;
    let archive = client
        .read_history(&credentials(), "123", &request)
        .unwrap();
    assert_eq!(recent.records[0], archive.records[0]);
    handle.join().unwrap();
}

#[test]
fn unified_usd_orderbook_uses_the_actual_trade_quote_currency() {
    let (client, handle) = server(
        vec![(
            "/api/v5/trade/fills-history?instType=SPOT&begin=1708500000000&end=1708600000000&limit=100",
            serde_json::json!({"code":"0","data":[{
                "billId":"456","instId":"BTC-USD","tradeQuoteCcy":"USDC","side":"buy","fillSz":"0.01","fillPx":"50000","fee":"-0.5","feeCcy":"USDC","ts":"1708587373361"
            }]}),
        )],
        true,
    );
    let request = model::HistoryRequest {
        source: model::HistorySource::Fills,
        begin: 1708500000000,
        end: 1708600000000,
        after: None,
        account_uid: None,
    };
    let result = client
        .read_history(&credentials(), "123", &request)
        .unwrap();
    assert_eq!(result.records[0].legs[1].asset, "USDC");
    assert!(result.records[0].validate());
    handle.join().unwrap();
}

#[test]
fn positions_preserve_net_pnl_without_subtracting_its_included_fees() {
    let history = serde_json::json!({"posId":"789","instId":"BTC-USDT-SWAP","ccy":"USDT","posSide":"long","mgnMode":"cross","closeTotalPos":"2","openAvgPx":"98000","closeAvgPx":"99000","realizedPnl":"0.001","pnl":"0.0011","fee":"-0.0001","fundingFee":"0","liqPenalty":"0","uTime":"1708587373361"});
    let (client, handle) = server(
        vec![
            (
                "/api/v5/account/positions?instType=SWAP",
                serde_json::json!({"code":"0","data":[]}),
            ),
            (
                "/api/v5/account/positions-history?instType=SWAP&limit=100&after=1708600000000",
                serde_json::json!({"code":"0","data":[history]}),
            ),
        ],
        true,
    );
    let positions = client
        .read_positions(&credentials(), "SWAP", Some("1708600000000"))
        .unwrap();
    assert_eq!(positions.history[0].realized_pnl.as_deref(), Some("0.001"));
    assert_eq!(positions.history[0].fee.as_deref(), Some("-0.0001"));
    handle.join().unwrap();
}

#[test]
fn order_ipc_models_keep_snake_case_while_deserializing_okx_field_names() {
    let acknowledgement: OrderAcknowledgement = serde_json::from_value(serde_json::json!({
        "ordId": "123456",
        "clOrdId": "client123",
        "sCode": "0",
        "sMsg": ""
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(acknowledgement).unwrap(),
        serde_json::json!({
            "order_id": "123456",
            "client_order_id": "client123",
            "code": "0",
            "message": ""
        })
    );

    let snapshot: OrderSnapshot = serde_json::from_value(serde_json::json!({
        "instId": "BTC-USDT",
        "ordId": "123456",
        "clOrdId": "client123",
        "instType": "SPOT",
        "side": "buy",
        "ordType": "market",
        "tdMode": "cash",
        "sz": "0.001",
        "px": "",
        "avgPx": "100",
        "accFillSz": "0.001",
        "state": "filled",
        "posSide": "net",
        "reduceOnly": "false",
        "tgtCcy": "base_ccy",
        "cTime": "123",
        "uTime": "124"
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        serde_json::json!({
            "inst_id": "BTC-USDT",
            "order_id": "123456",
            "client_order_id": "client123",
            "inst_type": "SPOT",
            "side": "buy",
            "order_type": "market",
            "trade_mode": "cash",
            "sz": "0.001",
            "price": "",
            "average_price": "100",
            "accumulated_fill_size": "0.001",
            "state": "filled",
            "position_side": "net",
            "reduce_only": "false",
            "target_currency": "base_ccy",
            "created_at": "123",
            "updated_at": "124"
        })
    );
}

#[test]
#[ignore = "requires a live public OKX endpoint; no private credentials"]
fn live_okx_public_tickers_and_candles() {
    let client = OkxClient::default();
    let tickers = client.read_tickers().unwrap();
    assert!(tickers.iter().any(|t| t.instrument == "BTC-USDT"));
    let candles = client.read_candles("BTC-USDT", "1H", None).unwrap();
    assert!(!candles.is_empty());
    let older = client
        .read_candles("BTC-USDT", "1H", Some(&candles[0].timestamp.to_string()))
        .unwrap();
    assert!(older.iter().any(|c| c.timestamp < candles[0].timestamp));
    println!(
        "Live public OKX: {} tickers, {} current candles, {} older candles",
        tickers.len(),
        candles.len(),
        older.len()
    );
}
