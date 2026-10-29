use super::*;
use ledger_app::classification::normalize_merchant;

const INITIAL_MIGRATION: &str = include_str!("../../migrations/0001_initial.sql");
const CLASSIFICATION_MIGRATION: &str =
    include_str!("../../migrations/0002_classification_rules.sql");
impl SqliteLedgerRepository {
    pub(super) fn apply_migrations(&self) -> Result<(), RepositoryError> {
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
                && v <= 12
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
                (2, Some(2))
                | (3, Some(3))
                | (4, Some(4))
                | (5, Some(5))
                | (6, Some(6))
                | (7, Some(7))
                | (8, Some(8))
                | (9, Some(9))
                | (10, Some(10))
                | (11, Some(11))
                | (12, Some(12)) => {
                    return Ok(());
                }
                (_, Some(version)) if version > 12 => {
                    return Err(RepositoryError::Storage(format!(
                        "database schema version {version} is newer than supported version 12"
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

    pub(super) fn apply_classification_migration(
        &self,
        current_version: i64,
    ) -> Result<(), RepositoryError> {
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

    pub(super) fn apply_optional_balance_migration(&self) -> Result<(), RepositoryError> {
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
            tx.execute_batch(include_str!("../../migrations/0003_optional_balance.sql"))
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

    pub(super) fn apply_feature_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 4 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0004_nine_features.sql"))
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

    pub(super) fn apply_pending_import_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 5 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0005_pending_import.sql"))
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
}

impl SqliteLedgerRepository {
    pub(super) fn apply_notification_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 6 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0006_notifications.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(6,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

impl SqliteLedgerRepository {
    pub(super) fn apply_recurring_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 7 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0007_recurring.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(7,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

impl SqliteLedgerRepository {
    pub(super) fn apply_search_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 8 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0008_search_rules.sql"))
            .map_err(storage_error)?;
        let merchants = {
            let mut query = tx
                .prepare("SELECT id, merchant FROM transactions WHERE merchant IS NOT NULL")
                .map_err(storage_error)?;
            query
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage_error)?
        };
        for (id, merchant) in merchants {
            tx.execute(
                "UPDATE transactions SET merchant_normalized = ?2 WHERE id = ?1",
                rusqlite::params![id, normalize_merchant(&merchant)],
            )
            .map_err(storage_error)?;
        }
        tx.execute(
            "INSERT INTO schema_version VALUES(8,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

impl SqliteLedgerRepository {
    pub(super) fn apply_reconciliation_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 9 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0009_reconciliation.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(9,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

impl SqliteLedgerRepository {
    pub(super) fn apply_screen_capture_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 10 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0010_screen_capture.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(10,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    pub(super) fn apply_refund_reference_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 11 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0011_refund_references.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(11,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    pub(super) fn apply_merchant_order_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 12 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0012_merchant_orders.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(12,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}
