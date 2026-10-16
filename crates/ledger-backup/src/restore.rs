use crate::{BackupError, BackupService, InspectedBackup};
use ledger_storage::sqlite::SqliteLedgerRepository;
impl BackupService {
    pub fn restore(
        repo: &SqliteLedgerRepository,
        backup: &InspectedBackup,
        expected: [u8; 32],
    ) -> Result<(), BackupError> {
        repo.replace_snapshot(&backup.snapshot, expected)
            .map_err(Into::into)
    }
}
