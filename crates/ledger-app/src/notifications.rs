//! Notification capture contracts. No raw notification text belongs in storage.
use ledger_domain::{AccountId, SourceEvidence, Transaction, TransactionId};

pub const NOTIFICATION_PACKAGES: [&str; 2] = ["com.tencent.mm", "com.eg.android.AlipayGphone"];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSettings {
    pub package_name: String,
    pub enabled: bool,
    pub account_id: Option<AccountId>,
}
impl NotificationSettings {
    pub fn disabled(package: &str) -> Self {
        Self {
            package_name: package.into(),
            enabled: false,
            account_id: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureResult {
    pub code: String,
    pub transaction_id: Option<TransactionId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureStatus {
    pub package_name: String,
    pub code: String,
    pub received_at_ms: i64,
    pub transaction_id: Option<TransactionId>,
}
/// Fully normalized input for a single atomic receipt/candidate write.
pub struct NotificationRecord {
    pub package_name: String,
    pub identity: String,
    pub digest: [u8; 32],
    pub candidate: Option<(Transaction, SourceEvidence)>,
    pub code: String,
    /// Rechecked by storage to prevent capture racing a source switch.
    pub settings: NotificationSettings,
}
