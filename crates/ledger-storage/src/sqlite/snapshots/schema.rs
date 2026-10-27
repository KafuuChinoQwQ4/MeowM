use super::*;
// Dependency order; deferred foreign keys handle transaction/refund self references.
pub(super) const TABLES: &[&str] = &[
    "accounts",
    "categories",
    "classification_rules",
    "import_batches",
    "transactions",
    "postings",
    "transaction_metadata",
    "source_evidence",
    "budgets",
    "recurring_templates",
    "recurring_instances",
    "notification_settings",
    "app_settings",
    "audit_events",
    "import_receipt_rows",
    "notification_receipts",
    "notification_status",
    "merchant_aliases",
    "reconciliation_sessions",
    "reconciliation_entries",
    "reconciliation_events",
    "platform_metadata",
    "screen_settings",
    "screen_receipts",
    "screen_status",
    "capture_matches",
];
const MIGRATIONS: &[&str] = &[
    include_str!("../../../migrations/0001_initial.sql"),
    include_str!("../../../migrations/0002_classification_rules.sql"),
    include_str!("../../../migrations/0003_optional_balance.sql"),
    include_str!("../../../migrations/0004_nine_features.sql"),
    include_str!("../../../migrations/0005_pending_import.sql"),
    include_str!("../../../migrations/0006_notifications.sql"),
    include_str!("../../../migrations/0007_recurring.sql"),
    include_str!("../../../migrations/0008_search_rules.sql"),
    include_str!("../../../migrations/0009_reconciliation.sql"),
    include_str!("../../../migrations/0010_screen_capture.sql"),
    include_str!("../../../migrations/0011_refund_references.sql"),
];
pub(super) fn exists(c: &Connection, name: &str) -> Result<bool, RepositoryError> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )
    .map_err(storage_error)
}
pub(super) fn columns(c: &Connection, name: &str) -> Result<Vec<String>, RepositoryError> {
    c.prepare(&format!("PRAGMA table_info({name})"))
        .map_err(storage_error)?
        .query_map([], |r| r.get(1))
        .map_err(storage_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(storage_error)
}
pub(super) fn empty_at(version: i64) -> Result<Connection, RepositoryError> {
    if !(1..=11).contains(&version) {
        return Err(invalid());
    }
    let c = Connection::open_in_memory().map_err(storage_error)?;
    // All tables are empty while trusted migrations rebuild their shapes.
    for (i, sql) in MIGRATIONS.iter().take(version as usize).enumerate() {
        c.execute_batch(sql).map_err(storage_error)?;
        c.execute("INSERT INTO schema_version VALUES(?1,0)", [i as i64 + 1])
            .map_err(storage_error)?;
    }
    c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE platform_metadata(account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE, category TEXT NOT NULL CHECK(category IN ('wallet','bank','securities','exchange','cash','other')));").map_err(storage_error)?;
    Ok(c)
}
pub(super) fn insert(c: &Connection, data: &LedgerSnapshot) -> Result<(), RepositoryError> {
    let mut expected = Vec::new();
    for name in TABLES {
        if exists(c, name)? {
            expected.push(*name);
        }
    }
    if data.tables.len() != expected.len() {
        return Err(invalid());
    }
    let mut count = 0;
    for (table, trusted) in data.tables.iter().zip(expected) {
        if table.name != trusted || table.columns != columns(c, trusted)? {
            return Err(invalid());
        }
        let placeholders = vec!["?"; table.columns.len()].join(",");
        let mut stmt = c
            .prepare(&format!("INSERT INTO {trusted} VALUES({placeholders})"))
            .map_err(storage_error)?;
        for row in &table.rows {
            count += 1;
            if count > MAX_ROWS || row.len() != table.columns.len() {
                return Err(invalid());
            }
            if trusted == "app_settings"
                && !matches!(row.first(), Some(Cell::Text(k)) if k=="hide_amounts")
            {
                return Err(invalid());
            }
            for (column, value) in table.columns.iter().zip(row) {
                validate_cell(trusted, column, value)?;
            }
            stmt.execute(rusqlite::params_from_iter(row.iter().map(Cell::value)))
                .map_err(storage_error)?;
        }
    }
    Ok(())
}

// SQLite affinity alone does not reject malformed IDs or out-of-range timestamps.
fn validate_cell(table: &str, column: &str, value: &Cell) -> Result<(), RepositoryError> {
    let uuid_id = (column == "id"
        && !["source_evidence", "reconciliation_events"].contains(&table))
        || (column.ends_with("_id")
            && ![
                "external_transaction_id",
                "original_external_transaction_id",
            ]
            .contains(&column))
        || column == "reversal_of";
    if column == "id" && matches!(value, Cell::Null) {
        return Err(invalid());
    }
    if uuid_id
        && !matches!(value, Cell::Null)
        && !matches!(value, Cell::Text(v) if Uuid::parse_str(v).is_ok())
    {
        return Err(invalid());
    }
    if column.ends_with("utc_micros")
        && !matches!(value, Cell::Null)
        && !matches!(value,Cell::Integer(v) if DateTime::from_timestamp_micros(*v).is_some())
    {
        return Err(invalid());
    }
    if column.contains("digest") && !matches!(value,Cell::Blob(v) if v.len()==32) {
        return Err(invalid());
    }
    Ok(())
}
