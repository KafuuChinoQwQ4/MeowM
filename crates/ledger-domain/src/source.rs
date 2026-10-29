//! Import-source evidence and provenance identifiers.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Alipay,
    WeChat,
    /// Bank statement import.
    Bank,
    Notification,
    ScreenCapture,
    Recurring,
    Manual,
}

/// Provenance metadata only; notification bodies must never be retained here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvidence {
    pub source_kind: SourceKind,
    pub external_transaction_id: Option<String>,
    /// Merchant order number, kept separate from the payment provider's transaction ID.
    pub merchant_order_id: Option<String>,
    /// Explicit original order reference on a refund, in the same provider's ID space.
    pub original_external_transaction_id: Option<String>,
    pub import_batch_id: Uuid,
    /// Digest of normalized source fields, not the original source content.
    pub source_fields_digest: [u8; 32],
}
