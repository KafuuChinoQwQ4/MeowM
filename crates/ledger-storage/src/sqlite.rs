//! SQLite connection, migrations, and repository implementation.

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

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");
const CLASSIFICATION_MIGRATION: &str = include_str!("../migrations/0002_classification_rules.sql");

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

    pub fn platform_category(&self, id: AccountId) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT category FROM platform_metadata WHERE account_id=?1",
                [id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }

    /// Stores platform identity and classification together, preserving postings and balances.
    pub fn save_platform(
        &self,
        id: AccountId,
        name: &str,
        category: &str,
        create: bool,
    ) -> Result<(), RepositoryError> {
        if !["wallet", "bank", "securities", "exchange", "cash", "other"].contains(&category) {
            return Err(RepositoryError::InvalidData(
                "invalid platform category".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let id = id.as_uuid().to_string();
        let duplicate: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE display_name=?1 AND id<>?2)",
                params![name, id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if duplicate {
            return Err(RepositoryError::InvalidData("平台名称已存在".into()));
        }
        if create {
            tx.execute("INSERT INTO accounts(id,display_name,kind,currency_code,currency_exponent) VALUES (?1,?2,'asset','CNY',2)", params![id,name]).map_err(storage_error)?;
        } else if tx
            .execute(
                "UPDATE accounts SET display_name=?2 WHERE id=?1",
                params![id, name],
            )
            .map_err(storage_error)?
            == 0
        {
            return Err(RepositoryError::InvalidData("平台不存在".into()));
        }
        tx.execute("INSERT INTO platform_metadata(account_id,category) VALUES (?1,?2) ON CONFLICT(account_id) DO UPDATE SET category=excluded.category", params![id,category]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    fn apply_migrations(&self) -> Result<(), RepositoryError> {
        let version_table_exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version')",
            [],
            |row| row.get(0),
        ).map_err(storage_error)?;
        if version_table_exists {
            let (version_count, recorded_version): (i64, Option<i64>) = self
                .connection
                .query_row(
                    "SELECT COUNT(*), MAX(version) FROM schema_version",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(storage_error)?;
            let versions: Vec<i64> = self
                .connection
                .prepare("SELECT version FROM schema_version ORDER BY version")
                .map_err(storage_error)?
                .query_map([], |r| r.get(0))
                .map_err(storage_error)?
                .collect::<Result<_, _>>()
                .map_err(storage_error)?;
            if let Some(v) = recorded_version
                && v <= 5
                && versions != (1..=v).collect::<Vec<_>>()
            {
                return Err(RepositoryError::Storage(
                    "database schema history is invalid".into(),
                ));
            }
            match (version_count, recorded_version) {
                (1, Some(1)) => {
                    self.apply_classification_migration(1)?;
                    return Ok(());
                }
                (2, Some(2)) | (3, Some(3)) | (4, Some(4)) | (5, Some(5)) => return Ok(()),
                (_, Some(version)) if version > 5 => {
                    return Err(RepositoryError::Storage(format!(
                        "database schema version {version} is newer than supported version 5"
                    )));
                }
                _ => {
                    return Err(RepositoryError::Storage(
                        "database schema version is empty or invalid".into(),
                    ));
                }
            }
        }

        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(INITIAL_MIGRATION).map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (1, ?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.execute_batch(CLASSIFICATION_MIGRATION)
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (2, ?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    fn apply_classification_migration(&self, current_version: i64) -> Result<(), RepositoryError> {
        if current_version != 1 {
            return Err(RepositoryError::Storage(format!(
                "cannot migrate schema version {current_version}"
            )));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(CLASSIFICATION_MIGRATION)
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (2, ?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    fn apply_optional_balance_migration(&self) -> Result<(), RepositoryError> {
        let version = self.schema_version()?;
        if version >= 3 {
            return Ok(());
        }
        // Rebuild the referenced table atomically without rewriting posting foreign keys.
        self.connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(storage_error)?;
        let result = (|| {
            let tx = self
                .connection
                .unchecked_transaction()
                .map_err(storage_error)?;
            tx.execute_batch(include_str!("../migrations/0003_optional_balance.sql"))
                .map_err(storage_error)?;
            let violations: i64 = tx
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .map_err(storage_error)?;
            if violations != 0 {
                return Err(RepositoryError::InvalidData(
                    "foreign key violations during migration".into(),
                ));
            }
            if version < 3 {
                tx.execute(
                    "INSERT INTO schema_version VALUES (3, ?1)",
                    [SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as i64],
                )
                .map_err(storage_error)?;
            }
            tx.commit().map_err(storage_error)
        })();
        self.connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(storage_error)?;
        result
    }

    fn apply_feature_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 4 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../migrations/0004_nine_features.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(4,?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    fn apply_pending_import_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 5 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../migrations/0005_pending_import.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(5,?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    /// Returns the highest applied schema version.
    pub fn schema_version(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .map_err(storage_error)
    }

    /// Inserts an import batch that may be referenced by source evidence.
    pub fn create_import_batch(&self, id: Uuid) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO import_batches(id) VALUES (?1)",
                [id.to_string()],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    /// Attaches provenance metadata without retaining notification or bill text.
    pub fn insert_source_evidence(
        &self,
        transaction_id: TransactionId,
        evidence: SourceEvidence,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO source_evidence(\
                    transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    transaction_id.as_uuid().to_string(),
                    source_kind_str(evidence.source_kind),
                    evidence.external_transaction_id,
                    evidence.import_batch_id.to_string(),
                    evidence.source_fields_digest.as_slice(),
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    /// Atomically inserts a transaction, its postings, and optional import provenance.
    /// When evidence is present, its import batch is created in the same SQL transaction.
    pub fn insert_transaction_with_evidence(
        &self,
        transaction: Transaction,
        evidence: Option<SourceEvidence>,
    ) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        Self::write_transaction_with_evidence(&tx, &transaction, evidence.as_ref())?;
        tx.commit().map_err(storage_error)
    }

    /// Counts stored postings; useful for diagnostics and integrity checks.
    pub fn posting_count(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM postings", [], |row| row.get(0))
            .map_err(storage_error)
    }

    /// Counts source evidence records.
    pub fn source_evidence_count(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM source_evidence", [], |row| row.get(0))
            .map_err(storage_error)
    }

    /// Counts stored import batches; useful for integrity checks.
    pub fn import_batch_count(&self) -> Result<i64, RepositoryError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM import_batches", [], |row| row.get(0))
            .map_err(storage_error)
    }

    fn write_transaction_with_evidence(
        tx: &SqlTransaction<'_>,
        transaction: &Transaction,
        evidence: Option<&SourceEvidence>,
    ) -> Result<(), RepositoryError> {
        Self::write_transaction(tx, transaction, true)?;
        if let Some(evidence) = evidence {
            tx.execute(
                "INSERT INTO import_batches(id) VALUES (?1) ON CONFLICT(id) DO NOTHING",
                [evidence.import_batch_id.to_string()],
            )
            .map_err(storage_error)?;
            tx.execute(
                "INSERT INTO source_evidence(\
                    transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    transaction.id.as_uuid().to_string(),
                    source_kind_str(evidence.source_kind),
                    evidence.external_transaction_id,
                    evidence.import_batch_id.to_string(),
                    evidence.source_fields_digest.as_slice(),
                ],
            )
            .map_err(map_insert_error)?;
        }
        Ok(())
    }

    fn write_transaction(
        tx: &SqlTransaction<'_>,
        transaction: &Transaction,
        insert: bool,
    ) -> Result<(), RepositoryError> {
        if transaction.kind == TransactionKind::Refund
            && let Some(parent) = transaction.original_transaction_id
        {
            let reversed: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM transaction_metadata WHERE reversal_of=?1)",
                    [parent.as_uuid().to_string()],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if reversed {
                return Err(RepositoryError::InvalidData(
                    "refund parent was reversed".into(),
                ));
            }
        }
        let values = transaction_values(transaction);
        let changed = if insert {
            tx.execute(
                "INSERT INTO transactions(id, occurred_at_utc_micros, kind, status, category_id, original_transaction_id, report_amount_minor, report_currency_code, report_currency_exponent, merchant, description)\
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![values.id, values.occurred_at, values.kind, values.status, values.category_id, values.original_transaction_id, values.report_amount_minor, values.report_currency_code, values.report_currency_exponent, values.merchant, values.description],
            )
        } else {
            tx.execute(
                "UPDATE transactions SET occurred_at_utc_micros = ?2, kind = ?3, status = ?4, category_id = ?5, original_transaction_id = ?6, report_amount_minor = ?7, report_currency_code = ?8, report_currency_exponent = ?9, merchant = ?10, description = ?11 WHERE id = ?1",
                params![values.id, values.occurred_at, values.kind, values.status, values.category_id, values.original_transaction_id, values.report_amount_minor, values.report_currency_code, values.report_currency_exponent, values.merchant, values.description],
            )
        }
        .map_err(map_insert_error)?;

        if !insert && changed == 0 {
            return Err(RepositoryError::NotFound);
        }
        tx.execute(
            "DELETE FROM postings WHERE transaction_id = ?1",
            [transaction.id.as_uuid().to_string()],
        )
        .map_err(storage_error)?;
        for (ordinal, posting) in transaction.postings.iter().enumerate() {
            tx.execute(
                "INSERT INTO postings(transaction_id, ordinal, account_id, delta_minor, currency_code, currency_exponent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    transaction.id.as_uuid().to_string(),
                    ordinal as i64,
                    posting.account_id.as_uuid().to_string(),
                    posting.delta.minor_units(),
                    posting.delta.currency().code(),
                    posting.delta.currency().minor_unit_exponent(),
                ],
            )
            .map_err(map_insert_error)?;
        }
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,?3,?4)",params![Uuid::new_v4().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64,if insert {"created"} else {"updated"},transaction.id.as_uuid().to_string()]).map_err(storage_error)?;
        tx.execute("INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros,edit_version) VALUES(?1,?2,0) ON CONFLICT(transaction_id) DO UPDATE SET updated_at_utc_micros=excluded.updated_at_utc_micros,edit_version=edit_version+1",params![transaction.id.as_uuid().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64]).map_err(storage_error)?;
        Ok(())
    }
}

impl LedgerRepository for SqliteLedgerRepository {
    fn report_override(
        &self,
        id: TransactionId,
    ) -> Result<Option<ledger_domain::ReportEffect>, RepositoryError> {
        self.connection.query_row("SELECT m.report_income_minor,m.report_expense_minor FROM transaction_metadata m JOIN transactions t ON t.id=m.transaction_id WHERE m.transaction_id=?1 AND t.status='confirmed' AND m.report_income_minor IS NOT NULL",[id.as_uuid().to_string()],|r|Ok(ledger_domain::ReportEffect{income_cny_minor:r.get(0)?,expense_cny_minor:r.get(1)?})).optional().map_err(storage_error)
    }
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO accounts(id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account.id.as_uuid().to_string(), account.display_name,
                    account_kind_str(account.kind), account.currency.code(),
                    account.currency.minor_unit_exponent(), account.opening_balance.as_ref().map(Money::minor_units),
                    account.opening_balance_effective_at.as_ref().map(DateTime::timestamp_micros),
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn get_account(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros FROM accounts WHERE id = ?1",
                [id.as_uuid().to_string()],
                account_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    fn list_accounts(&self) -> Result<Vec<Account>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros FROM accounts ORDER BY rowid",
        ).map_err(storage_error)?;
        statement
            .query_map([], account_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }

    fn update_account(&self, account: Account) -> Result<(), RepositoryError> {
        let changed = self.connection.execute(
            "UPDATE accounts SET display_name = ?2, kind = ?3, currency_code = ?4, currency_exponent = ?5, opening_balance_minor = ?6, opening_balance_effective_at_utc_micros = ?7 WHERE id = ?1",
            params![
                account.id.as_uuid().to_string(), account.display_name,
                account_kind_str(account.kind), account.currency.code(),
                account.currency.minor_unit_exponent(), account.opening_balance.as_ref().map(Money::minor_units),
                account.opening_balance_effective_at.as_ref().map(DateTime::timestamp_micros),
            ],
        ).map_err(map_insert_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn insert_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        self.insert_transaction_with_evidence(transaction, None)
    }

    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        let row = self.connection.query_row(
            "SELECT id, occurred_at_utc_micros, kind, status, category_id, original_transaction_id, report_amount_minor, report_currency_code, report_currency_exponent, merchant, description FROM transactions WHERE id = ?1",
            [id.as_uuid().to_string()],
            transaction_from_row,
        ).optional().map_err(storage_error)?;
        let Some((mut transaction, report_data)) = row else {
            return Ok(None);
        };
        transaction.report_amount_cny = report_data?;
        let mut statement = self.connection.prepare(
            "SELECT account_id, delta_minor, currency_code, currency_exponent FROM postings WHERE transaction_id = ?1 ORDER BY ordinal",
        ).map_err(storage_error)?;
        transaction.postings = statement
            .query_map([id.as_uuid().to_string()], posting_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        Ok(Some(transaction))
    }

    fn list_transactions(&self) -> Result<Vec<Transaction>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM transactions ORDER BY occurred_at_utc_micros, rowid")
            .map_err(storage_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        ids.into_iter()
            .map(|id| {
                let id = parse_uuid(&id)
                    .map(TransactionId::from_uuid)
                    .map_err(RepositoryError::InvalidData)?;
                self.get_transaction(id)?.ok_or_else(|| {
                    RepositoryError::Storage("transaction disappeared while listing".into())
                })
            })
            .collect()
    }

    fn update_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        Self::write_transaction(&tx, &transaction, false)?;
        tx.commit().map_err(storage_error)
    }

    fn balance_as_of(
        &self,
        account_id: AccountId,
        cutoff_utc: Timestamp,
    ) -> Result<Option<Money>, RepositoryError> {
        let account = self
            .get_account(account_id)?
            .ok_or(RepositoryError::NotFound)?;
        let (Some(mut balance), Some(reference)) = (
            account.opening_balance,
            account.opening_balance_effective_at,
        ) else {
            return Ok(None);
        };
        let reference = reference.timestamp_micros();
        let cutoff = cutoff_utc.timestamp_micros();
        let (lower, upper, direction) = if cutoff >= reference {
            (reference, cutoff, 1_i64)
        } else {
            (cutoff, reference, -1_i64)
        };
        let mut statement = self.connection.prepare(
            "SELECT p.delta_minor, p.currency_code, p.currency_exponent FROM postings p JOIN transactions t ON t.id = p.transaction_id WHERE p.account_id = ?1 AND t.status = 'confirmed' AND t.occurred_at_utc_micros > ?2 AND t.occurred_at_utc_micros <= ?3 ORDER BY t.occurred_at_utc_micros, t.rowid, p.ordinal",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![account_id.as_uuid().to_string(), lower, upper],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u8>(2)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        for row in rows {
            let (minor, code, exponent) = row.map_err(storage_error)?;
            let money = Money::new(
                minor,
                CurrencyCode::new(&code, exponent)
                    .map_err(|e| RepositoryError::InvalidData(e.to_string()))?,
            );
            balance = if direction > 0 {
                balance.checked_add(money)
            } else {
                balance.checked_sub(money)
            }
            .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
        }
        Ok(Some(balance))
    }
}

impl ClassificationRuleRepository for SqliteLedgerRepository {
    fn insert_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let enabled: Option<i64> = self
            .connection
            .query_row(
                "SELECT enabled FROM categories WHERE id = ?1",
                [rule.category_id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        match enabled {
            None => return Err(RepositoryError::NotFound),
            Some(0) => {
                return Err(RepositoryError::InvalidData(
                    "disabled category cannot receive new rules".into(),
                ));
            }
            Some(1) => {}
            Some(value) => {
                return Err(RepositoryError::InvalidData(format!(
                    "invalid category enabled value {value}"
                )));
            }
        }
        self.connection
            .execute(
                "INSERT INTO classification_rules(id, priority, merchant_contains, description_contains, category_id, enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rule.id.as_uuid().to_string(),
                    i64::from(rule.priority),
                    rule.merchant_contains,
                    rule.description_contains,
                    rule.category_id.as_uuid().to_string(),
                    if rule.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn list_rules(&self) -> Result<Vec<ClassificationRule>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, priority, merchant_contains, description_contains, category_id, enabled FROM classification_rules ORDER BY priority, id",
        ).map_err(storage_error)?;
        statement
            .query_map([], classification_rule_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }

    fn update_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let rule_id = rule.id.as_uuid().to_string();
        let category_id = rule.category_id.as_uuid().to_string();
        let existing_category_id: Option<String> = self
            .connection
            .query_row(
                "SELECT category_id FROM classification_rules WHERE id = ?1",
                [&rule_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let Some(existing_category_id) = existing_category_id else {
            return Err(RepositoryError::NotFound);
        };
        let enabled: Option<i64> = self
            .connection
            .query_row(
                "SELECT enabled FROM categories WHERE id = ?1",
                [&category_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        match enabled {
            None => return Err(RepositoryError::NotFound),
            Some(0) if rule.enabled || existing_category_id != category_id => {
                return Err(RepositoryError::InvalidData(
                    "disabled category cannot receive active rules".into(),
                ));
            }
            Some(0 | 1) => {}
            Some(value) => {
                return Err(RepositoryError::InvalidData(format!(
                    "invalid category enabled value {value}"
                )));
            }
        }
        let changed = self
            .connection
            .execute(
                "UPDATE classification_rules SET priority = ?2, merchant_contains = ?3, description_contains = ?4, category_id = ?5, enabled = ?6 WHERE id = ?1",
                params![
                    rule_id,
                    i64::from(rule.priority),
                    rule.merchant_contains,
                    rule.description_contains,
                    category_id,
                    if rule.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn insert_category(&self, category: Category) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO categories(id, name, enabled) VALUES (?1, ?2, ?3)",
                params![
                    category.id.as_uuid().to_string(),
                    category.name,
                    if category.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn get_category(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, name, enabled FROM categories WHERE id = ?1",
                [id.as_uuid().to_string()],
                category_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    fn update_category(&self, category: Category) -> Result<(), RepositoryError> {
        let changed = self
            .connection
            .execute(
                "UPDATE categories SET name = ?2, enabled = ?3 WHERE id = ?1",
                params![
                    category.id.as_uuid().to_string(),
                    category.name,
                    if category.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn list_categories(&self) -> Result<Vec<Category>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name, enabled FROM categories ORDER BY rowid")
            .map_err(storage_error)?;
        statement
            .query_map([], category_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }
}

struct TransactionSqlValues {
    id: String,
    occurred_at: i64,
    kind: &'static str,
    status: &'static str,
    category_id: Option<String>,
    original_transaction_id: Option<String>,
    report_amount_minor: Option<i64>,
    report_currency_code: Option<String>,
    report_currency_exponent: Option<u8>,
    merchant: Option<String>,
    description: Option<String>,
}

fn transaction_values(transaction: &Transaction) -> TransactionSqlValues {
    let report = transaction.report_amount_cny.as_ref();
    TransactionSqlValues {
        id: transaction.id.as_uuid().to_string(),
        occurred_at: transaction.occurred_at.timestamp_micros(),
        kind: transaction_kind_str(transaction.kind),
        status: transaction_status_str(transaction.status),
        category_id: transaction.category_id.map(|id| id.as_uuid().to_string()),
        original_transaction_id: transaction
            .original_transaction_id
            .map(|id| id.as_uuid().to_string()),
        report_amount_minor: report.map(Money::minor_units),
        report_currency_code: report.map(|m| m.currency().code().to_owned()),
        report_currency_exponent: report.map(|m| m.currency().minor_unit_exponent()),
        merchant: transaction.merchant.clone(),
        description: transaction.description.clone(),
    }
}

fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let id: String = row.get(0)?;
    let kind: String = row.get(2)?;
    let code: String = row.get(3)?;
    let exponent: u8 = row.get(4)?;
    let amount: Option<i64> = row.get(5)?;
    let effective: Option<i64> = row.get(6)?;
    Ok(Account {
        id: AccountId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        display_name: row.get(1)?,
        kind: parse_account_kind(&kind).map_err(to_sql_conversion_error)?,
        currency: CurrencyCode::new(&code, exponent)
            .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        opening_balance: amount
            .map(|amount| {
                CurrencyCode::new(&code, exponent).map(|currency| Money::new(amount, currency))
            })
            .transpose()
            .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        opening_balance_effective_at: effective
            .map(parse_timestamp)
            .transpose()
            .map_err(to_sql_conversion_error)?,
    })
}

fn category_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    let id: String = row.get(0)?;
    let enabled: i64 = row.get(2)?;
    Ok(Category {
        id: CategoryId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        name: row.get(1)?,
        enabled: match enabled {
            0 => false,
            1 => true,
            value => {
                return Err(to_sql_conversion_error(format!(
                    "invalid category enabled value {value}"
                )));
            }
        },
    })
}

fn classification_rule_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClassificationRule> {
    let id: String = row.get(0)?;
    let priority: i64 = row.get(1)?;
    let category_id: String = row.get(4)?;
    let enabled: i64 = row.get(5)?;
    Ok(ClassificationRule {
        id: RuleId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        priority: u32::try_from(priority)
            .map_err(|_| to_sql_conversion_error(format!("invalid rule priority {priority}")))?,
        merchant_contains: row.get(2)?,
        description_contains: row.get(3)?,
        category_id: CategoryId::from_uuid(
            parse_uuid(&category_id).map_err(to_sql_conversion_error)?,
        ),
        enabled: match enabled {
            0 => false,
            1 => true,
            value => {
                return Err(to_sql_conversion_error(format!(
                    "invalid rule enabled value {value}"
                )));
            }
        },
    })
}

fn transaction_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(Transaction, Result<Option<Money>, RepositoryError>)> {
    let id: String = row.get(0)?;
    let occurred: i64 = row.get(1)?;
    let kind: String = row.get(2)?;
    let status: String = row.get(3)?;
    let category: Option<String> = row.get(4)?;
    let original: Option<String> = row.get(5)?;
    let amount: Option<i64> = row.get(6)?;
    let code: Option<String> = row.get(7)?;
    let exponent: Option<u8> = row.get(8)?;
    let merchant: Option<String> = row.get(9)?;
    let description: Option<String> = row.get(10)?;
    let report = match (amount, code, exponent) {
        (None, None, None) => Ok(None),
        (Some(amount), Some(code), Some(exponent)) => CurrencyCode::new(&code, exponent)
            .map(|currency| Some(Money::new(amount, currency)))
            .map_err(|error| RepositoryError::InvalidData(error.to_string())),
        _ => Err(RepositoryError::InvalidData(
            "incomplete report amount fields".into(),
        )),
    };
    Ok((
        Transaction {
            id: TransactionId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
            occurred_at: parse_timestamp(occurred).map_err(to_sql_conversion_error)?,
            kind: parse_transaction_kind(&kind).map_err(to_sql_conversion_error)?,
            status: parse_transaction_status(&status).map_err(to_sql_conversion_error)?,
            postings: Vec::new(),
            category_id: category
                .map(|id| parse_uuid(&id).map(CategoryId::from_uuid))
                .transpose()
                .map_err(to_sql_conversion_error)?,
            original_transaction_id: original
                .map(|id| parse_uuid(&id).map(TransactionId::from_uuid))
                .transpose()
                .map_err(to_sql_conversion_error)?,
            report_amount_cny: None,
            merchant,
            description,
        },
        report,
    ))
}

fn posting_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Posting> {
    let account: String = row.get(0)?;
    let amount: i64 = row.get(1)?;
    let code: String = row.get(2)?;
    let exponent: u8 = row.get(3)?;
    Ok(Posting {
        account_id: AccountId::from_uuid(parse_uuid(&account).map_err(to_sql_conversion_error)?),
        delta: Money::new(
            amount,
            CurrencyCode::new(&code, exponent)
                .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        ),
    })
}

fn parse_timestamp(micros: i64) -> Result<DateTime<Utc>, String> {
    DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| "invalid Unix microsecond timestamp".into())
}

fn parse_uuid(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| error.to_string())
}

fn to_sql_conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error)),
    )
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn map_insert_error(error: rusqlite::Error) -> RepositoryError {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            RepositoryError::Conflict
        }
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            RepositoryError::InvalidData(error.to_string())
        }
        _ => storage_error(error),
    }
}

fn account_kind_str(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
    }
}
fn parse_account_kind(value: &str) -> Result<AccountKind, String> {
    match value {
        "asset" => Ok(AccountKind::Asset),
        "liability" => Ok(AccountKind::Liability),
        _ => Err(format!("unknown account kind {value}")),
    }
}
fn transaction_kind_str(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Transfer => "transfer",
        TransactionKind::Refund => "refund",
        TransactionKind::BalanceAdjustment => "balance_adjustment",
    }
}
fn parse_transaction_kind(value: &str) -> Result<TransactionKind, String> {
    match value {
        "income" => Ok(TransactionKind::Income),
        "expense" => Ok(TransactionKind::Expense),
        "transfer" => Ok(TransactionKind::Transfer),
        "refund" => Ok(TransactionKind::Refund),
        "balance_adjustment" => Ok(TransactionKind::BalanceAdjustment),
        _ => Err(format!("unknown transaction kind {value}")),
    }
}
fn transaction_status_str(status: TransactionStatus) -> &'static str {
    match status {
        TransactionStatus::Pending => "pending",
        TransactionStatus::Confirmed => "confirmed",
        TransactionStatus::Ignored => "ignored",
    }
}
fn parse_transaction_status(value: &str) -> Result<TransactionStatus, String> {
    match value {
        "pending" => Ok(TransactionStatus::Pending),
        "confirmed" => Ok(TransactionStatus::Confirmed),
        "ignored" => Ok(TransactionStatus::Ignored),
        _ => Err(format!("unknown transaction status {value}")),
    }
}
fn source_kind_str(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Alipay => "alipay",
        SourceKind::WeChat => "wechat",
        SourceKind::Bank => "bank",
        SourceKind::Notification => "notification",
        SourceKind::Manual => "manual",
    }
}

impl SqliteLedgerRepository {
    fn validate_feature_transaction(&self, t: &Transaction) -> Result<(), RepositoryError> {
        let mut shape = t.clone();
        if t.status == TransactionStatus::Pending
            && t.kind == TransactionKind::Refund
            && t.original_transaction_id.is_none()
        {
            // Candidate review may correct amount/account/text before the user
            // resolves a refund parent. Confirmation still validates Refund.
            shape.kind = TransactionKind::Income;
        }
        ledger_domain::validate_transaction_shape(&shape, &self.list_accounts()?)
            .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
        if t.kind == TransactionKind::Refund {
            if let Some(parent_id) = t.original_transaction_id {
                let parent = self
                    .get_transaction(parent_id)?
                    .ok_or(RepositoryError::NotFound)?;
                let valid_status = if t.status == TransactionStatus::Pending {
                    matches!(
                        parent.status,
                        TransactionStatus::Pending | TransactionStatus::Confirmed
                    )
                } else {
                    parent.status == TransactionStatus::Confirmed
                };
                if parent.kind != TransactionKind::Expense || !valid_status {
                    return Err(RepositoryError::InvalidData("refund parent must be an expense; confirmation requires a confirmed parent".into()));
                }
            } else if t.status != TransactionStatus::Pending {
                return Err(RepositoryError::InvalidData(
                    "refund parent must be confirmed expense".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn metadata(
        &self,
        id: TransactionId,
    ) -> Result<ledger_domain::TransactionMetadata, RepositoryError> {
        self.connection.query_row("SELECT updated_at_utc_micros,edit_version,reversal_of,report_income_minor,report_expense_minor FROM transaction_metadata WHERE transaction_id=?1",[id.as_uuid().to_string()],|r|{
            let at:i64=r.get(0)?; let reversal:Option<String>=r.get(2)?;let income:Option<i64>=r.get(3)?;
            Ok(ledger_domain::TransactionMetadata{transaction_id:id,updated_at:DateTime::from_timestamp_micros(at).ok_or(rusqlite::Error::InvalidQuery)?,edit_version:r.get(1)?,reversal_of:reversal.map(|v|Uuid::parse_str(&v).map(TransactionId::from_uuid)).transpose().map_err(|_|rusqlite::Error::InvalidQuery)?,report_override:income.map(|i|Ok::<_,rusqlite::Error>(ledger_domain::ReportEffect{income_cny_minor:i,expense_cny_minor:r.get(4)?})).transpose()?})
        }).map_err(storage_error)
    }
}
impl ledger_app::FeatureRepository for SqliteLedgerRepository {
    fn query_transactions(
        &self,
        q: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, RepositoryError> {
        if !(1..=200).contains(&q.limit)
            || q.from_utc_inclusive
                .zip(q.until_utc_exclusive)
                .is_some_and(|(a, b)| a >= b)
        {
            return Err(RepositoryError::InvalidData(
                "invalid page or date range".into(),
            ));
        }
        let sql = "FROM transactions t WHERE (?1 IS NULL OR status=?1) AND (?2 IS NULL OR occurred_at_utc_micros>=?2) AND (?3 IS NULL OR occurred_at_utc_micros<?3) AND (?4 IS NULL OR EXISTS(SELECT 1 FROM postings p WHERE p.transaction_id=t.id AND p.account_id=?4)) AND (?5 IS NULL OR category_id=?5) AND (?6 IS NULL OR instr(lower(coalesce(merchant,'')),lower(?6))>0) AND (?7 IS NULL OR instr(lower(coalesce(merchant,'')||' '||coalesce(description,'')),lower(?7))>0) AND (?8 IS NULL OR EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind=?8))";
        let values = params![
            q.status.map(transaction_status_str),
            q.from_utc_inclusive.map(|t| t.timestamp_micros()),
            q.until_utc_exclusive.map(|t| t.timestamp_micros()),
            q.account_id.map(|v| v.as_uuid().to_string()),
            q.category_id.map(|v| v.as_uuid().to_string()),
            q.merchant,
            q.keyword,
            q.source_kind.map(source_kind_str)
        ];
        let snapshot = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let total = self
            .connection
            .query_row(&format!("SELECT count(*) {sql}"), values, |r| r.get(0))
            .map_err(storage_error)?;
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT id {sql} ORDER BY occurred_at_utc_micros DESC,id LIMIT {} OFFSET {}",
                q.limit, q.offset
            ))
            .map_err(storage_error)?;
        let ids = stmt
            .query_map(values, |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let items = ids
            .into_iter()
            .map(|v| {
                let id = TransactionId::from_uuid(
                    Uuid::parse_str(&v)
                        .map_err(|_| RepositoryError::InvalidData("invalid ID".into()))?,
                );
                Ok(ledger_domain::PendingCandidate {
                    transaction: self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?,
                    metadata: self.metadata(id)?,
                })
            })
            .collect::<Result<_, RepositoryError>>()?;
        drop(stmt);
        snapshot.commit().map_err(storage_error)?;
        Ok(ledger_domain::TransactionPage {
            items,
            total,
            offset: q.offset,
            limit: q.limit,
        })
    }
    fn confirm_bulk(
        &self,
        request: ledger_domain::BulkConfirmation,
    ) -> Result<(), RepositoryError> {
        self.confirm_bulk_with_edits(request, Vec::new())
    }
    fn confirm_bulk_with_edits(
        &self,
        request: ledger_domain::BulkConfirmation,
        edits: Vec<ledger_domain::TransactionEdit>,
    ) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let ids = request.transaction_ids;
        if ids.len() > 200 || edits.iter().any(|edit| !ids.contains(&edit.transaction.id)) {
            return Err(RepositoryError::InvalidData(
                "invalid confirmation selection".into(),
            ));
        }
        let mut seen_edits = std::collections::HashSet::new();
        for edit in &edits {
            if !seen_edits.insert(edit.transaction.id) {
                return Err(RepositoryError::Conflict);
            }
        }
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let current = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
            let edit = edits.iter().find(|edit| edit.transaction.id == id);
            if current.status == TransactionStatus::Confirmed {
                if let Some(edit) = edit {
                    let mut proposed = edit.transaction.clone();
                    proposed.status = TransactionStatus::Confirmed;
                    if proposed != current {
                        return Err(RepositoryError::Conflict);
                    }
                }
                continue;
            }
            if current.status != TransactionStatus::Pending {
                return Err(RepositoryError::Conflict);
            }
            let mut proposed = if let Some(edit) = edit {
                if edit.transaction.status != TransactionStatus::Pending
                    || self.metadata(id)?.edit_version != edit.expected_version
                {
                    return Err(RepositoryError::Conflict);
                }
                edit.transaction.clone()
            } else {
                current
            };
            proposed.status = TransactionStatus::Confirmed;
            self.validate_feature_transaction(&proposed)?;
            Self::write_transaction(&tx, &proposed, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn ignore_bulk(&self, ids: &[TransactionId]) -> Result<(), RepositoryError> {
        if ids.len() > 200 {
            return Err(RepositoryError::InvalidData(
                "selection is too large".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        for id in ids {
            let mut current = self
                .get_transaction(*id)?
                .ok_or(RepositoryError::NotFound)?;
            if current.status == TransactionStatus::Confirmed {
                return Err(RepositoryError::Conflict);
            }
            if current.status == TransactionStatus::Pending {
                current.status = TransactionStatus::Ignored;
                Self::write_transaction(&tx, &current, false)?;
            }
        }
        tx.commit().map_err(storage_error)
    }
    fn restore_pending_atomic(&self, id: TransactionId) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut t = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if t.status == TransactionStatus::Confirmed {
            return Err(RepositoryError::Conflict);
        }
        if t.status == TransactionStatus::Ignored {
            t.status = TransactionStatus::Pending;
            Self::write_transaction(&tx, &t, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn ignore_atomic(&self, id: TransactionId) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut t = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if t.status == TransactionStatus::Confirmed {
            return Err(RepositoryError::Conflict);
        }
        if t.status == TransactionStatus::Pending {
            t.status = TransactionStatus::Ignored;
            Self::write_transaction(&tx, &t, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn reverse_atomic(
        &self,
        id: TransactionId,
        new_id: TransactionId,
        at: Timestamp,
    ) -> Result<TransactionId, RepositoryError> {
        self.reverse_named(id, new_id, at, "reversal")
    }
    fn edit_atomic(&self, e: ledger_domain::TransactionEdit) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let old = self
            .get_transaction(e.transaction.id)?
            .ok_or(RepositoryError::NotFound)?;
        let m = self.metadata(old.id)?;
        let reversed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM transaction_metadata WHERE reversal_of=?1)",
                [old.id.as_uuid().to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if m.edit_version != e.expected_version
            || m.reversal_of.is_some()
            || reversed
            || old.status != e.transaction.status
        {
            return Err(RepositoryError::Conflict);
        }
        if old.status == TransactionStatus::Confirmed
            && (old.kind != e.transaction.kind
                || old.original_transaction_id != e.transaction.original_transaction_id)
        {
            return Err(RepositoryError::Conflict);
        }
        if self.has_active_refunds(old.id)?
            && (e.transaction.kind != TransactionKind::Expense
                || e.transaction.postings != old.postings)
        {
            return Err(RepositoryError::InvalidData(
                "expense has active refunds".into(),
            ));
        }
        self.validate_feature_transaction(&e.transaction)?;
        Self::write_transaction(&tx, &e.transaction, false)?;
        tx.commit().map_err(storage_error)
    }
    fn source_evidence(&self, id: TransactionId) -> Result<Vec<SourceEvidence>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT source_kind,external_transaction_id,import_batch_id,source_fields_digest FROM source_evidence WHERE transaction_id=?1 ORDER BY id").map_err(storage_error)?;
        stmt.query_map([id.as_uuid().to_string()], |r| {
            let k: String = r.get(0)?;
            let batch: String = r.get(2)?;
            let digest: Vec<u8> = r.get(3)?;
            Ok(SourceEvidence {
                source_kind: match k.as_str() {
                    "alipay" => SourceKind::Alipay,
                    "wechat" => SourceKind::WeChat,
                    "bank" => SourceKind::Bank,
                    "notification" => SourceKind::Notification,
                    "manual" => SourceKind::Manual,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                },
                external_transaction_id: r.get(1)?,
                import_batch_id: Uuid::parse_str(&batch)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                source_fields_digest: digest
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<_, _>>()
        .map_err(storage_error)
    }
    fn privacy_settings(&self) -> Result<ledger_domain::PrivacySettings, RepositoryError> {
        let get = |key: &str| {
            self.connection
                .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .optional()
                .map_err(storage_error)
        };
        Ok(ledger_domain::PrivacySettings {
            hide_amounts: get("hide_amounts")?.as_deref() == Some("1"),
            lock_enabled: get("lock_enabled")?.as_deref() == Some("1"),
            password_verifier: get("password_verifier")?,
        })
    }
    fn save_privacy_settings(
        &self,
        s: ledger_domain::PrivacySettings,
    ) -> Result<(), RepositoryError> {
        // Password verifier format and lock guard are deliberately deferred. Never enable an unguarded lock.
        if s.lock_enabled || s.password_verifier.is_some() {
            return Err(RepositoryError::InvalidData(
                "app lock is not implemented yet".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        for (key, value) in [
            ("hide_amounts", if s.hide_amounts { "1" } else { "0" }),
            ("lock_enabled", "0"),
        ] {
            tx.execute("INSERT INTO app_settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }
}

impl ledger_app::ImportRepository for SqliteLedgerRepository {
    fn find_source_transaction(
        &self,
        source: SourceKind,
        external_id: &str,
    ) -> Result<Option<TransactionId>, RepositoryError> {
        let id: Option<String> = self.connection.query_row(
            "SELECT transaction_id FROM source_evidence WHERE source_kind=?1 AND external_transaction_id=?2",
            params![source_kind_str(source), external_id], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| {
            Uuid::parse_str(&id)
                .map(TransactionId::from_uuid)
                .map_err(|_| RepositoryError::InvalidData("invalid transaction ID".into()))
        })
        .transpose()
    }

    fn insert_pending_batch(
        &self,
        batch_id: Uuid,
        items: Vec<(Transaction, SourceEvidence)>,
    ) -> Result<Vec<TransactionId>, RepositoryError> {
        if items.is_empty() || items.len() > 10_000 {
            return Err(RepositoryError::InvalidData(
                "import batch must contain 1..10000 candidates".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let prior: Vec<(String, Vec<u8>)> = {
            let mut stmt = tx.prepare("SELECT transaction_id,source_fields_digest FROM import_receipt_rows WHERE batch_id=?1 ORDER BY ordinal").map_err(storage_error)?;
            let rows = stmt
                .query_map([batch_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(storage_error)?;
            rows.collect::<Result<_, _>>().map_err(storage_error)?
        };
        if !prior.is_empty() {
            if prior.len() != items.len()
                || prior
                    .iter()
                    .zip(&items)
                    .any(|((_, digest), (_, evidence))| {
                        digest.as_slice() != evidence.source_fields_digest
                    })
            {
                return Err(RepositoryError::Conflict);
            }
            return prior
                .into_iter()
                .map(|(id, _)| {
                    Uuid::parse_str(&id)
                        .map(TransactionId::from_uuid)
                        .map_err(|_| RepositoryError::InvalidData("invalid receipt ID".into()))
                })
                .collect();
        }
        tx.execute(
            "INSERT INTO import_batches(id) VALUES(?1)",
            [batch_id.to_string()],
        )
        .map_err(map_insert_error)?;
        let mut receipt = Vec::with_capacity(items.len());
        for (ordinal, (transaction, evidence)) in items.iter().enumerate() {
            if transaction.status != TransactionStatus::Pending
                || evidence.import_batch_id != batch_id
                || evidence
                    .external_transaction_id
                    .as_deref()
                    .is_some_and(|id| id.trim().is_empty())
            {
                return Err(RepositoryError::InvalidData(
                    "invalid pending import candidate".into(),
                ));
            }
            self.validate_feature_transaction(transaction)?;
            let existing = evidence
                .external_transaction_id
                .as_deref()
                .map(|external_id| self.find_source_transaction(evidence.source_kind, external_id))
                .transpose()?
                .flatten();
            let id = if let Some(id) = existing {
                id
            } else {
                Self::write_transaction_with_evidence(&tx, transaction, Some(evidence))?;
                transaction.id
            };
            tx.execute("INSERT INTO import_receipt_rows(batch_id,ordinal,transaction_id,source_fields_digest) VALUES(?1,?2,?3,?4)", params![batch_id.to_string(), ordinal as i64, id.as_uuid().to_string(), evidence.source_fields_digest.as_slice()]).map_err(map_insert_error)?;
            receipt.push(id);
        }
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

impl SqliteLedgerRepository {
    pub fn reverse_with_reason(
        &self,
        id: TransactionId,
        at: Timestamp,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        if reason.trim().is_empty() || reason.chars().count() > 200 {
            return Err(RepositoryError::InvalidData(
                "invalid reversal reason".into(),
            ));
        }
        self.reverse_named(id, TransactionId::new(), at, reason)
    }
    fn reverse_named(
        &self,
        id: TransactionId,
        new_id: TransactionId,
        at: Timestamp,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        if let Some(existing) = tx
            .query_row(
                "SELECT transaction_id FROM transaction_metadata WHERE reversal_of=?1",
                [id.as_uuid().to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?
        {
            return Uuid::parse_str(&existing)
                .map(TransactionId::from_uuid)
                .map_err(|_| RepositoryError::InvalidData("invalid ID".into()));
        }
        let original = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if original.status != TransactionStatus::Confirmed
            || self.metadata(id)?.reversal_of.is_some()
        {
            return Err(RepositoryError::Conflict);
        }
        if self.has_active_refunds(id)? {
            return Err(RepositoryError::InvalidData(
                "reverse related refunds first".into(),
            ));
        }
        let effect =
            ledger_domain::report_effect(&original).unwrap_or(ledger_domain::ReportEffect {
                income_cny_minor: 0,
                expense_cny_minor: 0,
            });
        let mut reverse = original.clone();
        reverse.id = new_id;
        reverse.occurred_at = at;
        reverse.kind = TransactionKind::BalanceAdjustment;
        reverse.original_transaction_id = Some(id);
        reverse.report_amount_cny = None;
        for p in &mut reverse.postings {
            p.delta = Money::new(
                p.delta
                    .minor_units()
                    .checked_neg()
                    .ok_or_else(|| RepositoryError::InvalidData("reversal overflow".into()))?,
                p.delta.currency().clone(),
            );
        }
        self.validate_feature_transaction(&reverse)?;
        Self::write_transaction(&tx, &reverse, true)?;
        tx.execute("UPDATE transaction_metadata SET reversal_of=?2,report_income_minor=?3,report_expense_minor=?4 WHERE transaction_id=?1",params![new_id.as_uuid().to_string(),id.as_uuid().to_string(),effect.income_cny_minor.checked_neg().ok_or(RepositoryError::Conflict)?,effect.expense_cny_minor.checked_neg().ok_or(RepositoryError::Conflict)?]).map_err(storage_error)?;
        tx.execute(
            "UPDATE transactions SET description=?2 WHERE id=?1",
            params![new_id.as_uuid().to_string(), reason],
        )
        .map_err(storage_error)?;
        tx.execute(
            "UPDATE audit_events SET action='reversed' WHERE transaction_id=?1",
            [new_id.as_uuid().to_string()],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,'reversed',?3)", params![Uuid::new_v4().to_string(), at.timestamp_micros(), id.as_uuid().to_string()]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(new_id)
    }
}

impl SqliteLedgerRepository {
    pub fn audit_history(&self, id: TransactionId) -> Result<Vec<(String, i64)>, RepositoryError> {
        self.connection.prepare("SELECT action,occurred_at_utc_micros FROM audit_events WHERE transaction_id=?1 ORDER BY occurred_at_utc_micros,id").map_err(storage_error)?
          .query_map([id.as_uuid().to_string()], |r| Ok((r.get(0)?,r.get(1)?))).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)
    }
    pub fn related_transactions(
        &self,
        id: TransactionId,
    ) -> Result<Vec<(TransactionId, bool)>, RepositoryError> {
        let id_text = id.as_uuid().to_string();
        let mut statement = self.connection.prepare(
            "SELECT t.id, m.reversal_of FROM transactions t LEFT JOIN transaction_metadata m ON m.transaction_id=t.id WHERE t.original_transaction_id=?1 ORDER BY t.occurred_at_utc_micros,t.id"
        ).map_err(storage_error)?;
        statement
            .query_map([&id_text], |row| {
                let raw_id: String = row.get(0)?;
                let reversal_of: Option<String> = row.get(1)?;
                let parsed = Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok((
                    TransactionId::from_uuid(parsed),
                    reversal_of.as_deref() == Some(id_text.as_str()),
                ))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }
    pub(crate) fn has_active_refunds(&self, id: TransactionId) -> Result<bool, RepositoryError> {
        self.connection.query_row("SELECT EXISTS(SELECT 1 FROM transactions t WHERE original_transaction_id=?1 AND kind='refund' AND status='confirmed' AND NOT EXISTS(SELECT 1 FROM transaction_metadata m WHERE m.reversal_of=t.id))",[id.as_uuid().to_string()],|r|r.get(0)).map_err(storage_error)
    }
}
