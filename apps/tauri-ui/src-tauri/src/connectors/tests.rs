use super::model::{
    ConnectionStatus, ConnectionTestOutcome, ConnectionTestResult, ConnectionTestSummary,
    CredentialInput, ProviderId,
};

#[test]
fn provider_ids_use_stable_wire_names() {
    assert_eq!(
        serde_json::to_string(&ProviderId::WechatPay).unwrap(),
        "\"wechat_pay\""
    );
    assert_eq!(
        serde_json::to_string(&ProviderId::Ifind).unwrap(),
        "\"ifind\""
    );
}

#[test]
fn public_status_contains_only_provider_and_configured_state() {
    let status = ConnectionStatus {
        provider: ProviderId::Binance,
        configured: true,
    };

    assert_eq!(
        serde_json::to_string(&status).unwrap(),
        r#"{"provider":"binance","configured":true}"#
    );
}

#[test]
fn credential_input_requires_all_provider_fields() {
    let missing_secret =
        serde_json::from_str::<CredentialInput>(r#"{"provider":"binance","api_key":"key-only"}"#);
    assert!(missing_secret.is_err());

    let complete = serde_json::from_str::<CredentialInput>(
        r#"{"provider":"binance","api_key":"api-key","secret_key":"api-secret"}"#,
    )
    .unwrap();
    assert!(matches!(complete, CredentialInput::Binance { .. }));
}

#[test]
fn connection_result_serializes_a_safe_typed_summary() {
    let result = ConnectionTestResult {
        provider: ProviderId::WechatPay,
        outcome: ConnectionTestOutcome::Connected,
        summary: Some(ConnectionTestSummary::BillRead {
            bill_date: "2026-10-30".into(),
            downloaded_bytes: 123,
            hash_verified: Some(true),
        }),
        error_code: None,
    };

    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        r#"{"provider":"wechat_pay","outcome":"connected","summary":{"kind":"bill_read","data":{"bill_date":"2026-10-30","downloaded_bytes":123,"hash_verified":true}},"error_code":null}"#
    );
}
