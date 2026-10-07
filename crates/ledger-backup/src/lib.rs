//! Password protected local backups. No password is persisted or logged.
pub mod archive;
pub mod crypto;
pub mod restore;
use ledger_app::{BackupCodec, LedgerArchive, RepositoryError};
use zeroize::Zeroizing;
pub struct EncryptedBackup;
impl BackupCodec for EncryptedBackup {
    fn encrypt(&self, archive: &LedgerArchive, password: &str) -> Result<Vec<u8>, RepositoryError> {
        let bytes = Zeroizing::new(
            serde_json::to_vec(archive)
                .map_err(|_| RepositoryError::InvalidData("无法编码备份".into()))?,
        );
        crypto::seal(&bytes, password)
    }
    fn decrypt(&self, bytes: &[u8], password: &str) -> Result<LedgerArchive, RepositoryError> {
        let plaintext = crypto::open(bytes, password)?;
        let archive: LedgerArchive = serde_json::from_slice(&plaintext)
            .map_err(|_| RepositoryError::InvalidData("备份格式无效".into()))?;
        restore::check_version(&archive)?;
        Ok(archive)
    }
}
