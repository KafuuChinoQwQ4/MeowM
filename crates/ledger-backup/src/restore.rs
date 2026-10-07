//! SQLite implementation rebuilds the decrypted archive in a temporary database.
use ledger_app::{LedgerArchive, RepositoryError};
pub fn check_version(archive: &LedgerArchive) -> Result<(), RepositoryError> {
    if !matches!(archive.schema_version, 3 | 4) {
        return Err(RepositoryError::InvalidData("不支持此备份账本版本".into()));
    }
    Ok(())
}
