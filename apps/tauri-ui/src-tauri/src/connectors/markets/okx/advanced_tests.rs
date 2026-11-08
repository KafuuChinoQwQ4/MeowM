use super::advanced::{AdvancedClient, ApiRequest};
use crate::connectors::credential_store::{CredentialService, MemoryCredentialStore};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
};

fn catalogue() -> Value {
    json!({"source_url":"https://www.okx.com/docs-v5/", "retrieved_on":"2026-11-08",
        "rest":[
            {"id":"read", "method":"GET", "path":"/api/v5/account/balance", "auth":"read", "mutating":false, "body_kind":"object", "parameters":[
                {"name":"ccy", "type":"String", "required":true, "conditional":false, "description":"currency"},
                {"name":"sequence", "type":"Integer", "required":false, "conditional":false, "description":"sequence"}]},
            {"id":"batch", "method":"POST", "path":"/api/v5/trade/batch-orders", "auth":"trade", "mutating":true, "body_kind":"array", "parameters":[
                {"name":"instId", "type":"String", "required":true, "conditional":false, "description":"instrument"},
                {"name":"units", "type":"Integer", "required":false, "conditional":false, "description":"units"},
                {"name":"attachAlgoOrds", "type":"Array of objects", "required":false, "conditional":false, "description":"attached orders"},
                {"name":"> triggerPx", "type":"String", "required":true, "conditional":false, "description":"trigger"}]},
            {"id":"withdraw", "method":"POST", "path":"/api/v5/asset/withdrawal", "auth":"withdraw", "mutating":true, "body_kind":"object", "parameters":[
                {"name":"amt", "type":"String", "required":true, "conditional":false, "description":"amount"},
                {"name":"pwd", "type":"String", "required":false, "conditional":false, "description":"password"}]},
            {"id":"public", "method":"GET", "path":"/api/v5/public/time", "auth":"public", "mutating":false, "body_kind":"object", "parameters":[]}
            ,{"id":"public_post", "method":"POST", "path":"/api/v5/tradingBot/grid/min-investment", "auth":"public", "mutating":false, "body_kind":"object", "parameters":[]}
            ,{"id":"sbe", "method":"GET", "path":"/api/v5/market/books-sbe", "auth":"public", "mutating":false, "body_kind":"object", "parameters":[]}
        ], "websocket":[]})
}

fn service() -> CredentialService {
    let result = CredentialService::new(Arc::new(MemoryCredentialStore::default()));
    for (provider, key, secret) in [
        ("okx", "read-key", "read-secret"),
        ("okx_trade_demo", "demo-key", "demo-secret"),
        ("okx_trade_live", "live-key", "live-secret"),
        ("okx_withdraw", "withdraw-key", "withdraw-secret"),
    ] {
        result.save(serde_json::from_value(json!({"provider":provider,"api_key":key,"secret_key":secret,"passphrase":"passphrase"})).unwrap()).unwrap();
    }
    result
}

fn request(id: &str, env: &str, credentials: &str, params: Value, confirmed: bool) -> ApiRequest {
    serde_json::from_value(json!({"endpoint_id":id,"environment":env,"credentials":credentials,"params":params,"confirmed":confirmed,"request_id":"test-request-1"})).unwrap()
}

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
    }
    let header = String::from_utf8(header).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (header, body)
}

fn header<'a>(headers: &'a str, key: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        line.split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.trim())
    })
}

fn signed(secret: &str, timestamp: &str, method: &str, path: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{timestamp}{method}{path}").as_bytes());
    mac.update(body);
    STANDARD.encode(mac.finalize().into_bytes())
}

fn server<F>(check: F, response: &'static str) -> (String, std::thread::JoinHandle<()>)
where
    F: FnOnce(String, Vec<u8>) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut stream);
        check(headers, body);
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
    });
    (url, thread)
}

