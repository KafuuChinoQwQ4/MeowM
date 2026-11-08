use super::alipay::{AlipayClient, AlipayCredentials};
use super::crypto::{
    alipay_canonical_parameters, resolve_alipay_download_url, sign_rsa_sha256,
    validate_wechat_bill_date, verify_rsa_sha256, wechat_canonical_request,
};
use super::http::android_http_proxy_uri;
use super::wechat::{WechatClient, WechatCredentials, verify_wechat_bill};
use chrono::NaiveDate;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

const PRIVATE_KEY: &str = include_str!("../testdata/provider_test_private.pem");
const PUBLIC_KEY: &str = include_str!("../testdata/provider_test_public.pem");

#[test]
fn android_system_proxy_uri_accepts_network_hosts_and_rejects_invalid_values() {
    assert_eq!(
        android_http_proxy_uri("127.0.0.1", 10808).as_deref(),
        Some("http://127.0.0.1:10808")
    );
    assert_eq!(
        android_http_proxy_uri("::1", 8080).as_deref(),
        Some("http://[::1]:8080")
    );
    assert_eq!(android_http_proxy_uri("", 10808), None);
    assert_eq!(android_http_proxy_uri("127.0.0.1", 0), None);
    assert_eq!(android_http_proxy_uri("127.0.0.1", 65536), None);
    assert_eq!(android_http_proxy_uri("proxy.example/path", 10808), None);
}

#[test]
fn wechat_canonical_request_has_exact_v3_newline_layout() {
    assert_eq!(
        wechat_canonical_request(
            "GET",
            "/v3/bill/tradebill?bill_date=2026-10-30",
            "123",
            "nonce",
            ""
        ),
        "GET\n/v3/bill/tradebill?bill_date=2026-10-30\n123\nnonce\n\n"
    );
}

#[test]
fn rsa_sha256_signatures_verify_and_reject_modified_messages() {
    let message = b"fixed merchant bill request";
    let signature = sign_rsa_sha256(PRIVATE_KEY, message).unwrap();
    assert!(verify_rsa_sha256(PUBLIC_KEY, message, &signature).is_ok());
    assert!(verify_rsa_sha256(PUBLIC_KEY, b"modified request", &signature).is_err());
}

#[test]
fn alipay_canonical_parameters_are_sorted_and_exclude_sign_fields() {
    let params = BTreeMap::from([
        ("method".to_string(), "alipay.bill".to_string()),
        ("app_id".to_string(), "app-1".to_string()),
        ("sign".to_string(), "ignored".to_string()),
        ("sign_type".to_string(), "RSA2".to_string()),
    ]);
    assert_eq!(
        alipay_canonical_parameters(&params),
        "app_id=app-1&method=alipay.bill&sign_type=RSA2"
    );
}

#[test]
fn wechat_bill_date_must_be_prior_and_within_three_months() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
    assert!(validate_wechat_bill_date("2026-10-30", today).is_ok());
    assert!(validate_wechat_bill_date("2026-10-31", today).is_err());
    assert!(validate_wechat_bill_date("2026-07-30", today).is_err());
    assert!(validate_wechat_bill_date("not-a-date", today).is_err());
}

#[test]
fn alipay_bill_download_url_is_upgraded_and_host_pinned() {
    assert_eq!(
        resolve_alipay_download_url(
            "http://dwbillcenter.alipay.com/downloadBillFile.resource?token=x"
        )
        .unwrap(),
        "https://dwbillcenter.alipay.com/downloadBillFile.resource?token=x"
    );
    for url in [
        "https://attacker.example/downloadBillFile.resource?token=x",
        "https://dwbillcenter.alipay.com.attacker.example/file?token=x",
        "https://user@dwbillcenter.alipay.com/file?token=x",
    ] {
        assert!(resolve_alipay_download_url(url).is_err());
    }
}

#[test]
fn wechat_client_sends_signed_trade_bill_request_and_parses_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0; 1];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let request = String::from_utf8(request).unwrap();
        let body = r#"{"download_url":"https://api.mch.weixin.qq.com/v3/billdownload/file?token=temporary","hash_type":"SHA1","hash_value":"a9993e364706816aba3e25717850c26c9cd0d89d"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });
    let client = WechatClient::for_test(ureq::Agent::new_with_defaults(), endpoint);
    let credentials = WechatCredentials {
        merchant_id: "1900000001",
        certificate_serial: "AABBCCDDEEFF",
        private_key_pem: PRIVATE_KEY,
    };

    let metadata = client
        .request_trade_bill(
            "2026-10-30",
            NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
            &credentials,
        )
        .unwrap();
    let request = server.join().unwrap();
    assert!(request.starts_with(
        "GET /v3/bill/tradebill?bill_date=2026-10-30&bill_type=ALL&tar_type=GZIP HTTP/1.1"
    ));
    let request_lower = request.to_ascii_lowercase();
    assert!(request_lower.contains("authorization: wechatpay2-sha256-rsa2048 "));
    assert!(request_lower.contains("mchid=\"1900000001\""));
    assert!(metadata.download_url.ends_with("token=temporary"));
}

#[test]
fn wechat_gzip_bill_requires_sha1_of_uncompressed_content() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"csv,contents").unwrap();
    let compressed = encoder.finish().unwrap();
    let digest = format!("{:x}", Sha1::digest(b"csv,contents"));

    assert_eq!(verify_wechat_bill(&compressed, &digest).unwrap(), 12);
    assert!(verify_wechat_bill(&compressed, "0000000000000000000000000000000000000000").is_err());
}

#[test]
fn alipay_client_posts_signed_bill_lookup_and_verifies_provider_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0; 1];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let header = String::from_utf8(request.clone()).unwrap();
        let length = header
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .map(str::to_owned)
            })
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let body_start = request.len();
        request.resize(body_start + length, 0);
        stream.read_exact(&mut request[body_start..]).unwrap();
        let request_body = String::from_utf8(request[body_start..].to_vec()).unwrap();

        let response_data = serde_json::json!({
            "code": "10000",
            "msg": "Success",
            "bill_download_url": "http://dwbillcenter.alipay.com/downloadBillFile.resource?token=temporary"
        });
        let signed = BTreeMap::from([(
            "alipay_data_dataservice_bill_downloadurl_query_response".to_string(),
            serde_json::to_string(&response_data).unwrap(),
        )]);
        let canonical = alipay_canonical_parameters(&signed);
        let signature = sign_rsa_sha256(PRIVATE_KEY, canonical.as_bytes()).unwrap();
        let response = serde_json::json!({
            "alipay_data_dataservice_bill_downloadurl_query_response": response_data,
            "sign": signature
        });
        let response_body = response.to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        )
        .unwrap();
        request_body
    });

    let client = AlipayClient::for_test(ureq::Agent::new_with_defaults(), endpoint);
    let credentials = AlipayCredentials {
        app_id: "app-123",
        app_private_key_pem: PRIVATE_KEY,
        alipay_public_key_pem: PUBLIC_KEY,
    };
    let url = client
        .request_bill_url(
            "2026-10-30",
            NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
            &credentials,
        )
        .unwrap();
    let request_body = server.join().unwrap();
    assert!(request_body.contains("method=alipay.data.dataservice.bill.downloadurl.query"));
    assert!(request_body.contains("sign_type=RSA2"));
    assert!(request_body.contains("&sign="));
    assert!(url.starts_with("https://dwbillcenter.alipay.com/downloadBillFile.resource?token="));
}
