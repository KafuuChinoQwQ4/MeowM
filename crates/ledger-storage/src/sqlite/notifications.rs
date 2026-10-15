//! Source settings, normalized status, and atomic pending notification receipts.
use super::*;
use ledger_app::{
    notifications::{
        CaptureResult, CaptureStatus, NOTIFICATION_PACKAGES, NotificationRecord,
        NotificationSettings,
    },
    ports::NotificationRepository,
};
impl NotificationRepository for SqliteLedgerRepository {
    fn notification_settings(&self) -> Result<Vec<NotificationSettings>, RepositoryError> {
        NOTIFICATION_PACKAGES.iter().map(|package| {
            self.connection.query_row("SELECT enabled,account_id FROM notification_settings WHERE package_name=?1", [package], |r| {
                let account: Option<String> = r.get(1)?;
                Ok(NotificationSettings { package_name: (*package).into(), enabled: r.get(0)?,
                    account_id: account.map(|s| Uuid::parse_str(&s).map(AccountId::from_uuid)).transpose().map_err(|_| rusqlite::Error::InvalidQuery)? })
            }).optional().map(|s| s.unwrap_or_else(|| NotificationSettings::disabled(package))).map_err(storage_error)
        }).collect()
    }
    fn save_notification_settings(
        &self,
        settings: NotificationSettings,
    ) -> Result<(), RepositoryError> {
        if !NOTIFICATION_PACKAGES.contains(&settings.package_name.as_str()) {
            return Err(RepositoryError::InvalidData(
                "unsupported notification source".into(),
            ));
        }
        if let Some(id) = settings.account_id {
            let account = self.get_account(id)?.ok_or(RepositoryError::NotFound)?;
            if account.currency.code() != "CNY" || account.currency.minor_unit_exponent() != 2 {
                return Err(RepositoryError::InvalidData(
                    "notification source requires a CNY account".into(),
                ));
            }
        }
        self.connection.execute("INSERT INTO notification_settings(package_name,enabled,account_id) VALUES(?1,?2,?3) ON CONFLICT(package_name) DO UPDATE SET enabled=excluded.enabled,account_id=excluded.account_id",
            params![settings.package_name, settings.enabled, settings.account_id.map(|id| id.as_uuid().to_string())]).map_err(storage_error)?;
        Ok(())
    }
    fn notification_status(&self) -> Result<Vec<CaptureStatus>, RepositoryError> {
        let mut stmt = self.connection.prepare("SELECT package_name,code,received_at_ms,transaction_id FROM notification_status ORDER BY package_name DESC").map_err(storage_error)?;
        stmt.query_map([], |row| {
            let id: Option<String> = row.get(3)?;
            Ok(CaptureStatus {
                package_name: row.get(0)?,
                code: row.get(1)?,
                received_at_ms: row.get(2)?,
                transaction_id: id
                    .map(|s| Uuid::parse_str(&s).map(TransactionId::from_uuid))
                    .transpose()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<_, _>>()
        .map_err(storage_error)
    }
    fn record_notification(
        &self,
        record: NotificationRecord,
    ) -> Result<CaptureResult, RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let settings = self
            .notification_settings()?
            .into_iter()
            .find(|s| s.package_name == record.package_name);
        if settings.as_ref() != Some(&record.settings) || !record.settings.enabled {
            return Ok(CaptureResult {
                code: "ignored".into(),
                transaction_id: None,
            });
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT transaction_id FROM notification_receipts WHERE identity=?1 OR digest=?2",
                params![record.identity, record.digest.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let result = if let Some(id) = existing {
            CaptureResult {
                code: "duplicate".into(),
                transaction_id: Some(TransactionId::from_uuid(
                    Uuid::parse_str(&id)
                        .map_err(|_| RepositoryError::InvalidData("invalid receipt".into()))?,
                )),
            }
        } else if let Some((candidate, evidence)) = record.candidate {
            if candidate.status != TransactionStatus::Pending
                || evidence.source_kind != SourceKind::Notification
                || evidence.source_fields_digest != record.digest
                || evidence.external_transaction_id.as_deref() != Some(record.identity.as_str())
            {
                return Err(RepositoryError::InvalidData(
                    "invalid notification candidate".into(),
                ));
            }
            self.validate_feature_transaction(&candidate)?;
            Self::write_transaction_with_evidence(&tx, &candidate, Some(&evidence))?;
            tx.execute("INSERT INTO notification_receipts(identity,digest,transaction_id) VALUES(?1,?2,?3)", params![record.identity, record.digest.as_slice(), candidate.id.as_uuid().to_string()]).map_err(map_insert_error)?;
            CaptureResult {
                code: record.code,
                transaction_id: Some(candidate.id),
            }
        } else {
            CaptureResult {
                code: "ignored".into(),
                transaction_id: None,
            }
        };
        tx.execute("INSERT INTO notification_status(package_name,code,received_at_ms,transaction_id) VALUES(?1,?2,?3,?4) ON CONFLICT(package_name) DO UPDATE SET code=excluded.code,received_at_ms=excluded.received_at_ms,transaction_id=excluded.transaction_id",
            params![record.package_name, result.code, Utc::now().timestamp_millis(), result.transaction_id.map(|id| id.as_uuid().to_string())]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
