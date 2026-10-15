//! Storage interfaces consumed by application use cases.

use ledger_domain::{
    Account, AccountId, Category, CategoryId, Money, SourceEvidence, SourceKind, Transaction,
    TransactionEdit, TransactionId,
};
use uuid::Uuid;

use crate::{RepositoryError, Timestamp, classification::ClassificationRule};

/// Persistence boundary for account and transaction workflows.
///
/// `insert_*` methods reject duplicate identifiers; `update_*` methods replace an
/// existing record and fail with `NotFound` when the identifier is absent.
pub trait LedgerRepository {
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError>;
    fn get_account(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;
    fn list_accounts(&self) -> Result<Vec<Account>, RepositoryError>;
    fn update_account(&self, account: Account) -> Result<(), RepositoryError>;

    fn insert_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError>;
    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError>;
    fn list_transactions(&self) -> Result<Vec<Transaction>, RepositoryError>;
    fn update_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError>;
    /// Confirmed rows in a half-open UTC range; storage can avoid loading unrelated history.
    fn report_transactions(
        &self,
        from: Timestamp,
        until: Timestamp,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        Ok(self
            .list_transactions()?
            .into_iter()
            .filter(|t| {
                t.status == ledger_domain::TransactionStatus::Confirmed
                    && t.occurred_at >= from
                    && t.occurred_at < until
            })
            .collect())
    }

    /// Allows compatible repositories to supply explicit inverse report contributions.
    fn report_override(
        &self,
        _id: TransactionId,
    ) -> Result<Option<ledger_domain::ReportEffect>, RepositoryError> {
        Ok(None)
    }

    /// Returns None when no reference balance is set (distinct from known zero).
    /// Confirmed postings in (reference, cutoff] are added; for earlier cutoffs,
    /// postings in (cutoff, reference] are subtracted. Uses microsecond precision.
    fn balance_as_of(
        &self,
        account_id: AccountId,
        cutoff_utc: Timestamp,
    ) -> Result<Option<Money>, RepositoryError>;
}

/// Persistence boundary for user-managed categories and classification rules.
///
/// This is deliberately separate from [`LedgerRepository`] so existing ledger
/// implementations can add classification support incrementally.
pub trait ClassificationRuleRepository {
    fn insert_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError>;
    fn list_rules(&self) -> Result<Vec<ClassificationRule>, RepositoryError>;
    fn update_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError>;

    fn insert_category(&self, category: Category) -> Result<(), RepositoryError>;
    fn get_category(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError>;
    fn update_category(&self, category: Category) -> Result<(), RepositoryError>;
    fn list_categories(&self) -> Result<Vec<Category>, RepositoryError>;
}

/// Optional atomic feature capability. Implementations must validate and mutate
/// within a single transaction; page limits are 1..=200, newest first then ID.
pub trait FeatureRepository: LedgerRepository {
    fn query_transactions(
        &self,
        query: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, RepositoryError>;
    fn confirm_bulk(&self, request: ledger_domain::BulkConfirmation)
    -> Result<(), RepositoryError>;
    /// Confirms and applies user edits as one transaction. The default keeps
    /// compatibility for repositories that only implement the original batch API.
    fn confirm_bulk_with_edits(
        &self,
        request: ledger_domain::BulkConfirmation,
        edits: Vec<TransactionEdit>,
    ) -> Result<(), RepositoryError> {
        if !edits.is_empty() {
            return Err(RepositoryError::InvalidData(
                "repository does not support atomic confirmation edits".into(),
            ));
        }
        self.confirm_bulk(request)
    }
    fn ignore_bulk(&self, ids: &[TransactionId]) -> Result<(), RepositoryError> {
        if ids.len() > 1 {
            return Err(RepositoryError::InvalidData(
                "repository does not support atomic bulk ignore".into(),
            ));
        }
        if let Some(id) = ids.first() {
            self.ignore_atomic(*id)?;
        }
        Ok(())
    }
    fn restore_pending_atomic(&self, id: TransactionId) -> Result<(), RepositoryError>;
    fn ignore_atomic(&self, id: TransactionId) -> Result<(), RepositoryError>;
    /// Returns the existing reversal on retries, regardless of the proposed new ID.
    fn reverse_atomic(
        &self,
        original: TransactionId,
        reversal: TransactionId,
        at: Timestamp,
    ) -> Result<TransactionId, RepositoryError>;
    fn edit_atomic(&self, edit: ledger_domain::TransactionEdit) -> Result<(), RepositoryError>;
    fn source_evidence(
        &self,
        id: TransactionId,
    ) -> Result<Vec<ledger_domain::SourceEvidence>, RepositoryError>;
    fn privacy_settings(&self) -> Result<ledger_domain::PrivacySettings, RepositoryError>;
    fn save_privacy_settings(
        &self,
        settings: ledger_domain::PrivacySettings,
    ) -> Result<(), RepositoryError>;
}

/// Storage boundary for creating pending import candidates and their evidence.
/// Implementations must insert the complete batch atomically.
pub trait ImportRepository: FeatureRepository {
    fn insert_pending_batch(
        &self,
        batch_id: Uuid,
        items: Vec<(Transaction, SourceEvidence)>,
    ) -> Result<Vec<TransactionId>, RepositoryError>;
    fn find_source_transaction(
        &self,
        source: SourceKind,
        external_transaction_id: &str,
    ) -> Result<Option<TransactionId>, RepositoryError>;
}

/// Atomic budget persistence with scope uniqueness and enabled-category validation.
pub trait BudgetRepository: LedgerRepository + ClassificationRuleRepository {
    fn list_budgets(&self) -> Result<Vec<crate::BudgetPlan>, RepositoryError>;
    fn save_budget(&self, budget: crate::BudgetPlan, create: bool) -> Result<(), RepositoryError>;
    fn archive_budget(&self, id: ledger_domain::BudgetId) -> Result<(), RepositoryError>;
}

/// Notification receipts, settings, and candidates must be committed together.
pub trait NotificationRepository: FeatureRepository {
    fn notification_settings(
        &self,
    ) -> Result<Vec<crate::notifications::NotificationSettings>, RepositoryError>;
    fn save_notification_settings(
        &self,
        settings: crate::notifications::NotificationSettings,
    ) -> Result<(), RepositoryError>;
    fn notification_status(
        &self,
    ) -> Result<Vec<crate::notifications::CaptureStatus>, RepositoryError>;
    fn record_notification(
        &self,
        record: crate::notifications::NotificationRecord,
    ) -> Result<crate::notifications::CaptureResult, RepositoryError>;
}
