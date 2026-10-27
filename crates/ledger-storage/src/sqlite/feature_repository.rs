use super::*;

impl ledger_app::FeatureRepository for SqliteLedgerRepository {
    fn query_transactions(
        &self,
        q: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, RepositoryError> {
        self.search_transactions(q)
    }

    fn confirm_bulk(
        &self,
        request: ledger_domain::BulkConfirmation,
    ) -> Result<(), RepositoryError> {
        self.confirm_bulk_with_edits(request, Vec::new())
    }
    fn confirm_bulk_with_edits(
        &self,
        request: ledger_domain::BulkConfirmation,
        edits: Vec<ledger_domain::TransactionEdit>,
    ) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let ids = request.transaction_ids;
        // A filtered confirm-all can span more than one query page. Keep the
        // complete selection in this transaction so a later failure rolls back all.
        if edits.iter().any(|edit| !ids.contains(&edit.transaction.id)) {
            return Err(RepositoryError::InvalidData(
                "invalid confirmation selection".into(),
            ));
        }
        let mut seen_edits = std::collections::HashSet::new();
        for edit in &edits {
            if !seen_edits.insert(edit.transaction.id) {
                return Err(RepositoryError::Conflict);
            }
        }
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let current = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
            let edit = edits.iter().find(|edit| edit.transaction.id == id);
            if current.status == TransactionStatus::Confirmed {
                if let Some(edit) = edit {
                    let mut proposed = edit.transaction.clone();
                    proposed.status = TransactionStatus::Confirmed;
                    if proposed != current {
                        return Err(RepositoryError::Conflict);
                    }
                }
                continue;
            }
            if current.status != TransactionStatus::Pending {
                return Err(RepositoryError::Conflict);
            }
            let mut proposed = if let Some(edit) = edit {
                if edit.transaction.status != TransactionStatus::Pending
                    || self.metadata(id)?.edit_version != edit.expected_version
                {
                    return Err(RepositoryError::Conflict);
                }
                edit.transaction.clone()
            } else {
                current
            };
            proposed.status = TransactionStatus::Confirmed;
            self.validate_feature_transaction(&proposed)?;
            Self::write_transaction(&tx, &proposed, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn ignore_bulk(&self, ids: &[TransactionId]) -> Result<(), RepositoryError> {
        if ids.len() > 200 {
            return Err(RepositoryError::InvalidData(
                "selection is too large".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        for id in ids {
            let mut current = self
                .get_transaction(*id)?
                .ok_or(RepositoryError::NotFound)?;
            if current.status == TransactionStatus::Confirmed {
                return Err(RepositoryError::Conflict);
            }
            if current.status == TransactionStatus::Pending {
                current.status = TransactionStatus::Ignored;
                Self::write_transaction(&tx, &current, false)?;
            }
        }
        tx.commit().map_err(storage_error)
    }
    fn restore_pending_atomic(&self, id: TransactionId) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut t = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if t.status == TransactionStatus::Confirmed {
            return Err(RepositoryError::Conflict);
        }
        if t.status == TransactionStatus::Ignored {
            t.status = TransactionStatus::Pending;
            Self::write_transaction(&tx, &t, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn ignore_atomic(&self, id: TransactionId) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let mut t = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if t.status == TransactionStatus::Confirmed {
            return Err(RepositoryError::Conflict);
        }
        if t.status == TransactionStatus::Pending {
            t.status = TransactionStatus::Ignored;
            Self::write_transaction(&tx, &t, false)?;
        }
        tx.commit().map_err(storage_error)
    }
    fn reverse_atomic(
        &self,
        id: TransactionId,
        new_id: TransactionId,
        at: Timestamp,
    ) -> Result<TransactionId, RepositoryError> {
        self.reverse_named(id, new_id, at, "reversal")
    }
    fn edit_atomic(&self, e: ledger_domain::TransactionEdit) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let old = self
            .get_transaction(e.transaction.id)?
            .ok_or(RepositoryError::NotFound)?;
        let m = self.metadata(old.id)?;
        let reversed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM transaction_metadata WHERE reversal_of=?1)",
                [old.id.as_uuid().to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if m.edit_version != e.expected_version
            || m.reversal_of.is_some()
            || reversed
            || old.status != e.transaction.status
        {
            return Err(RepositoryError::Conflict);
        }
        if old.status == TransactionStatus::Confirmed
            && (old.kind != e.transaction.kind
                || old.original_transaction_id != e.transaction.original_transaction_id)
        {
            return Err(RepositoryError::Conflict);
        }
        if self.has_active_refunds(old.id)?
            && (e.transaction.kind != TransactionKind::Expense
                || e.transaction.postings != old.postings)
        {
            return Err(RepositoryError::InvalidData(
                "expense has active refunds".into(),
            ));
        }
        self.validate_feature_transaction(&e.transaction)?;
        Self::write_transaction(&tx, &e.transaction, false)?;
        tx.commit().map_err(storage_error)
    }
    fn source_evidence(&self, id: TransactionId) -> Result<Vec<SourceEvidence>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT source_kind,external_transaction_id,import_batch_id,source_fields_digest,original_external_transaction_id FROM source_evidence WHERE transaction_id=?1 ORDER BY id").map_err(storage_error)?;
        stmt.query_map([id.as_uuid().to_string()], |r| {
            let k: String = r.get(0)?;
            let batch: String = r.get(2)?;
            let digest: Vec<u8> = r.get(3)?;
            Ok(SourceEvidence {
                source_kind: match k.as_str() {
                    "alipay" => SourceKind::Alipay,
                    "wechat" => SourceKind::WeChat,
                    "bank" => SourceKind::Bank,
                    "notification" => SourceKind::Notification,
                    "screen_capture" => SourceKind::ScreenCapture,
                    "manual" => SourceKind::Manual,
                    "recurring" => SourceKind::Recurring,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                },
                external_transaction_id: r.get(1)?,
                original_external_transaction_id: r.get(4)?,
                import_batch_id: Uuid::parse_str(&batch)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                source_fields_digest: digest
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })
        .map_err(storage_error)?
        .collect::<Result<_, _>>()
        .map_err(storage_error)
    }
    fn privacy_settings(&self) -> Result<ledger_domain::PrivacySettings, RepositoryError> {
        let get = |key: &str| {
            self.connection
                .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .optional()
                .map_err(storage_error)
        };
        Ok(ledger_domain::PrivacySettings {
            hide_amounts: get("hide_amounts")?.as_deref() == Some("1"),
            lock_enabled: get("lock_enabled")?.as_deref() == Some("1"),
            password_verifier: get("password_verifier")?,
            auto_lock_minutes: get("auto_lock_minutes")?
                .as_deref()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
        })
    }
    fn save_privacy_settings(
        &self,
        s: ledger_domain::PrivacySettings,
    ) -> Result<(), RepositoryError> {
        if !(1..=120).contains(&s.auto_lock_minutes)
            || (s.lock_enabled
                && !s
                    .password_verifier
                    .as_deref()
                    .is_some_and(|v| v.starts_with("$argon2id$")))
            || (!s.lock_enabled && s.password_verifier.is_some())
        {
            return Err(RepositoryError::InvalidData(
                "invalid privacy settings".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        for (key, value) in [
            ("hide_amounts", if s.hide_amounts { "1" } else { "0" }),
            ("lock_enabled", if s.lock_enabled { "1" } else { "0" }),
            ("auto_lock_minutes", ""),
        ] {
            let value = if key == "auto_lock_minutes" {
                s.auto_lock_minutes.to_string()
            } else {
                value.into()
            };
            tx.execute("INSERT INTO app_settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(storage_error)?;
        }
        tx.execute("DELETE FROM app_settings WHERE key='password_verifier'", [])
            .map_err(storage_error)?;
        if let Some(verifier) = s.password_verifier {
            tx.execute(
                "INSERT INTO app_settings VALUES('password_verifier',?1)",
                [verifier],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }
}
