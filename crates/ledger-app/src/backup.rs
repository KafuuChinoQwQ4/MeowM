//! Explicit-consent backup boundary. Only allowlisted ledger data crosses this port.
use crate::{AppError, MobileLedger, MobileRepository, RepositoryError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerArchive {
    pub schema_version: u32,
    pub tables: BTreeMap<String, Vec<Vec<ArchiveCell>>>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum ArchiveCell {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}
pub trait BackupRepository: MobileRepository {
    fn backup_snapshot(&self) -> Result<LedgerArchive, RepositoryError>;
    /// Rebuild and validate in isolation, then replace atomically on the same database inode.
    fn restore_snapshot(&self, archive: LedgerArchive) -> Result<(), RepositoryError>;
}
pub trait BackupCodec {
    fn encrypt(&self, archive: &LedgerArchive, password: &str) -> Result<Vec<u8>, RepositoryError>;
    fn decrypt(&self, bytes: &[u8], password: &str) -> Result<LedgerArchive, RepositoryError>;
}
impl<R: BackupRepository> MobileLedger<R> {
    fn backup_consent(&self, confirmed: bool) -> Result<(), AppError> {
        if !confirmed || !self.settings()?.backup_enabled {
            return Err(RepositoryError::InvalidData("请先启用备份并明确确认操作".into()).into());
        }
        Ok(())
    }
    pub fn export_backup(
        &self,
        codec: &impl BackupCodec,
        password: &str,
        confirmed: bool,
    ) -> Result<Vec<u8>, AppError> {
        self.backup_consent(confirmed)?;
        Ok(codec.encrypt(&self.repository().backup_snapshot()?, password)?)
    }
    pub fn restore_backup(
        &self,
        codec: &impl BackupCodec,
        bytes: &[u8],
        password: &str,
        confirmed: bool,
    ) -> Result<(), AppError> {
        self.backup_consent(confirmed)?;
        self.repository()
            .restore_snapshot(codec.decrypt(bytes, password)?)?;
        Ok(())
    }
}
