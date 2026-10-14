//! Import-source evidence and provenance identifiers.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Alipay,
    WeChat,
    /// Bank statement import.
    Bank,
    Notification,
    Manual,
}

/// Provenance metadata only; notification bodies must never be retained here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvidence {
    pub source_kind: SourceKind,
    pub external_transaction_id: Option<String>,
    pub import_batch_id: Uuid,
    /// Digest of normalized source fields, not the original source content.
    pub source_fields_digest: [u8; 32],
}
