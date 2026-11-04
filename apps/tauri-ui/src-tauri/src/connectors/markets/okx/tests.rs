use super::model::{OkxEnvironment, OrderAcknowledgement};
use super::*;
use crate::connectors::payments::{MAX_JSON_BYTES, agent};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
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
fn credentials() -> OkxCredentials<'static> {
    OkxCredentials {
        api_key: "test-key",
        secret_key: "secret",
        passphrase: "test-passphrase",
    }
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
