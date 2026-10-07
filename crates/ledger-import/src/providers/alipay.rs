//! Synthetic alipay contract only; no real API endpoints or platform access claims.
use super::*;
pub struct AlipayAdapter;
impl ProviderAdapter for AlipayAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::Alipay
    }
    fn source(&self) -> ImportSource {
        ImportSource::Alipay
    }
    fn decode_page(&self, body: &[u8]) -> Result<ProviderPage, ProviderError> {
        decode(body, "payments")
    }
}
