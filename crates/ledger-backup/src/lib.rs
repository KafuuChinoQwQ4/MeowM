//! Password-encrypted portable backups; plaintext is never written to disk.
pub mod archive;
mod crypto;
mod restore;
pub use archive::{BackupInfo, InspectedBackup};
use ledger_app::RepositoryError;
#[derive(Debug)]
pub enum BackupError {
    Password,
    Authentication,
    Format,
    Version,
    InvalidData,
    Changed,
    Storage,
}
impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Password => "备份密码须为 8 至 1024 字节，请妥善保存，无法找回",
            Self::Authentication => "密码错误或备份已损坏，请检查密码和文件",
            Self::Format => "不是有效的 MeowM 备份，或文件超过 32 MB",
            Self::Version => "此备份版本暂不支持，请升级应用",
            Self::InvalidData => "备份数据校验失败，原账本未被替换",
            Self::Changed => "预览后账本已变化，请重新选择备份并预览",
            Self::Storage => "无法读取或写入账本，请稍后重试",
        })
    }
}
impl std::error::Error for BackupError {}
impl From<RepositoryError> for BackupError {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::Conflict => Self::Changed,
            RepositoryError::InvalidData(_) => Self::InvalidData,
            _ => Self::Storage,
        }
    }
}
pub struct BackupService;
