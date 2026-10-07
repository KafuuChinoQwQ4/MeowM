//! Stable allowlist; never serialize database pages, SQL, or arbitrary future tables.
use crate::sqlite::SqliteLedgerRepository;
use ledger_app::{
    ArchiveCell, BackupRepository, ClassificationRuleRepository, ImportRepository, LedgerArchive,
    LedgerRepository, MobileRepository, RepositoryError,
};
use ledger_domain::{TransactionKind, TransactionStatus, validate_transaction_shape};
use rusqlite::{
    Connection, TransactionBehavior, params_from_iter,
    types::{Value, ValueRef},
};
use std::collections::BTreeMap;

const TABLES: &[(&str, &str)] = &[
    (
        "accounts",
        "id,display_name,kind,currency_code,currency_exponent,opening_balance_minor,opening_balance_effective_at_utc_micros",
    ),
    ("categories", "id,name,enabled"),
    (
        "transactions",
        "id,occurred_at_utc_micros,kind,status,category_id,original_transaction_id,report_amount_minor,report_currency_code,report_currency_exponent,merchant,description",
    ),
    (
        "postings",
        "transaction_id,ordinal,account_id,delta_minor,currency_code,currency_exponent",
    ),
    ("import_batches", "id,created_at_utc_micros"),
    (
        "source_evidence",
        "id,transaction_id,source_kind,external_transaction_id,import_batch_id,source_fields_digest",
    ),
    (
        "classification_rules",
        "id,priority,merchant_contains,description_contains,category_id,enabled",
    ),
    (
        "mobile_settings",
        "id,notification_wechat,notification_alipay,import_wechat,import_alipay,import_bank,api_enabled,backup_enabled,default_account",
    ),
    (
        "pending_details",
        "transaction_id,amount_minor,currency_code,currency_exponent,source,account_hint,original_external_id",
    ),
];
fn invalid() -> RepositoryError {
    RepositoryError::InvalidData("备份账本内容无效，当前账本未更改".into())
}
fn sql(_: rusqlite::Error) -> RepositoryError {
    invalid()
}
fn snapshot(connection: &Connection) -> Result<LedgerArchive, RepositoryError> {
    let mut tables = BTreeMap::new();
    for (table, columns) in TABLES {
        let mut statement = connection
            .prepare(&format!("SELECT {columns} FROM {table} ORDER BY rowid"))
            .map_err(sql)?;
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|i| {
                        Ok(match row.get_ref(i)? {
                            ValueRef::Null => ArchiveCell::Null,
                            ValueRef::Integer(n) => ArchiveCell::Integer(n),
                            ValueRef::Text(t) => ArchiveCell::Text(
                                std::str::from_utf8(t)
                                    .map_err(|_| rusqlite::Error::InvalidQuery)?
                                    .into(),
                            ),
                            ValueRef::Blob(b) => ArchiveCell::Blob(b.to_vec()),
                            ValueRef::Real(_) => return Err(rusqlite::Error::InvalidQuery),
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(sql)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql)?;
        tables.insert((*table).into(), rows);
    }
    Ok(LedgerArchive {
        schema_version: 4,
        tables,
    })
}
/// Validate every cell before SQLite can coerce or skip a nullable key. SQLite
/// rowid tables allow NULL in a TEXT PRIMARY KEY unless NOT NULL is explicit.
fn validate_rows(archive: &LedgerArchive) -> Result<(), RepositoryError> {
    // U/V: required/optional UUID; T/N: required/optional text;
    // I/J: required/optional integer; B: required binary digest.
    const TYPES: &[(&str, &str)] = &[
        ("accounts", "UTTTIII"),
        ("categories", "UTI"),
        ("transactions", "UITTVVJNJNN"),
        ("postings", "UIUITI"),
        ("import_batches", "UI"),
        ("source_evidence", "IUTNUB"),
        ("classification_rules", "UINNUI"),
        ("mobile_settings", "IIIIIIIIV"),
        ("pending_details", "UJNJTNN"),
    ];
    for (table, types) in TYPES {
        for row in archive.tables.get(*table).ok_or_else(invalid)? {
            if row.len() != types.len() {
                return Err(invalid());
            }
            for (cell, kind) in row.iter().zip(types.bytes()) {
                let valid = match (cell, kind) {
                    (ArchiveCell::Text(value), b'U' | b'V') => uuid::Uuid::parse_str(value).is_ok(),
                    (ArchiveCell::Text(_), b'T' | b'N') => true,
                    (ArchiveCell::Integer(_), b'I' | b'J') => true,
                    (ArchiveCell::Blob(value), b'B') => value.len() == 32,
                    (ArchiveCell::Null, b'V' | b'N' | b'J') => true,
                    _ => false,
                };
                if !valid {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}
fn replace(connection: &Connection, archive: &LedgerArchive) -> Result<(), RepositoryError> {
    connection
        .execute_batch("PRAGMA defer_foreign_keys=ON;")
        .map_err(sql)?;
    for (table, _) in TABLES.iter().rev() {
        connection
            .execute(&format!("DELETE FROM {table}"), [])
            .map_err(sql)?;
    }
    for (table, columns) in TABLES {
        let count = columns.split(',').count();
        let query = format!(
            "INSERT INTO {table}({columns}) VALUES ({})",
            vec!["?"; count].join(",")
        );
        let mut statement = connection.prepare(&query).map_err(sql)?;
        for row in archive.tables.get(*table).ok_or_else(invalid)? {
            if row.len() != count {
                return Err(invalid());
            }
            let values = row.iter().map(|v| match v {
                ArchiveCell::Null => Value::Null,
                ArchiveCell::Integer(n) => Value::Integer(*n),
                ArchiveCell::Text(t) => Value::Text(t.clone()),
                ArchiveCell::Blob(b) => Value::Blob(b.clone()),
            });
            statement.execute(params_from_iter(values)).map_err(sql)?;
        }
    }
    Ok(())
}
impl SqliteLedgerRepository {
    fn validate_archive(&self) -> Result<(), RepositoryError> {
        if self
            .connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .map_err(sql)?
            != "ok"
        {
            return Err(invalid());
        }
        let mut foreign = self
            .connection
            .prepare("PRAGMA foreign_key_check")
            .map_err(sql)?;
        if foreign
            .query([])
            .map_err(sql)?
            .next()
            .map_err(sql)?
            .is_some()
        {
            return Err(invalid());
        }
        let accounts = self.list_accounts()?;
        if accounts.iter().any(|a| a.display_name.trim().is_empty()) {
            return Err(invalid());
        }
        if self
            .list_categories()?
            .iter()
            .any(|c| c.name.trim().is_empty())
        {
            return Err(invalid());
        }
        self.list_rules()?;
        self.list_import_records()?;
        // SQLite's affinity is permissive; reject coerced malformed booleans and row ordinals.
        let bad: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM mobile_settings WHERE id<>1 OR notification_wechat NOT IN (0,1) OR notification_alipay NOT IN (0,1) OR import_wechat NOT IN (0,1) OR import_alipay NOT IN (0,1) OR import_bank NOT IN (0,1) OR api_enabled NOT IN (0,1) OR backup_enabled NOT IN (0,1)) OR (SELECT count(*) FROM mobile_settings)<>1 OR EXISTS(SELECT 1 FROM postings GROUP BY transaction_id HAVING min(ordinal)<>0 OR max(ordinal)<>count(*)-1)", [], |r|r.get(0)).map_err(sql)?;
        if bad {
            return Err(invalid());
        }
        self.settings()?;
        let mut batches = self
            .connection
            .prepare("SELECT id,created_at_utc_micros FROM import_batches")
            .map_err(sql)?;
        let rows = batches
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql)?;
        for row in rows {
            let (id, time) = row.map_err(sql)?;
            uuid::Uuid::parse_str(&id).map_err(|_| invalid())?;
            chrono::DateTime::from_timestamp_micros(time).ok_or_else(invalid)?;
        }
        for transaction in self.list_transactions()? {
            // Captured unresolved refunds legitimately have no parent and no postings.
            if !(transaction.status != TransactionStatus::Confirmed
                && transaction.kind == TransactionKind::Refund
                && transaction.original_transaction_id.is_none()
                && transaction.postings.is_empty())
            {
                validate_transaction_shape(&transaction, &accounts).map_err(|_| invalid())?;
            }
            if let Some(report) = &transaction.report_amount_cny
                && (report.currency().code() != "CNY"
                    || report.currency().minor_unit_exponent() != 2
                    || report.minor_units() < 0)
            {
                return Err(invalid());
            }
            if transaction.status == TransactionStatus::Confirmed
                && transaction.kind == TransactionKind::Refund
            {
                let parent = self
                    .get_transaction(transaction.original_transaction_id.ok_or_else(invalid)?)?
                    .ok_or_else(invalid)?;
                if parent.kind != TransactionKind::Expense
                    || parent.status != TransactionStatus::Confirmed
                {
                    return Err(invalid());
                }
            }
            if let Some(details) = self.pending_details(transaction.id)?
                && (!matches!(
                    details.source.as_str(),
                    "wechat" | "alipay" | "notification" | "bank_csv" | "manual"
                ) || details
                    .amount
                    .as_ref()
                    .is_some_and(|m| m.minor_units() <= 0))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
impl BackupRepository for SqliteLedgerRepository {
    fn backup_snapshot(&self) -> Result<LedgerArchive, RepositoryError> {
        let tx = self.connection.unchecked_transaction().map_err(sql)?;
        let result = snapshot(&tx)?;
        tx.commit().map_err(sql)?;
        Ok(result)
    }
    fn restore_snapshot(&self, mut archive: LedgerArchive) -> Result<(), RepositoryError> {
        let expected = match archive.schema_version {
            3 => 7,
            4 => 9,
            _ => return Err(invalid()),
        };
        if archive.tables.len() != expected
            || TABLES[..expected]
                .iter()
                .any(|(t, _)| !archive.tables.contains_key(*t))
        {
            return Err(invalid());
        }
        let temp = Self::in_memory()?;
        if archive.schema_version == 3 {
            let defaults = snapshot(&temp.connection)?;
            for table in ["mobile_settings", "pending_details"] {
                archive
                    .tables
                    .insert(table.into(), defaults.tables[table].clone());
            }
            archive.schema_version = 4;
        }
        validate_rows(&archive)?;
        {
            let tx = temp.connection.unchecked_transaction().map_err(sql)?;
            replace(&tx, &archive)?;
            tx.commit().map_err(sql)?;
        }
        temp.validate_archive()?;
        // Reject SQLite affinity coercions (e.g. text supplied for integer money).
        if snapshot(&temp.connection)? != archive {
            return Err(invalid());
        }
        // Same inode: existing UI and capture connections see the new contents.
        // BEGIN IMMEDIATE serializes capture writes; errors roll back the entire replacement.
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(sql)?;
        let permissions: (i64,i64,i64)=tx.query_row("SELECT notification_wechat,notification_alipay,api_enabled FROM mobile_settings WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
        replace(&tx, &archive)?;
        // Restoring data must not re-enable Android capture/API consent from another device.
        tx.execute("UPDATE mobile_settings SET notification_wechat=?1,notification_alipay=?2,api_enabled=?3 WHERE id=1",permissions).map_err(sql)?;
        tx.commit().map_err(sql)?;
        Ok(())
    }
}
