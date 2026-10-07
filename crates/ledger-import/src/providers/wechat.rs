//! Synthetic wechat contract only; no real API endpoints or platform access claims.
use super::*;
pub struct WeChatAdapter;
impl ProviderAdapter for WeChatAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::WeChat
    }
    fn source(&self) -> ImportSource {
        ImportSource::WeChat
    }
    fn decode_page(&self, body: &[u8]) -> Result<ProviderPage, ProviderError> {
        decode(body, "transactions")
    }
}
