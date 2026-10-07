//! Synthetic bank contract only; no real API endpoints or platform access claims.
use super::*;
pub struct BankAdapter;
impl ProviderAdapter for BankAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::Bank
    }
    fn source(&self) -> ImportSource {
        ImportSource::BankCsv
    }
    fn decode_page(&self, body: &[u8]) -> Result<ProviderPage, ProviderError> {
        decode(body, "entries")
    }
}
