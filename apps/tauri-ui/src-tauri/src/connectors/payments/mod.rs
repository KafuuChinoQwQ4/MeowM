pub mod alipay;
mod crypto;
mod http;
pub mod wechat;
#[cfg(target_os = "android")]
pub(crate) use http::initialize_android_http_proxy;
pub(crate) use http::{MAX_JSON_BYTES, agent, configured_proxy, map_ureq_error, read_bounded};

use crate::connectors::model::{
    ConnectionTestRequest, ConnectionTestSummary, CredentialInput, ProviderErrorCode, ProviderId,
};

pub fn test(
    request: ConnectionTestRequest,
    credentials: CredentialInput,
) -> Result<ConnectionTestSummary, ProviderErrorCode> {
    let bill_date = request
        .bill_date
        .as_deref()
        .ok_or(ProviderErrorCode::InvalidRequest)?;
    match (request.provider, credentials) {
        (
            ProviderId::WechatPay,
            CredentialInput::WechatPay {
                merchant_id,
                certificate_serial,
                private_key_pem,
            },
        ) => {
            let (_, downloaded_bytes) = wechat::WechatClient::default().read_bill(
                bill_date,
                wechat::WechatCredentials {
                    merchant_id: merchant_id.expose_secret(),
                    certificate_serial: certificate_serial.expose_secret(),
                    private_key_pem: private_key_pem.expose_secret(),
                },
            )?;
            Ok(ConnectionTestSummary::BillRead {
                bill_date: bill_date.to_string(),
                downloaded_bytes,
                hash_verified: Some(true),
            })
        }
        (
            ProviderId::Alipay,
            CredentialInput::Alipay {
                app_id,
                app_private_key_pem,
                alipay_public_key_pem,
            },
        ) => {
            let (_, downloaded_bytes) = alipay::AlipayClient::default().read_bill(
                bill_date,
                alipay::AlipayCredentials {
                    app_id: app_id.expose_secret(),
                    app_private_key_pem: app_private_key_pem.expose_secret(),
                    alipay_public_key_pem: alipay_public_key_pem.expose_secret(),
                },
            )?;
            Ok(ConnectionTestSummary::BillRead {
                bill_date: bill_date.to_string(),
                downloaded_bytes,
                hash_verified: None,
            })
        }
        _ => Err(ProviderErrorCode::InvalidCredentials),
    }
}

#[cfg(test)]
mod tests;
