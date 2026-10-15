use super::*;

impl SqliteLedgerRepository {
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
}
