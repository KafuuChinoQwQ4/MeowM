//! Portable data only: never execute SQL or restore schema supplied by a backup.
mod schema;
mod validation;
use super::*;
use rusqlite::types::{Value, ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
const MAX_ROWS: usize = 200_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerSnapshot {
    pub schema_version: i64,
    tables: Vec<Table>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    columns: Vec<String>,
    rows: Vec<Vec<Cell>>,
}
#[derive(Clone, Serialize, Deserialize)]
enum Cell {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}
impl Cell {
    fn value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Integer(v) => Value::Integer(*v),
            Self::Text(v) => Value::Text(v.clone()),
            Self::Blob(v) => Value::Blob(v.clone()),
        }
    }
}
fn invalid() -> RepositoryError {
    RepositoryError::InvalidData("invalid or unsupported backup data".into())
}
impl LedgerSnapshot {
    pub fn fingerprint(&self) -> Result<[u8; 32], RepositoryError> {
        Ok(Sha256::digest(serde_json::to_vec(self).map_err(|_| invalid())?).into())
    }
    pub fn counts(&self) -> Vec<(String, usize)> {
        self.tables
            .iter()
            .map(|t| (t.name.clone(), t.rows.len()))
            .collect()
    }
}
impl SqliteLedgerRepository {
    /// Consistent portable snapshot, excluding credentials and permission grants.
    pub fn snapshot(&self) -> Result<LedgerSnapshot, RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let data = self.snapshot_inner()?;
        tx.commit().map_err(storage_error)?;
        Ok(data)
    }
    fn snapshot_inner(&self) -> Result<LedgerSnapshot, RepositoryError> {
        let mut tables = Vec::new();
        let mut row_count = 0;
        for name in schema::TABLES {
            if !schema::exists(&self.connection, name)? {
                continue;
            }
            let columns = schema::columns(&self.connection, name)?;
            let filter = if *name == "app_settings" {
                " WHERE key='hide_amounts'"
            } else {
                ""
            };
            // Rowid order also preserves the user's account ordering after restore.
            let mut stmt = self
                .connection
                .prepare(&format!("SELECT * FROM {name}{filter} ORDER BY rowid"))
                .map_err(storage_error)?;
            let rows = stmt
                .query_map([], |r| {
                    (0..columns.len())
                        .map(|i| {
                            Ok(match r.get_ref(i)? {
                                ValueRef::Null => Cell::Null,
                                ValueRef::Integer(v) => Cell::Integer(v),
                                ValueRef::Text(v) => Cell::Text(
                                    std::str::from_utf8(v)
                                        .map_err(|_| rusqlite::Error::InvalidQuery)?
                                        .into(),
                                ),
                                ValueRef::Blob(v) => Cell::Blob(v.into()),
                                ValueRef::Real(_) => return Err(rusqlite::Error::InvalidQuery),
                            })
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(storage_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(storage_error)?;
            row_count += rows.len();
            if row_count > MAX_ROWS {
                return Err(invalid());
            }
            tables.push(Table {
                name: name.to_string(),
                columns,
                rows,
            });
        }
        let data = LedgerSnapshot {
            schema_version: self.schema_version()?,
            tables,
        };
        if serde_json::to_vec(&data).map_err(|_| invalid())?.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid());
        }
        Ok(data)
    }
    /// Rebuild in an isolated database using only the application's trusted migrations.
    pub fn from_snapshot(data: &LedgerSnapshot) -> Result<Self, RepositoryError> {
        let connection = schema::empty_at(data.schema_version)?;
        let repo = Self { connection };
        let tx = repo
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON")
            .map_err(storage_error)?;
        schema::insert(&tx, data)?;
        tx.commit().map_err(storage_error)?;
        let repo = Self::from_connection(repo.connection)?;
        repo.validate_snapshot()?;
        Ok(repo)
    }
    /// Atomic replacement; a concurrent write invalidates the preview instead of being lost.
    pub fn replace_snapshot(
        &self,
        data: &LedgerSnapshot,
        expected: [u8; 32],
    ) -> Result<(), RepositoryError> {
        let validated = Self::from_snapshot(data)?.snapshot()?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        if self.snapshot_inner()?.fingerprint()? != expected {
            return Err(RepositoryError::Conflict);
        }
        tx.execute_batch("PRAGMA defer_foreign_keys=ON")
            .map_err(storage_error)?;
        for name in schema::TABLES.iter().rev() {
            // Future app-lock credentials stay local and never enter backup files.
            let sql = if *name == "app_settings" {
                "DELETE FROM app_settings WHERE key='hide_amounts'".to_string()
            } else {
                format!("DELETE FROM {name}")
            };
            tx.execute(&sql, []).map_err(storage_error)?;
        }
        schema::insert(&tx, &validated)?;
        self.validate_snapshot()?;
        tx.commit().map_err(storage_error)
    }
}
