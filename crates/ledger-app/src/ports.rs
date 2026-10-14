//! Storage interfaces consumed by application use cases.

use ledger_domain::{Account, AccountId, Category, CategoryId, Money, Transaction, TransactionId};

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
