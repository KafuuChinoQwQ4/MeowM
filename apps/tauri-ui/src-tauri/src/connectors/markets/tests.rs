use super::IfindTokenCache;
use super::binance::{BinanceClient, BinanceCredentials, decimal_is_nonzero, map_binance_code};
use super::ifind::{IfindClient, map_ifind_error, validate_ifind_symbol};
use super::map_exchange_http_status;
use super::okx::{OkxClient, OkxCredentials, map_okx_code};
use crate::connectors::model::ProviderErrorCode;
use crate::connectors::payments::agent;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    let header = String::from_utf8(bytes.clone()).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(str::to_owned)
        })
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let body_start = bytes.len();
    if length > 0 {
        bytes.resize(body_start + length, 0);
        stream.read_exact(&mut bytes[body_start..]).unwrap();
    } else if header
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        loop {
            let mut size_line = Vec::new();
            while !size_line.ends_with(b"\r\n") {
                stream.read_exact(&mut byte).unwrap();
                size_line.push(byte[0]);
            }
            let size = usize::from_str_radix(
                std::str::from_utf8(&size_line[..size_line.len() - 2]).unwrap(),
                16,
            )
            .unwrap();
            if size == 0 {
                let mut ending = [0; 2];
                stream.read_exact(&mut ending).unwrap();
                assert_eq!(&ending, b"\r\n");
                break;
            }
            let start = bytes.len();
            bytes.resize(start + size, 0);
            stream.read_exact(&mut bytes[start..]).unwrap();
            let mut ending = [0; 2];
            stream.read_exact(&mut ending).unwrap();
            assert_eq!(&ending, b"\r\n");
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn respond(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn header<'a>(request: &'a str, name: &str) -> &'a str {
    request
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
        .unwrap()
}

fn body_value(request: &str) -> serde_json::Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap()
}

fn hmac_hex(key: &[u8], message: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(message);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hmac_base64(key: &[u8], message: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(message);
    STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn binance_reads_only_account_balances_with_signed_get() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let request_line = request.lines().next().unwrap();
        assert!(request_line.starts_with("GET /api/v3/account?recvWindow=5000&timestamp="));
        assert!(header(&request, "X-MBX-APIKEY") == "public-key");
        let query = request_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split_once('?')
            .unwrap()
            .1;
        let (payload, signature) = query.rsplit_once("&signature=").unwrap();
        assert_eq!(signature, hmac_hex(b"secret", payload.as_bytes()));
        respond(
            &mut stream,
            r#"{"balances":[{"asset":"BTC","free":"0.12500000","locked":"0.00000000"},{"asset":"ETH","free":"0.00","locked":"0"},{"asset":"币安人生","free":"0.00000001","locked":"0"}]}"#,
        );
    });

    let client = BinanceClient::for_test(agent(), endpoint);
    let balances = client
        .read_balances(&BinanceCredentials {
            api_key: "public-key",
            secret_key: "secret",
        })
        .unwrap();
    server.join().unwrap();
    assert_eq!(balances.len(), 2);
    assert_eq!(balances[0].asset, "BTC");
    assert_eq!(balances[0].free, "0.12500000");
    assert_eq!(balances[0].locked, "0.00000000");
    assert_eq!(balances[1].asset, "币安人生");
    assert_eq!(balances[1].free, "0.00000001");
}

#[test]
fn okx_reads_account_balance_and_signs_the_exact_get_path() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("GET /api/v5/account/balance HTTP/1.1\r\n"));
        assert_eq!(header(&request, "OK-ACCESS-KEY"), "api-key");
        assert_eq!(header(&request, "OK-ACCESS-PASSPHRASE"), "passphrase");
        let timestamp = header(&request, "OK-ACCESS-TIMESTAMP");
        assert!(timestamp.ends_with('Z'));
        let expected = hmac_base64(
            b"secret",
            format!("{timestamp}GET/api/v5/account/balance").as_bytes(),
        );
        assert_eq!(header(&request, "OK-ACCESS-SIGN"), expected);
        respond(
            &mut stream,
            r#"{"code":"0","msg":"","data":[{"details":[{"ccy":"USDT","availBal":"12.3400","frozenBal":"0.0000"},{"ccy":"BTC","availBal":"0","frozenBal":"0"}]}]}"#,
        );
    });

    let client = OkxClient::for_test(agent(), endpoint);
    let balances = client
        .read_balances(&OkxCredentials {
            api_key: "api-key",
            secret_key: "secret",
            passphrase: "passphrase",
        })
        .unwrap();
    server.join().unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].asset, "USDT");
    assert_eq!(balances[0].free, "12.3400");
}

