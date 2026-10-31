mod binance;
mod ifind;
mod okx;

use crate::connectors::model::{
    ConnectionTestRequest, ConnectionTestSummary, CredentialInput, ProviderErrorCode, ProviderId,
};
use ureq::Body;
use ureq::http::Response;

use crate::connectors::payments::{MAX_JSON_BYTES, agent, map_ureq_error, read_bounded};
pub use ifind::IfindTokenCache;
use serde_json::Value;

pub(crate) fn test(
    request: ConnectionTestRequest,
    credentials: CredentialInput,
    ifind_tokens: &IfindTokenCache,
) -> Result<ConnectionTestSummary, ProviderErrorCode> {
    match (request.provider, credentials) {
        (
            ProviderId::Binance,
            CredentialInput::Binance {
                api_key,
                secret_key,
            },
        ) => {
            let balances =
                binance::BinanceClient::default().read_balances(&binance::BinanceCredentials {
                    api_key: api_key.expose_secret(),
                    secret_key: secret_key.expose_secret(),
                })?;
            Ok(ConnectionTestSummary::BinanceBalances { balances })
        }
        (
            ProviderId::Okx,
            CredentialInput::Okx {
                api_key,
                secret_key,
                passphrase,
            },
        ) => {
            let balances = okx::OkxClient::default().read_balances(&okx::OkxCredentials {
                api_key: api_key.expose_secret(),
                secret_key: secret_key.expose_secret(),
                passphrase: passphrase.expose_secret(),
            })?;
            Ok(ConnectionTestSummary::OkxBalances { balances })
        }
        (ProviderId::Ifind, CredentialInput::Ifind { refresh_token }) => {
            let symbol = request.symbol.as_deref().unwrap_or("000001.SZ");
            let quote = ifind::IfindClient::default().read_quote(
                refresh_token.expose_secret(),
                symbol,
                ifind_tokens,
            )?;
            Ok(ConnectionTestSummary::IfindQuote { quote })
        }
        _ => Err(ProviderErrorCode::InvalidCredentials),
    }
}

fn parse_json_response(
    response: &mut Response<Body>,
    status_error: fn(u16) -> ProviderErrorCode,
) -> Result<Value, ProviderErrorCode> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }
    let bytes = read_bounded(response, MAX_JSON_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| ProviderErrorCode::InvalidResponse)
}

fn request_error(error: ureq::Error) -> ProviderErrorCode {
    map_ureq_error(error)
}

fn map_exchange_http_status(status: u16) -> ProviderErrorCode {
    match status {
        400 => ProviderErrorCode::InvalidRequest,
        401 => ProviderErrorCode::InvalidCredentials,
        403 => ProviderErrorCode::PermissionDenied,
        418 | 429 => ProviderErrorCode::RateLimited,
        408 | 504 => ProviderErrorCode::Timeout,
        500..=599 => ProviderErrorCode::ProviderFailure,
        _ => ProviderErrorCode::InvalidResponse,
    }
}

fn map_ifind_http_status(status: u16) -> ProviderErrorCode {
    match status {
        400 => ProviderErrorCode::InvalidRequest,
        401 => ProviderErrorCode::InvalidCredentials,
        403 => ProviderErrorCode::PermissionDenied,
        429 => ProviderErrorCode::RateLimited,
        408 | 504 => ProviderErrorCode::Timeout,
        500..=599 => ProviderErrorCode::ProviderFailure,
        _ => ProviderErrorCode::InvalidResponse,
    }
}

fn default_agent() -> ureq::Agent {
    agent()
}

#[cfg(test)]
mod tests;
