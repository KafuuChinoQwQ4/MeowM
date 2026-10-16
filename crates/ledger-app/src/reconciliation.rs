//! Immutable reconciliation snapshots and explicit versioned actions.
use crate::{RepositoryError, Timestamp};
use ledger_domain::{
    AccountId, Money, ReconciliationId, ReconciliationSession, TransactionId, TransactionKind,
    TransactionStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationStatus {
    Open,
    Reviewed,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationRecord {
    pub session: ReconciliationSession,
    pub status: ReconciliationStatus,
    pub version: u32,
    pub created_at: Timestamp,
    pub reference: Money,
    pub reference_at: Timestamp,
    pub period_from: Option<Timestamp>,
    /// Legacy placeholders have no trustworthy transaction snapshot; only new sessions are actionable.
    pub snapshot_ready: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationEntry {
    pub transaction_id: TransactionId,
    pub occurred_at: Timestamp,
    pub kind: TransactionKind,
    pub status: TransactionStatus,
    pub delta_minor: i64,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub reviewed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationEvent {
    pub action: String,
    pub occurred_at: Timestamp,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationSummary {
    pub record: ReconciliationRecord,
    pub entries: Vec<ReconciliationEntry>,
    pub events: Vec<ReconciliationEvent>,
}
impl ReconciliationSummary {
    pub fn pending_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == TransactionStatus::Pending)
            .count()
    }
    pub fn all_reviewed(&self) -> bool {
        self.entries.iter().all(|e| e.reviewed)
    }
}
/// Every write must validate and commit atomically, serialized against ledger writes.
pub trait ReconciliationRepository {
    fn start_reconciliation(
        &self,
        account: AccountId,
        cutoff: Timestamp,
        actual: Money,
    ) -> Result<ReconciliationId, RepositoryError>;
    fn list_reconciliations(
        &self,
        account: AccountId,
    ) -> Result<Vec<ReconciliationRecord>, RepositoryError>;
    fn reconciliation(
        &self,
        id: ReconciliationId,
    ) -> Result<Option<ReconciliationSummary>, RepositoryError>;
    fn review_reconciliation(
        &self,
        id: ReconciliationId,
        entry: TransactionId,
        reviewed: bool,
        version: u32,
    ) -> Result<(), RepositoryError>;
    fn adjust_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError>;
    fn close_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
    ) -> Result<(), RepositoryError>;
    fn reopen_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
    ) -> Result<(), RepositoryError>;
}
pub struct ReconciliationService<'a, R: ReconciliationRepository> {
    repository: &'a R,
}
impl<'a, R: ReconciliationRepository> ReconciliationService<'a, R> {
    pub fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    pub fn start(
        &self,
        account: AccountId,
        as_of: Timestamp,
        actual_balance: Money,
    ) -> Result<ReconciliationSummary, RepositoryError> {
        let id = self
            .repository
            .start_reconciliation(account, as_of, actual_balance)?;
        self.summary(id)
    }
    pub fn list(&self, account: AccountId) -> Result<Vec<ReconciliationRecord>, RepositoryError> {
        self.repository.list_reconciliations(account)
    }
    pub fn summary(&self, id: ReconciliationId) -> Result<ReconciliationSummary, RepositoryError> {
        self.repository
            .reconciliation(id)?
            .ok_or(RepositoryError::NotFound)
    }
    pub fn mark_reviewed(
        &self,
        id: ReconciliationId,
        entry: TransactionId,
        reviewed: bool,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.repository
            .review_reconciliation(id, entry, reviewed, version)
    }
    pub fn add_adjustment(
        &self,
        id: ReconciliationId,
        version: u32,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        self.repository.adjust_reconciliation(id, version, reason)
    }
    pub fn close(&self, id: ReconciliationId, version: u32) -> Result<(), RepositoryError> {
        self.repository.close_reconciliation(id, version)
    }
    pub fn reopen(&self, id: ReconciliationId, version: u32) -> Result<(), RepositoryError> {
        self.repository.reopen_reconciliation(id, version)
    }
}
