//! Source consent, normalized status and atomic capture receipts.
use super::*;
use ledger_app::{
    notifications::{
        CaptureResult, CaptureStatus, NOTIFICATION_PACKAGES, NotificationRecord,
        NotificationSettings,
    },
    ports::{NotificationRepository, ScreenCaptureRepository},
};

impl NotificationRepository for SqliteLedgerRepository {
    fn notification_settings(&self) -> Result<Vec<NotificationSettings>, RepositoryError> {
        self.capture_settings("notification_settings")
    }
    fn save_notification_settings(&self, s: NotificationSettings) -> Result<(), RepositoryError> {
        self.save_capture_settings("notification_settings", s)
    }
    fn notification_status(&self) -> Result<Vec<CaptureStatus>, RepositoryError> {
        self.capture_status("notification_status")
    }
    fn record_notification(
        &self,
        record: NotificationRecord,
    ) -> Result<CaptureResult, RepositoryError> {
        self.record_capture(record, SourceKind::Notification)
    }
}
impl ScreenCaptureRepository for SqliteLedgerRepository {
    fn screen_settings(&self) -> Result<Vec<NotificationSettings>, RepositoryError> {
        self.capture_settings("screen_settings")
    }
    fn save_screen_settings(&self, s: NotificationSettings) -> Result<(), RepositoryError> {
        self.save_capture_settings("screen_settings", s)
    }
    fn screen_status(&self) -> Result<Vec<CaptureStatus>, RepositoryError> {
        self.capture_status("screen_status")
    }
    fn record_screen_capture(
        &self,
        record: NotificationRecord,
    ) -> Result<CaptureResult, RepositoryError> {
        self.record_capture(record, SourceKind::ScreenCapture)
    }
}
impl SqliteLedgerRepository {
    // Table names below are private constants, never IPC input.
    fn capture_settings(&self, table: &str) -> Result<Vec<NotificationSettings>, RepositoryError> {
        NOTIFICATION_PACKAGES
            .iter()
            .map(|package| {
                self.connection
                    .query_row(
                        &format!("SELECT enabled,account_id FROM {table} WHERE package_name=?1"),
                        [package],
                        |r| {
                            let account: Option<String> = r.get(1)?;
                            Ok(NotificationSettings {
                                package_name: (*package).into(),
                                enabled: r.get(0)?,
                                account_id: account
                                    .map(|s| Uuid::parse_str(&s).map(AccountId::from_uuid))
                                    .transpose()
                                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            })
                        },
                    )
                    .optional()
                    .map(|s| s.unwrap_or_else(|| NotificationSettings::disabled(package)))
                    .map_err(storage_error)
            })
            .collect()
    }
    fn save_capture_settings(
        &self,
        table: &str,
        s: NotificationSettings,
    ) -> Result<(), RepositoryError> {
        if !NOTIFICATION_PACKAGES.contains(&s.package_name.as_str()) {
            return Err(RepositoryError::InvalidData(
                "unsupported capture source".into(),
            ));
        }
        if let Some(id) = s.account_id {
            let a = self.get_account(id)?.ok_or(RepositoryError::NotFound)?;
            if a.currency.code() != "CNY" || a.currency.minor_unit_exponent() != 2 {
                return Err(RepositoryError::InvalidData(
                    "capture requires a CNY account".into(),
                ));
            }
        }
        self.connection.execute(&format!("INSERT INTO {table}(package_name,enabled,account_id) VALUES(?1,?2,?3) ON CONFLICT(package_name) DO UPDATE SET enabled=excluded.enabled,account_id=excluded.account_id"),params![s.package_name,s.enabled,s.account_id.map(|id|id.as_uuid().to_string())]).map_err(storage_error)?;
        Ok(())
    }
    fn capture_status(&self, table: &str) -> Result<Vec<CaptureStatus>, RepositoryError> {
        let mut stmt = self.connection.prepare(&format!("SELECT package_name,code,received_at_ms,transaction_id FROM {table} ORDER BY package_name")).map_err(storage_error)?;
        stmt.query_map([], |r| {
            let id: Option<String> = r.get(3)?;
            Ok(CaptureStatus {
                package_name: r.get(0)?,
                code: r.get(1)?,
                received_at_ms: r.get(2)?,
                transaction_id: id
                    .map(|s| Uuid::parse_str(&s).map(TransactionId::from_uuid))
                    .transpose()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })
        .map_err(storage_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(storage_error)
    }
    fn record_capture(
        &self,
        record: NotificationRecord,
        source: SourceKind,
    ) -> Result<CaptureResult, RepositoryError> {
        let (settings_table, receipts, status_table) = if source == SourceKind::Notification {
            (
                "notification_settings",
                "notification_receipts",
                "notification_status",
            )
        } else {
            ("screen_settings", "screen_receipts", "screen_status")
        };
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let settings = self
            .capture_settings(settings_table)?
            .into_iter()
            .find(|s| s.package_name == record.package_name);
        if settings.as_ref() != Some(&record.settings) || !record.settings.enabled {
            return Ok(CaptureResult {
                code: "ignored".into(),
                transaction_id: None,
            });
        }
        if record.source_kind != source || record.identity.len() != 64 {
            return Err(RepositoryError::InvalidData(
                "invalid capture source".into(),
            ));
        }
        if let Some((candidate, evidence)) = &record.candidate {
            if candidate.status != TransactionStatus::Pending
                || evidence.source_kind != source
                || evidence.source_fields_digest != record.digest
                || evidence.external_transaction_id.as_deref() != Some(&record.identity)
            {
                return Err(RepositoryError::InvalidData(
                    "invalid capture candidate".into(),
                ));
            }
            self.validate_feature_transaction(candidate)?;
        }
        // Notification callbacks may change key but keep content; screen identities are per visible payment episode.
        let query = if source == SourceKind::Notification {
            format!("SELECT transaction_id FROM {receipts} WHERE identity=?1 OR digest=?2")
        } else {
            format!("SELECT transaction_id FROM {receipts} WHERE identity=?1 AND ?2 IS NOT NULL")
        };
        let existing: Option<String> = tx
            .query_row(
                &query,
                params![record.identity, record.digest.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let result = if let Some(id) = existing {
            CaptureResult {
                code: "duplicate".into(),
                transaction_id: Some(parse_capture_id(&id)?),
            }
        } else if let Some((candidate, evidence)) = &record.candidate {
            let matched = self.match_other_capture(&tx, &record, candidate, source)?;
            // Amount, merchant and time cannot prove that two callbacks describe one payment.
            // Preserve both candidates; only the stable source identity above may suppress a write.
            let id = candidate.id;
            let id_string = id.as_uuid().to_string();
            let mut candidate = candidate.clone();
            if let Some(other) = matched {
                candidate.description = Some(format!(
                    "{}；疑似与另一来源记录 {} 重复，请核对后确认或忽略",
                    candidate.description.as_deref().unwrap_or("支付候选"),
                    other.as_uuid()
                ));
            }
            Self::write_transaction_with_evidence(&tx, &candidate, Some(evidence))?;
            tx.execute(
                &format!("INSERT INTO {receipts}(identity,digest,transaction_id) VALUES(?1,?2,?3)"),
                params![record.identity, record.digest.as_slice(), id_string],
            )
            .map_err(storage_error)?;
            tx.execute("INSERT INTO capture_matches(source_kind,package_name,transaction_id,captured_at_ms) VALUES(?1,?2,?3,?4)",params![source_kind_str(source),record.package_name,id_string,candidate.occurred_at.timestamp_millis()]).map_err(storage_error)?;
            CaptureResult {
                code: if matched.is_some() {
                    "needs_confirmation".into()
                } else {
                    record.code
                },
                transaction_id: Some(id),
            }
        } else {
            CaptureResult {
                code: "ignored".into(),
                transaction_id: None,
            }
        };
        tx.execute(&format!("INSERT INTO {status_table}(package_name,code,received_at_ms,transaction_id) VALUES(?1,?2,?3,?4) ON CONFLICT(package_name) DO UPDATE SET code=excluded.code,received_at_ms=excluded.received_at_ms,transaction_id=excluded.transaction_id"),params![record.package_name,result.code,Utc::now().timestamp_millis(),result.transaction_id.map(|id|id.as_uuid().to_string())]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn match_other_capture(
        &self,
        tx: &SqlTransaction<'_>,
        record: &NotificationRecord,
        candidate: &Transaction,
        source: SourceKind,
    ) -> Result<Option<TransactionId>, RepositoryError> {
        let (Some(merchant), Some(amount), Some(posting)) = (
            candidate.merchant.as_deref(),
            candidate.report_amount_cny.as_ref(),
            candidate.postings.first(),
        ) else {
            return Ok(None);
        };
        if merchant.trim().is_empty()
            || !matches!(
                candidate.kind,
                TransactionKind::Expense | TransactionKind::Income
            )
        {
            return Ok(None);
        }
        let at = candidate.occurred_at.timestamp_millis();
        let mut stmt=tx.prepare("SELECT DISTINCT t.id FROM capture_matches m JOIN transactions t ON t.id=m.transaction_id JOIN postings p ON p.transaction_id=t.id WHERE m.source_kind!=?1 AND m.package_name=?2 AND m.captured_at_ms BETWEEN ?3 AND ?4 AND t.status IN ('pending','confirmed') AND t.kind=?5 AND t.merchant=?6 AND t.report_amount_minor=?7 AND p.account_id=?8 AND p.delta_minor=?9 AND NOT EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind=?1) AND NOT EXISTS(SELECT 1 FROM transaction_metadata rev WHERE rev.reversal_of=t.id) LIMIT 2").map_err(storage_error)?;
        let ids = stmt
            .query_map(
                params![
                    source_kind_str(source),
                    record.package_name,
                    at.saturating_sub(10_000),
                    at.saturating_add(10_000),
                    transaction_kind_str(candidate.kind),
                    merchant,
                    amount.minor_units(),
                    posting.account_id.as_uuid().to_string(),
                    posting.delta.minor_units()
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage_error)?;
        if ids.len() == 1 {
            Ok(Some(parse_capture_id(&ids[0])?))
        } else {
            Ok(None)
        }
    }
}
fn parse_capture_id(id: &str) -> Result<TransactionId, RepositoryError> {
    Uuid::parse_str(id)
        .map(TransactionId::from_uuid)
        .map_err(|_| RepositoryError::InvalidData("invalid capture receipt".into()))
}
