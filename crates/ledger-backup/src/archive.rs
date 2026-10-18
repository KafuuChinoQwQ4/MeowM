use crate::{BackupError, BackupService, crypto};
use ledger_storage::sqlite::{LedgerSnapshot, SqliteLedgerRepository};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    format_version: u32,
    created_at: String,
    checksum: [u8; 32],
    snapshot: LedgerSnapshot,
}
#[derive(Clone, Serialize)]
pub struct BackupInfo {
    pub format_version: u32,
    pub schema_version: i64,
    pub created_at: String,
    pub counts: Vec<(String, usize)>,
}
pub struct InspectedBackup {
    pub info: BackupInfo,
    pub(crate) snapshot: LedgerSnapshot,
}
impl BackupService {
    pub fn create(repo: &SqliteLedgerRepository, password: &str) -> Result<Vec<u8>, BackupError> {
        let snapshot = repo.snapshot()?;
        // A backup must be restorable before it is offered to the user.
        SqliteLedgerRepository::from_snapshot(&snapshot).map_err(|_| BackupError::InvalidData)?;
        let archive = Archive {
            format_version: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            checksum: snapshot.fingerprint()?,
            snapshot,
        };
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&archive).map_err(|_| BackupError::Format)?);
        crypto::encrypt(&plaintext, password)
    }
    pub fn inspect(data: &[u8], password: &str) -> Result<InspectedBackup, BackupError> {
        let plaintext = crypto::decrypt(data, password)?;
        let archive: Archive =
            serde_json::from_slice(&plaintext).map_err(|_| BackupError::InvalidData)?;
        if archive.format_version != 1 || !(1..=10).contains(&archive.snapshot.schema_version) {
            return Err(BackupError::Version);
        }
        if archive.snapshot.fingerprint()? != archive.checksum
            || chrono::DateTime::parse_from_rfc3339(&archive.created_at).is_err()
        {
            return Err(BackupError::InvalidData);
        }
        let validated = SqliteLedgerRepository::from_snapshot(&archive.snapshot)
            .map_err(|_| BackupError::InvalidData)?;
        let snapshot = validated.snapshot()?;
        Ok(InspectedBackup {
            info: BackupInfo {
                format_version: 1,
                schema_version: archive.snapshot.schema_version,
                created_at: archive.created_at,
                counts: snapshot.counts(),
            },
            snapshot,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_schema_is_migrated_but_unknown_version_is_rejected() {
        let repo = SqliteLedgerRepository::in_memory().unwrap();
        let mut data = serde_json::to_value(repo.snapshot().unwrap()).unwrap();
        data["schema_version"] = 3.into();
        let old = [
            "accounts",
            "categories",
            "classification_rules",
            "import_batches",
            "transactions",
            "postings",
            "source_evidence",
            "platform_metadata",
        ];
        data["tables"]
            .as_array_mut()
            .unwrap()
            .retain(|t| old.contains(&t["name"].as_str().unwrap()));
        for t in data["tables"].as_array_mut().unwrap() {
            if t["name"] == "transactions" {
                t["columns"].as_array_mut().unwrap().pop();
            }
        }
        let snapshot: LedgerSnapshot = serde_json::from_value(data).unwrap();
        let a = Archive {
            format_version: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            checksum: snapshot.fingerprint().unwrap(),
            snapshot,
        };
        let bytes = crypto::encrypt(&serde_json::to_vec(&a).unwrap(), "test-only-pass").unwrap();
        let checked = BackupService::inspect(&bytes, "test-only-pass").unwrap();
        assert_eq!(checked.info.schema_version, 3);
        assert_eq!(checked.snapshot.schema_version, 10);
        let mut bytes = bytes;
        bytes[8] = 2;
        assert!(matches!(
            BackupService::inspect(&bytes, "test-only-pass"),
            Err(BackupError::Version)
        ));
    }
}
