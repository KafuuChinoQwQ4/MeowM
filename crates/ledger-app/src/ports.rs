//! Storage interfaces consumed by application use cases.

use ledger_domain::{
    Account, AccountId, Category, CategoryId, Money, SourceEvidence, Transaction, TransactionId,
};

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

    fn balance_as_of(
        &self,
        account_id: AccountId,
        cutoff_utc: Timestamp,
    ) -> Result<Money, RepositoryError>;
}

/// One stored transaction and its import provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    pub transaction: Transaction,
    pub evidence: SourceEvidence,
}

/// Persistence boundary for atomic imports. Implementations must commit a batch
/// as one unit and return zero for a batch that has already been committed.
pub trait ImportRepository {
    fn list_import_records(&self) -> Result<Vec<ImportRecord>, RepositoryError>;
    fn confirm_import_batch(
        &self,
        batch_id: uuid::Uuid,
        records: Vec<ImportRecord>,
    ) -> Result<usize, RepositoryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletionScope {
    Transaction(TransactionId),
    ImportBatch(uuid::Uuid),
    AllLocal,
}

pub trait DeletionRepository {
    fn delete_scope(&self, scope: DeletionScope) -> Result<(), RepositoryError>;
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

/// Device-local provider consent/credential namespace, independent of ledger accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    WeChat,
    Alipay,
    Bank,
}
impl ProviderId {
    pub fn key(self) -> &'static str {
        match self {
            Self::WeChat => "wechat",
            Self::Alipay => "alipay",
            Self::Bank => "bank",
        }
    }
}
/// No Display/serialization/Clone implementation. Copies remain the caller's responsibility.
pub struct SecretToken(zeroize::Zeroizing<Vec<u8>>);
impl SecretToken {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(zeroize::Zeroizing::new(bytes))
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}
impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialError;
/// Android implementations must encrypt bytes with a non-exportable Keystore key.
/// Desktop has no insecure fallback. Credentials never enter repository or backup ports.
pub trait CredentialStore {
    fn load(&mut self, provider: ProviderId) -> Result<Option<SecretToken>, CredentialError>;
    fn save(&mut self, provider: ProviderId, token: &SecretToken) -> Result<(), CredentialError>;
    fn delete(&mut self, provider: ProviderId) -> Result<(), CredentialError>;
}
