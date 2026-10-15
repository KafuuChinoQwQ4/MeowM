//! SQLite connection and repository module boundary. Schema and public API stay unchanged.
mod accounts;
mod classification;
mod core_repository;
mod diagnostics;
mod feature_repository;
mod imports;
mod mapping;
mod migrations;
mod reversals;
mod transaction_write;
mod validation;
use mapping::*;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use ledger_app::{
    ClassificationRule, ClassificationRuleRepository, LedgerRepository, RepositoryError, Timestamp,
};
use ledger_domain::{
    Account, AccountId, AccountKind, Category, CategoryId, CurrencyCode, Money, Posting, RuleId,
    SourceEvidence, SourceKind, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use rusqlite::{Connection, OptionalExtension, Transaction as SqlTransaction, params};
use uuid::Uuid;
/// SQLite-backed local ledger repository.
pub struct SqliteLedgerRepository {
    connection: Connection,
}
impl SqliteLedgerRepository {
    /// Opens or creates a ledger database at `path` and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(storage_error)?;
        Self::from_connection(connection)
    }

    /// Creates an isolated in-memory ledger database.
    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, RepositoryError> {
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(storage_error)?;
        let repo = Self { connection };
        repo.apply_migrations()?;
        repo.apply_optional_balance_migration()?;
        repo.apply_feature_migration()?;
        repo.apply_pending_import_migration()?;
        repo.connection.execute_batch("CREATE TABLE IF NOT EXISTS platform_metadata (
            account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
            category TEXT NOT NULL CHECK(category IN ('wallet','bank','securities','exchange','cash','other'))
        );").map_err(storage_error)?;
        Ok(repo)
    }
}
