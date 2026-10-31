use crate::connectors::model::ProviderErrorCode;
use std::io::{ErrorKind, Read};
use std::time::Duration;
use zeroize::Zeroizing;

pub const MAX_JSON_BYTES: u64 = 512 * 1024;
pub const MAX_BILL_BYTES: u64 = 32 * 1024 * 1024;

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(0)
        .max_redirects_will_error(true)
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn read_bounded(
    response: &mut ureq::http::Response<ureq::Body>,
    max_bytes: u64,
) -> Result<Zeroizing<Vec<u8>>, ProviderErrorCode> {
    let mut bytes = Zeroizing::new(Vec::new());
    response
        .body_mut()
        .as_reader()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            if error.kind() == ErrorKind::TimedOut {
                ProviderErrorCode::Timeout
            } else {
                ProviderErrorCode::NetworkFailure
            }
        })?;
    if bytes.len() as u64 > max_bytes {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(bytes)
}

pub fn map_http_status(status: u16) -> ProviderErrorCode {
    match status {
        401 => ProviderErrorCode::InvalidCredentials,
        403 => ProviderErrorCode::PermissionDenied,
        404 => ProviderErrorCode::BillUnavailable,
        429 => ProviderErrorCode::RateLimited,
        408 | 504 => ProviderErrorCode::Timeout,
        500..=599 => ProviderErrorCode::ProviderFailure,
        _ => ProviderErrorCode::InvalidResponse,
    }
}

pub fn map_ureq_error(error: ureq::Error) -> ProviderErrorCode {
    match error {
        ureq::Error::Timeout(_) => ProviderErrorCode::Timeout,
        ureq::Error::TooManyRedirects => ProviderErrorCode::NetworkFailure,
        _ => ProviderErrorCode::NetworkFailure,
    }
}