#[test]
fn ifind_fetches_a_short_lived_token_then_reads_quote_without_exposing_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut token_stream, _) = listener.accept().unwrap();
        let token_request = read_request(&mut token_stream);
        assert!(token_request.starts_with("POST /api/v1/get_access_token HTTP/1.1\r\n"));
        assert_eq!(header(&token_request, "refresh_token"), "refresh-secret");
        respond(
            &mut token_stream,
            r#"{"data":{"access_token":"access-secret"}}"#,
        );

        let (mut quote_stream, _) = listener.accept().unwrap();
        let quote_request = read_request(&mut quote_stream);
        assert!(quote_request.starts_with("POST /api/v1/real_time_quotation HTTP/1.1\r\n"));
        assert_eq!(header(&quote_request, "access_token"), "access-secret");
        assert_eq!(header(&quote_request, "ifindlang"), "cn");
        assert_eq!(
            body_value(&quote_request),
            serde_json::json!({"codes":"000001.SZ","indicators":"latest"})
        );
        respond(
            &mut quote_stream,
            r#"{"errorcode":0,"tables":[{"thscode":"000001.SZ","time":["2026-10-31 15:00:00"],"table":{"latest":[12.34]}}]}"#,
        );
        quote_request
    });

    let client = IfindClient::for_test(agent(), endpoint);
    let quote = client
        .read_quote("refresh-secret", "000001.SZ", &IfindTokenCache::default())
        .unwrap();
    let request = server.join().unwrap();
    assert_eq!(quote.symbol, "000001.SZ");
    assert_eq!(quote.last, "12.34");
    assert_eq!(quote.quote_time.as_deref(), Some("2026-10-31 15:00:00"));
    assert!(
        !serde_json::to_string(&quote)
            .unwrap()
            .contains("access-secret")
    );
    assert_eq!(
        body_value(&request),
        serde_json::json!({"codes":"000001.SZ","indicators":"latest"})
    );
}

#[test]
fn ifind_reuses_the_in_memory_access_token_for_multiple_quotes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            if index == 0 {
                assert!(request.starts_with("POST /api/v1/get_access_token"));
                respond(
                    &mut stream,
                    r#"{"data":{"access_token":"one-access-token"}}"#,
                );
            } else {
                assert_eq!(header(&request, "access_token"), "one-access-token");
                respond(
                    &mut stream,
                    r#"{"errorcode":0,"tables":[{"thscode":"600000.SH","table":{"latest":[8.5]}}]}"#,
                );
            }
            requests.push(request);
        }
        requests
    });

    let client = IfindClient::for_test(agent(), endpoint);
    let cache = IfindTokenCache::default();
    client.read_quote("refresh", "600000.SH", &cache).unwrap();
    client.read_quote("refresh", "600000.SH", &cache).unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.contains("get_access_token"))
            .count(),
        1
    );
}

#[test]
fn ifind_refreshes_an_expired_access_token_once_and_retries_the_quote() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for index in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            match index {
                0 => {
                    assert!(request.starts_with("POST /api/v1/get_access_token"));
                    respond(&mut stream, r#"{"data":{"access_token":"old-token"}}"#);
                }
                1 => {
                    assert_eq!(header(&request, "access_token"), "old-token");
                    respond(
                        &mut stream,
                        r#"{"errorcode":-1302,"errmsg":"Access_Token is expired or illegal"}"#,
                    );
                }
                2 => {
                    assert!(request.starts_with("POST /api/v1/get_access_token"));
                    respond(&mut stream, r#"{"data":{"access_token":"new-token"}}"#);
                }
                3 => {
                    assert_eq!(header(&request, "access_token"), "new-token");
                    respond(
                        &mut stream,
                        r#"{"errorcode":0,"tables":[{"thscode":"600000.SH","table":{"latest":[9.25]}}]}"#,
                    );
                }
                _ => unreachable!(),
            }
        }
    });

    let quote = IfindClient::for_test(agent(), endpoint)
        .read_quote("refresh", "600000.SH", &IfindTokenCache::default())
        .unwrap();
    server.join().unwrap();
    assert_eq!(quote.last, "9.25");
}

#[test]
fn ifind_symbol_validation_is_strict_and_supports_mainland_exchange_suffixes() {
    for valid in ["000001.SZ", "600000.SH", "430047.BJ"] {
        assert_eq!(validate_ifind_symbol(valid), Ok(()));
    }
    for invalid in [
        "000001.sz",
        "000001.US",
        "60000.SH",
        "000001.SZ?x=1",
        "600000.SH ",
    ] {
        assert!(validate_ifind_symbol(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn provider_failures_map_to_safe_ui_error_codes() {
    assert_eq!(
        map_exchange_http_status(418),
        ProviderErrorCode::RateLimited
    );
    assert_eq!(
        map_binance_code(-2015),
        ProviderErrorCode::InvalidCredentials
    );
    assert_eq!(map_okx_code("50011"), ProviderErrorCode::RateLimited);
    assert_eq!(map_okx_code("50113"), ProviderErrorCode::InvalidCredentials);
    assert_eq!(
        map_ifind_error(-1302),
        ProviderErrorCode::InvalidCredentials
    );
    assert_eq!(map_ifind_error(-1303), ProviderErrorCode::PermissionDenied);
    assert_eq!(map_ifind_error(-4001), ProviderErrorCode::DataUnavailable);
}

#[test]
fn exchange_amount_filter_keeps_decimal_precision_and_rejects_invalid_values() {
    assert_eq!(decimal_is_nonzero("0.00000000"), Ok(false));
    assert_eq!(decimal_is_nonzero("000.0001"), Ok(true));
    assert_eq!(decimal_is_nonzero("000"), Ok(false));
    assert_eq!(
        decimal_is_nonzero("1e-8"),
        Err(ProviderErrorCode::InvalidResponse)
    );
    assert_eq!(
        decimal_is_nonzero("-1"),
        Err(ProviderErrorCode::InvalidResponse)
    );
}