#[test]
#[ignore = "requires a live public OKX endpoint; no private credentials"]
fn live_catalogue_ticker_request_returns_market_data() {
    let credentials = CredentialService::new(Arc::new(MemoryCredentialStore::default()));
    for environment in ["production", "demo"] {
        let result = AdvancedClient::default().execute(
            request(
                "order-book-trading-market-data-get-ticker",
                environment,
                "read",
                json!({"instId":"BTC-USDT"}),
                false,
            ),
            &credentials,
        );
        assert_eq!(result.response.status, "success", "{:?}", result.response);
        assert_eq!(result.response.data[0]["instId"], "BTC-USDT");
    }
}

#[test]
fn get_query_is_encoded_once_and_signed_as_sent_with_demo_header() {
    let path = "/api/v5/account/balance?ccy=BTC%2FUSDT%2B%26%3D";
    let (url, handle) = server(
        move |headers, body| {
            assert!(headers.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            assert_eq!(header(&headers, "ok-access-key"), Some("demo-key"));
            assert_eq!(header(&headers, "x-simulated-trading"), Some("1"));
            let timestamp = header(&headers, "ok-access-timestamp").unwrap();
            assert_eq!(
                header(&headers, "ok-access-sign"),
                Some(signed("demo-secret", timestamp, "GET", path, &body).as_str())
            );
            assert!(body.is_empty());
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let client = AdvancedClient::for_test(url, catalogue());
    let result = client.execute(
        request("read", "demo", "trade", json!({"ccy":"BTC/USDT+&="}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn batch_post_signs_exact_array_bytes_and_reports_item_failures() {
    let path = "/api/v5/trade/batch-orders";
    let (url, handle) = server(
        move |headers, body| {
            assert!(headers.starts_with(&format!("POST {path} HTTP/1.1\r\n")));
            assert_eq!(header(&headers, "ok-access-key"), Some("demo-key"));
            assert_eq!(header(&headers, "x-simulated-trading"), Some("1"));
            let timestamp = header(&headers, "ok-access-timestamp").unwrap();
            assert_eq!(
                header(&headers, "ok-access-sign"),
                Some(signed("demo-secret", timestamp, "POST", path, &body).as_str())
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!([{"instId":"BTC-USDT","attachAlgoOrds":[{"triggerPx":"0.00000001"}]}])
            );
        },
        r#"{"code":"0","msg":"","data":[{"sCode":"0"},{"sCode":"51000","sMsg":"invalid"}]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request(
            "batch",
            "demo",
            "trade",
            json!([{"instId":"BTC-USDT","attachAlgoOrds":[{"triggerPx":"0.00000001"}]}]),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "partial_failure");
    assert_eq!(result.response.request_id, "test-request-1");
    handle.join().unwrap();
}

#[test]
fn unconfirmed_unknown_and_invalid_parameters_never_reach_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = AdvancedClient::for_test(
        format!("http://{}", listener.local_addr().unwrap()),
        catalogue(),
    );
    for input in [
        request(
            "batch",
            "production",
            "trade",
            json!([{"instId":"BTC-USDT"}]),
            false,
        ),
        request(
            "/api/v5/trade/order",
            "production",
            "trade",
            json!({}),
            true,
        ),
        request(
            "batch",
            "production",
            "trade",
            json!([{"instId":"BTC-USDT", "extra":"no"}]),
            true,
        ),
        request(
            "batch",
            "production",
            "trade",
            json!([{"instId":"BTC-USDT", "attachAlgoOrds":[{}]}]),
            true,
        ),
        request(
            "batch",
            "production",
            "trade",
            json!({"instId":"BTC-USDT"}),
            true,
        ),
        request("read", "demo", "read", json!({"ccy":"BTC"}), false),
    ] {
        let result = client.execute(input, &service());
        assert_eq!(result.response.status, "rejected");
        assert!(!result.sent);
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn withdrawal_uses_only_dedicated_production_credentials() {
    let (url, handle) = server(
        |headers, _| {
            assert_eq!(header(&headers, "ok-access-key"), Some("withdraw-key"));
            assert_eq!(header(&headers, "x-simulated-trading"), None);
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let client = AdvancedClient::for_test(url, catalogue());
    for input in [
        request("withdraw", "demo", "withdraw", json!({"amt":"1"}), true),
        request("withdraw", "production", "trade", json!({"amt":"1"}), true),
    ] {
        let result = client.execute(input, &service());
        assert_eq!(result.response.status, "rejected");
        assert!(!result.sent);
    }
    let result = client.execute(
        request(
            "withdraw",
            "production",
            "withdraw",
            json!({"amt":"1"}),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn public_calls_never_send_credentials() {
    let (url, handle) = server(
        |headers, _| {
            assert!(!headers.to_ascii_lowercase().contains("ok-access-"));
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("public", "production", "withdraw", json!({}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn documented_readonly_public_post_needs_no_confirmation_or_credentials() {
    let (url, handle) = server(
        |headers, body| {
            assert!(
                headers.starts_with("POST /api/v5/tradingBot/grid/min-investment HTTP/1.1\r\n")
            );
            assert!(!headers.to_ascii_lowercase().contains("ok-access-"));
            assert_eq!(body, b"{}");
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("public_post", "production", "trade", json!({}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn rejected_errors_and_data_never_expose_credentials() {
    let (url, handle) = server(
        |_, _| {},
        r#"{"code":"51000","msg":"read-secret, read-key, passphrase", "data":[{"apiKey":"new-api-key","secretKey":"new-secret","other":"read-secret"}]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("read", "production", "read", json!({"ccy":"BTC"}), false),
        &service(),
    );
    assert_eq!(result.response.status, "rejected");
    let serialized = serde_json::to_string(&result.response).unwrap();
    for secret in [
        "read-secret",
        "read-key",
        "passphrase",
        "new-api-key",
        "new-secret",
    ] {
        assert!(!serialized.contains(secret));
    }
    handle.join().unwrap();
}

#[test]
fn lost_response_after_mutation_is_unknown_and_preserves_request_id() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    });
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request(
            "withdraw",
            "production",
            "withdraw",
            json!({"amt":"1"}),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "unknown");
    assert_eq!(result.response.request_id, "test-request-1");
    assert!(result.sent);
    handle.join().unwrap();
}

#[test]
fn sbe_binary_is_returned_without_claiming_it_was_decoded() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 8\r\nConnection: close\r\n\r\n\x01\x00\x02\x00\x03\x00\x04\x00").unwrap();
    });
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("sbe", "production", "read", json!({}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    assert_eq!(
        result.response.data,
        json!({"format":"sbe","encoding":"base64","decoded":false,"bytes":"AQACAAMABAA="})
    );
    handle.join().unwrap();
}

#[test]
fn an_html_or_text_error_cannot_be_mistaken_for_sbe_data() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\nConnection: close\r\n\r\n<html></html>").unwrap();
    });
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("sbe", "production", "read", json!({}), false),
        &service(),
    );
    assert_eq!(result.response.status, "rejected");
    handle.join().unwrap();
}

#[test]
fn invalidated_session_is_checked_again_immediately_before_send() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = AdvancedClient::for_test(
        format!("http://{}", listener.local_addr().unwrap()),
        catalogue(),
    );
    let result = client.execute_guarded(
        request(
            "withdraw",
            "production",
            "withdraw",
            json!({"amt":"1"}),
            true,
        ),
        &service(),
        || false,
    );
    assert_eq!(result.response.status, "rejected");
    assert!(!result.sent);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn supported_expiry_header_is_sent_without_changing_signature() {
    let path = "/api/v5/trade/batch-orders";
    let (url, handle) = server(
        move |headers, body| {
            assert_eq!(header(&headers, "expTime"), Some("1730000000123"));
            let timestamp = header(&headers, "ok-access-timestamp").unwrap();
            assert_eq!(
                header(&headers, "ok-access-sign"),
                Some(signed("live-secret", timestamp, "POST", path, &body).as_str())
            );
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let input: ApiRequest = serde_json::from_value(json!({"endpoint_id":"batch","environment":"production","credentials":"trade","params":[{"instId":"BTC-USDT"}],"confirmed":true,"request_id":"req-1","exp_time":"1730000000123"})).unwrap();
    let result = AdvancedClient::for_test(url, catalogue()).execute(input, &service());
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn expiry_is_rejected_for_other_paths_or_non_decimal_values_before_send() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = AdvancedClient::for_test(
        format!("http://{}", listener.local_addr().unwrap()),
        catalogue(),
    );
    for (id, credential, params, expiry) in [
        ("read", "read", json!({"ccy":"BTC"}), "1730000000123"),
        ("batch", "trade", json!([{"instId":"BTC-USDT"}]), "17x"),
    ] {
        let input: ApiRequest = serde_json::from_value(json!({"endpoint_id":id,"environment":"production","credentials":credential,"params":params,"confirmed":true,"request_id":"req-1","exp_time":expiry})).unwrap();
        let result = client.execute(input, &service());
        assert_eq!(result.response.status, "rejected");
        assert!(!result.sent);
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn echoed_sensitive_request_values_are_redacted_even_under_other_names() {
    let (url, handle) = server(
        |_, _| {},
        r#"{"code":"51000","msg":"invalid one-time-password", "data":[{"pwd":"one-time-password","note":"one-time-password"}]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request(
            "withdraw",
            "production",
            "withdraw",
            json!({"amt":"1", "pwd":"one-time-password"}),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "rejected");
    assert!(
        !serde_json::to_string(&result.response)
            .unwrap()
            .contains("one-time-password")
    );
    handle.join().unwrap();
}

#[test]
fn result_classification_uses_codes_before_redacting_matching_credentials() {
    let service = CredentialService::new(Arc::new(MemoryCredentialStore::default()));
    service.save(serde_json::from_value(json!({"provider":"okx","api_key":"read-key","secret_key":"read-secret","passphrase":"0"})).unwrap()).unwrap();
    let (url, handle) = server(
        |_, _| {},
        r#"{"code":"0","msg":"", "data":[{"sCode":"0"}]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("read", "production", "read", json!({"ccy":"BTC"}), false),
        &service,
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn readonly_endpoint_can_use_explicitly_selected_demo_trade_credentials() {
    let (url, handle) = server(
        |headers, _| {
            assert_eq!(header(&headers, "ok-access-key"), Some("demo-key"));
            assert_eq!(header(&headers, "x-simulated-trading"), Some("1"));
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("read", "demo", "trade", json!({"ccy":"BTC"}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn real_algo_order_catalogue_accepts_other_strategies_without_iceberg_limits() {
    let (url, handle) = server(
        |headers, body| {
            assert!(headers.starts_with("POST /api/v5/trade/order-algo HTTP/1.1\r\n"));
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({
                    "instId":"BTC-USDT", "tdMode":"cash", "side":"sell", "ordType":"conditional", "sz":"0.01", "tpTriggerPx":"75000", "tpOrdPx":"-1"
                })
            );
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let input = request(
        "order-book-trading-algo-trading-post-place-algo-order",
        "demo",
        "trade",
        json!({
            "instId":"BTC-USDT", "tdMode":"cash", "side":"sell", "ordType":"conditional", "sz":"0.01", "tpTriggerPx":"75000", "tpOrdPx":"-1"
        }),
        true,
    );
    let result = AdvancedClient::for_test(url, super::advanced::catalogue().clone())
        .execute(input, &service());
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn server_failure_and_malformed_write_response_remain_unknown() {
    for raw_response in [
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nnot-json",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            stream.write_all(raw_response.as_bytes()).unwrap();
        });
        let result = AdvancedClient::for_test(url, catalogue()).execute(
            request(
                "withdraw",
                "production",
                "withdraw",
                json!({"amt":"1"}),
                true,
            ),
            &service(),
        );
        assert_eq!(result.response.status, "unknown");
        assert_eq!(result.response.request_id, "test-request-1");
        handle.join().unwrap();
    }
}

#[test]
fn bundled_catalogue_rest_entries_match_transport_schema_and_have_unique_ids() {
    use std::collections::HashSet;
    let entries = super::advanced::catalogue()
        .get("rest")
        .and_then(Value::as_array)
        .unwrap();
    assert!(!entries.is_empty());
    let mut ids = HashSet::new();
    for entry in entries {
        let _: super::advanced::RestEndpoint = serde_json::from_value(entry.clone()).unwrap();
        let id = entry["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate endpoint id: {id}");
        assert!(matches!(entry["method"].as_str(), Some("GET" | "POST")));
        assert!(entry["path"].as_str().unwrap().starts_with("/api/v5/"));
        assert!(matches!(
            entry["auth"].as_str(),
            Some("public" | "read" | "trade" | "withdraw")
        ));
        assert!(matches!(
            entry["body_kind"].as_str(),
            Some("object" | "array")
        ));
    }
}

#[test]
fn documented_string_amount_rejects_json_numbers_before_sending() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = AdvancedClient::for_test(
        format!("http://{}", listener.local_addr().unwrap()),
        catalogue(),
    );
    let result = client.execute(
        request(
            "withdraw",
            "production",
            "withdraw",
            json!({"amt":0.1}),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "rejected");
    assert!(!result.sent);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn integer_query_preserves_digits_beyond_javascript_safe_integer() {
    let path = "/api/v5/account/balance?ccy=BTC&sequence=9007199254740993123456789";
    let (url, handle) = server(
        move |headers, body| {
            assert!(headers.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            let timestamp = header(&headers, "ok-access-timestamp").unwrap();
            assert_eq!(
                header(&headers, "ok-access-sign"),
                Some(signed("read-secret", timestamp, "GET", path, &body).as_str())
            );
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request(
            "read",
            "production",
            "read",
            json!({"ccy":"BTC", "sequence":"9007199254740993123456789"}),
            false,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}

#[test]
fn sbe_snapshot_is_decoded_when_schema_is_supported() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        let mut bytes = Vec::new();
        for number in [26u16, 1006, 1, 0] {
            bytes.extend(number.to_le_bytes());
        }
        for number in [211_874i64, 1_000, 99] {
            bytes.extend(number.to_le_bytes());
        }
        bytes.extend([(-2i8) as u8, (-3i8) as u8]);
        for number in [20u16, 0, 20, 0] {
            bytes.extend(number.to_le_bytes());
        }
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
        stream.write_all(&bytes).unwrap();
    });
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request("sbe", "production", "read", json!({}), false),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    assert_eq!(result.response.data["decoded"], true);
    assert_eq!(
        result.response.data["value"]["event"],
        "SnapshotDepthResponseEvent"
    );
    assert_eq!(result.response.data["value"]["seqId"], "99");
    assert!(result.response.data["bytes"].is_string());
    handle.join().unwrap();
}

#[test]
fn integer_body_is_serialized_as_exact_json_number() {
    let path = "/api/v5/trade/batch-orders";
    let (url, handle) = server(
        move |headers, body| {
            let text = std::str::from_utf8(&body).unwrap();
            assert!(text.contains("\"units\":9007199254740993123456789"));
            let timestamp = header(&headers, "ok-access-timestamp").unwrap();
            assert_eq!(
                header(&headers, "ok-access-sign"),
                Some(signed("demo-secret", timestamp, "POST", path, &body).as_str())
            );
        },
        r#"{"code":"0","msg":"","data":[]}"#,
    );
    let result = AdvancedClient::for_test(url, catalogue()).execute(
        request(
            "batch",
            "demo",
            "trade",
            json!([{"instId":"BTC-USDT", "units":"9007199254740993123456789"}]),
            true,
        ),
        &service(),
    );
    assert_eq!(result.response.status, "success");
    handle.join().unwrap();
}
