//! SQLite connection, migrations, and repository implementation.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use ledger_app::{
    ClassificationRule, ClassificationRuleRepository, DeletionRepository, DeletionScope,
    ImportRecord, ImportRepository, LedgerRepository, RepositoryError, Timestamp,
};
use ledger_domain::{
    Account, AccountId, AccountKind, Category, CategoryId, CurrencyCode, Money, Posting, RuleId,
    SourceEvidence, SourceKind, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use rusqlite::{Connection, OptionalExtension, Transaction as SqlTransaction, params};
use uuid::Uuid;

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");
const CLASSIFICATION_MIGRATION: &str = include_str!("../migrations/0002_classification_rules.sql");
const IMPORT_SOURCE_MIGRATION: &str = include_str!("../migrations/0003_bank_csv_source.sql");

/// SQLite-backed local ledger repository.
pub struct SqliteLedgerRepository {
    pub(crate) connection: Connection,
}

impl SqliteLedgerRepository {
    /// Opens or creates a ledger database at `path` and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self, RepositoryError> {
        let connection = Connection::open(path).map_err(storage_error)?;
        Self::from_connection(connection)
    }

    /// The active database path, used to prevent exporting over the open ledger.
    pub fn database_path(&self) -> Option<&str> {
        self.connection.path()
    }

    /// Creates an isolated in-memory ledger database.
    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, RepositoryError> {
        connection
            .busy_timeout(std::time::Duration::from_secs(15))
            .map_err(storage_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;")
            .map_err(storage_error)?;
        let repo = Self { connection };
        repo.apply_migrations()?;
        repo.apply_mobile_migration()?;
        // Device grants are deliberately outside the portable backup schema.
        repo.connection.execute_batch("CREATE TABLE IF NOT EXISTS provider_settings (id INTEGER PRIMARY KEY CHECK(id=1), wechat INTEGER NOT NULL DEFAULT 0 CHECK(wechat IN (0,1)), alipay INTEGER NOT NULL DEFAULT 0 CHECK(alipay IN (0,1)), bank INTEGER NOT NULL DEFAULT 0 CHECK(bank IN (0,1))); INSERT OR IGNORE INTO provider_settings(id) VALUES(1);").map_err(storage_error)?;
        Ok(repo)
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
            match (version_count, recorded_version) {
                (1, Some(1)) => {
                    self.apply_classification_migration(1)?;
                    self.apply_import_source_migration(2)?;
                    return Ok(());
                }
                (2, Some(2)) => {
                    self.apply_import_source_migration(2)?;
                    return Ok(());
                }
                (3, Some(3)) | (4, Some(4)) => return Ok(()),
                (_, Some(version)) if version > 4 => {
                    return Err(RepositoryError::Storage(format!(
                        "database schema version {version} is newer than supported version 4"
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
        tx.commit().map_err(storage_error)?;
        self.apply_import_source_migration(2)
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

    fn apply_import_source_migration(&self, current_version: i64) -> Result<(), RepositoryError> {
        if current_version != 2 {
            return Err(RepositoryError::Storage(format!(
                "cannot migrate schema version {current_version}"
            )));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(IMPORT_SOURCE_MIGRATION)
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (3, ?1)",
            [SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    fn apply_mobile_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? == 4 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../migrations/0004_mobile_workflows.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES(4, ?1)",
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
        Ok(())
    }
}

impl LedgerRepository for SqliteLedgerRepository {
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO accounts(id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account.id.as_uuid().to_string(), account.display_name,
                    account_kind_str(account.kind), account.currency.code(),
                    account.currency.minor_unit_exponent(), account.opening_balance.minor_units(),
                    account.opening_balance_effective_at.timestamp_micros(),
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
                account.currency.minor_unit_exponent(), account.opening_balance.minor_units(),
                account.opening_balance_effective_at.timestamp_micros(),
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
    ) -> Result<Money, RepositoryError> {
        let account = self
            .get_account(account_id)?
            .ok_or(RepositoryError::NotFound)?;
        let mut balance = account.opening_balance;
        let mut statement = self.connection.prepare(
            "SELECT p.delta_minor, p.currency_code, p.currency_exponent FROM postings p JOIN transactions t ON t.id = p.transaction_id WHERE p.account_id = ?1 AND t.status = 'confirmed' AND t.occurred_at_utc_micros > ?2 AND t.occurred_at_utc_micros <= ?3 ORDER BY t.occurred_at_utc_micros, t.rowid, p.ordinal",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    account_id.as_uuid().to_string(),
                    account.opening_balance_effective_at.timestamp_micros(),
                    cutoff_utc.timestamp_micros()
                ],
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
            balance = balance
                .checked_add(money)
                .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
        }
        Ok(balance)
    }
}

impl ImportRepository for SqliteLedgerRepository {
    fn list_import_records(&self) -> Result<Vec<ImportRecord>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest FROM source_evidence ORDER BY id",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        rows.into_iter()
            .map(|(transaction_id, source, external, batch, digest)| {
                let id = TransactionId::from_uuid(
                    parse_uuid(&transaction_id).map_err(RepositoryError::InvalidData)?,
                );
                let transaction = self.get_transaction(id)?.ok_or_else(|| {
                    RepositoryError::Storage(
                        "import evidence references missing transaction".into(),
                    )
                })?;
                if digest.len() != 32 {
                    return Err(RepositoryError::InvalidData(
                        "invalid source digest length".into(),
                    ));
                }
                let mut source_fields_digest = [0; 32];
                source_fields_digest.copy_from_slice(&digest);
                Ok(ImportRecord {
                    transaction,
                    evidence: SourceEvidence {
                        source_kind: parse_source_kind(&source)
                            .map_err(RepositoryError::InvalidData)?,
                        external_transaction_id: external,
                        import_batch_id: parse_uuid(&batch)
                            .map_err(RepositoryError::InvalidData)?,
                        source_fields_digest,
                    },
                })
            })
            .collect()
    }

    fn confirm_import_batch(
        &self,
        batch_id: Uuid,
        records: Vec<ImportRecord>,
    ) -> Result<usize, RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let existing: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM import_batches WHERE id = ?1)",
                [batch_id.to_string()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if existing {
            tx.rollback().map_err(storage_error)?;
            return Ok(0);
        }
        tx.execute(
            "INSERT INTO import_batches(id) VALUES (?1)",
            [batch_id.to_string()],
        )
        .map_err(map_insert_error)?;
        let mut inserted = 0;
        for record in records {
            if record.evidence.import_batch_id != batch_id {
                return Err(RepositoryError::InvalidData(
                    "import record batch ID mismatch".into(),
                ));
            }
            if record.transaction.kind == TransactionKind::Refund {
                let parent_id = record.transaction.original_transaction_id.ok_or_else(|| {
                    RepositoryError::InvalidData(
                        "refund import requires a mapped parent transaction".into(),
                    )
                })?;
                let valid_parent: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM transactions WHERE id = ?1 AND kind = 'expense' AND status = 'confirmed')",
                    [parent_id.as_uuid().to_string()],
                    |row| row.get(0),
                ).map_err(storage_error)?;
                if !valid_parent {
                    return Err(RepositoryError::InvalidData(
                        "refund parent must be an existing confirmed expense".into(),
                    ));
                }
            }
            if let Some(old) = self.get_transaction(record.transaction.id)? {
                if old.status == TransactionStatus::Pending
                    && record.transaction.status == TransactionStatus::Confirmed
                {
                    Self::write_transaction(&tx, &record.transaction, false)?;
                } else if old != record.transaction {
                    return Err(RepositoryError::Conflict);
                }
            } else {
                Self::write_transaction(&tx, &record.transaction, true)?;
            }
            tx.execute("INSERT INTO source_evidence(transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest) VALUES (?1, ?2, ?3, ?4, ?5)", params![
                record.transaction.id.as_uuid().to_string(), source_kind_str(record.evidence.source_kind), record.evidence.external_transaction_id,
                batch_id.to_string(), record.evidence.source_fields_digest.as_slice(),
            ]).map_err(map_insert_error)?;
            inserted += 1;
        }
        tx.commit().map_err(storage_error)?;
        Ok(inserted)
    }
}

impl DeletionRepository for SqliteLedgerRepository {
    fn delete_scope(&self, scope: DeletionScope) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        match scope {
            DeletionScope::Transaction(id) => {
                let referenced: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE original_transaction_id=?1)", [id.as_uuid().to_string()], |r| r.get(0)).map_err(storage_error)?;
                if referenced {
                    return Err(RepositoryError::InvalidData(
                        "cannot delete a transaction referenced by a refund".into(),
                    ));
                }
                tx.execute(
                    "DELETE FROM transactions WHERE id=?1",
                    [id.as_uuid().to_string()],
                )
                .map_err(storage_error)?;
            }
            DeletionScope::ImportBatch(batch) => {
                // A merged transaction belongs to every linked source. Remove only
                // this batch's evidence when another source still owns the record.
                let ids = {
                    let mut s=tx.prepare("SELECT DISTINCT transaction_id FROM source_evidence e WHERE import_batch_id=?1 AND NOT EXISTS(SELECT 1 FROM source_evidence other WHERE other.transaction_id=e.transaction_id AND other.import_batch_id<>?1)").map_err(storage_error)?;
                    s.query_map([batch.to_string()], |r| r.get::<_, String>(0))
                        .map_err(storage_error)?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(storage_error)?
                };
                for id in &ids {
                    let refs = {
                        let mut s = tx
                            .prepare("SELECT id FROM transactions WHERE original_transaction_id=?1")
                            .map_err(storage_error)?;
                        s.query_map([id], |r| r.get::<_, String>(0))
                            .map_err(storage_error)?
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(storage_error)?
                    };
                    if refs.iter().any(|r| !ids.contains(r)) {
                        return Err(RepositoryError::InvalidData("batch contains an expense referenced by a retained refund; delete the refund first".into()));
                    }
                }
                tx.execute_batch("PRAGMA defer_foreign_keys=ON;")
                    .map_err(storage_error)?;
                tx.execute(
                    "DELETE FROM source_evidence WHERE import_batch_id=?1",
                    [batch.to_string()],
                )
                .map_err(storage_error)?;
                for id in ids {
                    tx.execute("DELETE FROM transactions WHERE id=?1", [id])
                        .map_err(storage_error)?;
                }
                tx.execute(
                    "DELETE FROM import_batches WHERE id=?1",
                    [batch.to_string()],
                )
                .map_err(storage_error)?;
            }
            DeletionScope::AllLocal => {
                tx.execute_batch("PRAGMA defer_foreign_keys=ON; DELETE FROM transactions; DELETE FROM accounts; DELETE FROM classification_rules; DELETE FROM categories; DELETE FROM import_batches; DELETE FROM mobile_settings; INSERT INTO mobile_settings(id) VALUES(1); DELETE FROM provider_settings; INSERT INTO provider_settings(id) VALUES(1);").map_err(storage_error)?;
            }
        }
        tx.execute("DELETE FROM import_batches WHERE NOT EXISTS(SELECT 1 FROM source_evidence WHERE import_batch_id=import_batches.id)",[]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
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
    let amount: i64 = row.get(5)?;
    let effective: i64 = row.get(6)?;
    Ok(Account {
        id: AccountId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        display_name: row.get(1)?,
        kind: parse_account_kind(&kind).map_err(to_sql_conversion_error)?,
        currency: CurrencyCode::new(&code, exponent)
            .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        opening_balance: Money::new(
            amount,
            CurrencyCode::new(&code, exponent)
                .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        ),
        opening_balance_effective_at: parse_timestamp(effective)
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
        SourceKind::Notification => "notification",
        SourceKind::BankCsv => "bank_csv",
        SourceKind::Manual => "manual",
    }
}
fn parse_source_kind(value: &str) -> Result<SourceKind, String> {
    match value {
        "alipay" => Ok(SourceKind::Alipay),
        "wechat" => Ok(SourceKind::WeChat),
        "notification" => Ok(SourceKind::Notification),
        "bank_csv" => Ok(SourceKind::BankCsv),
        "manual" => Ok(SourceKind::Manual),
        _ => Err(format!("unknown source kind {value}")),
    }
}

impl ledger_app::MobileRepository for SqliteLedgerRepository {
    fn settings(&self) -> Result<ledger_app::MobileSettings, RepositoryError> {
        let (mut settings,id)=self.connection.query_row("SELECT notification_wechat,notification_alipay,import_wechat,import_alipay,import_bank,api_enabled,backup_enabled,default_account FROM mobile_settings WHERE id=1",[],|r|Ok((ledger_app::MobileSettings{notification_wechat:r.get(0)?,notification_alipay:r.get(1)?,import_wechat:r.get(2)?,import_alipay:r.get(3)?,import_bank:r.get(4)?,api_enabled:r.get(5)?,backup_enabled:r.get(6)?,default_account:None,..Default::default()},r.get::<_,Option<String>>(7)?))).map_err(storage_error)?;
        (settings.api_wechat, settings.api_alipay, settings.api_bank) = self
            .connection
            .query_row(
                "SELECT wechat,alipay,bank FROM provider_settings WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(storage_error)?;
        settings.default_account = id
            .map(|v| {
                parse_uuid(&v)
                    .map(AccountId::from_uuid)
                    .map_err(RepositoryError::InvalidData)
            })
            .transpose()?;
        Ok(settings)
    }
    fn save_settings(&self, v: &ledger_app::MobileSettings) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute("UPDATE mobile_settings SET notification_wechat=?1,notification_alipay=?2,import_wechat=?3,import_alipay=?4,import_bank=?5,api_enabled=?6,backup_enabled=?7,default_account=?8 WHERE id=1",params![v.notification_wechat,v.notification_alipay,v.import_wechat,v.import_alipay,v.import_bank,v.api_enabled,v.backup_enabled,v.default_account.map(|i|i.as_uuid().to_string())]).map_err(storage_error)?;
        tx.execute(
            "UPDATE provider_settings SET wechat=?1,alipay=?2,bank=?3 WHERE id=1",
            params![v.api_wechat, v.api_alipay, v.api_bank],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }
    fn pending_details(
        &self,
        id: TransactionId,
    ) -> Result<Option<ledger_app::PendingDetails>, RepositoryError> {
        let row=self.connection.query_row("SELECT amount_minor,currency_code,currency_exponent,source,account_hint,original_external_id FROM pending_details WHERE transaction_id=?1",[id.as_uuid().to_string()],|r|Ok((r.get::<_,Option<i64>>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<u8>>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?))).optional().map_err(storage_error)?;
        row.map(|(n, c, e, source, account_hint, original_external_id)| {
            let amount = match (n, c, e) {
                (Some(n), Some(c), Some(e)) => Some(Money::new(
                    n,
                    CurrencyCode::new(&c, e).map_err(|_| {
                        RepositoryError::InvalidData("Invalid pending currency".into())
                    })?,
                )),
                (None, None, None) => None,
                _ => {
                    return Err(RepositoryError::InvalidData(
                        "Invalid pending amount".into(),
                    ));
                }
            };
            Ok(ledger_app::PendingDetails {
                amount,
                source,
                account_hint,
                original_external_id,
            })
        })
        .transpose()
    }
    fn capture_pending(
        &self,
        record: ImportRecord,
        details: ledger_app::PendingDetails,
    ) -> Result<usize, RepositoryError> {
        if record.transaction.status != TransactionStatus::Pending {
            return Err(RepositoryError::InvalidData(
                "Captured records must be pending".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let batch = record.evidence.import_batch_id.to_string();
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM import_batches WHERE id=?1)",
                [&batch],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if exists {
            return Ok(0);
        }
        tx.execute("INSERT INTO import_batches(id) VALUES(?1)", [&batch])
            .map_err(map_insert_error)?;
        Self::write_transaction(&tx, &record.transaction, true)?;
        tx.execute("INSERT INTO source_evidence(transaction_id,source_kind,external_transaction_id,import_batch_id,source_fields_digest) VALUES(?1,?2,?3,?4,?5)",params![record.transaction.id.as_uuid().to_string(),source_kind_str(record.evidence.source_kind),record.evidence.external_transaction_id,batch,record.evidence.source_fields_digest.as_slice()]).map_err(map_insert_error)?;
        tx.execute("INSERT INTO pending_details(transaction_id,amount_minor,currency_code,currency_exponent,source,account_hint,original_external_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![record.transaction.id.as_uuid().to_string(),details.amount.as_ref().map(Money::minor_units),details.amount.as_ref().map(|m|m.currency().code()),details.amount.as_ref().map(|m|m.currency().minor_unit_exponent()),details.source,details.account_hint,details.original_external_id]).map_err(map_insert_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(1)
    }
    fn replace_transaction(
        &self,
        expected: &Transaction,
        replacement: &Transaction,
    ) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        if expected.id != replacement.id
            || self.get_transaction(expected.id)?.as_ref() != Some(expected)
        {
            return Err(RepositoryError::Conflict);
        }
        Self::write_transaction(&tx, replacement, false)?;
        tx.commit().map_err(storage_error)
    }
}
