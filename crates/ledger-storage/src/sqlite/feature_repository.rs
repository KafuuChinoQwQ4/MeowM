use super::*;

impl ledger_app::FeatureRepository for SqliteLedgerRepository {
    fn query_transactions(
        &self,
        q: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, RepositoryError> {
        if !(1..=200).contains(&q.limit)
            || q.from_utc_inclusive
                .zip(q.until_utc_exclusive)
                .is_some_and(|(a, b)| a >= b)
        {
            return Err(RepositoryError::InvalidData(
                "invalid page or date range".into(),
            ));
        }
        let sql = "FROM transactions t WHERE (?1 IS NULL OR status=?1) AND (?2 IS NULL OR occurred_at_utc_micros>=?2) AND (?3 IS NULL OR occurred_at_utc_micros<?3) AND (?4 IS NULL OR EXISTS(SELECT 1 FROM postings p WHERE p.transaction_id=t.id AND p.account_id=?4)) AND (?5 IS NULL OR category_id=?5) AND (?6 IS NULL OR instr(lower(coalesce(merchant,'')),lower(?6))>0) AND (?7 IS NULL OR instr(lower(coalesce(merchant,'')||' '||coalesce(description,'')),lower(?7))>0) AND (?8 IS NULL OR EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind=?8))";
        let values = params![
            q.status.map(transaction_status_str),
            q.from_utc_inclusive.map(|t| t.timestamp_micros()),
            q.until_utc_exclusive.map(|t| t.timestamp_micros()),
            q.account_id.map(|v| v.as_uuid().to_string()),
            q.category_id.map(|v| v.as_uuid().to_string()),
            q.merchant,
            q.keyword,
            q.source_kind.map(source_kind_str)
        ];
        let snapshot = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let total = self
            .connection
            .query_row(&format!("SELECT count(*) {sql}"), values, |r| r.get(0))
            .map_err(storage_error)?;
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT id {sql} ORDER BY occurred_at_utc_micros DESC,id LIMIT {} OFFSET {}",
                q.limit, q.offset
            ))
            .map_err(storage_error)?;
        let ids = stmt
            .query_map(values, |r| r.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let items = ids
            .into_iter()
            .map(|v| {
                let id = TransactionId::from_uuid(
                    Uuid::parse_str(&v)
                        .map_err(|_| RepositoryError::InvalidData("invalid ID".into()))?,
                );
                Ok(ledger_domain::PendingCandidate {
                    transaction: self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?,
                    metadata: self.metadata(id)?,
                })
            })
            .collect::<Result<_, RepositoryError>>()?;
        drop(stmt);
        snapshot.commit().map_err(storage_error)?;
        Ok(ledger_domain::TransactionPage {
            items,
            total,
            offset: q.offset,
            limit: q.limit,
        })
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
        if ids.len() > 200 || edits.iter().any(|edit| !ids.contains(&edit.transaction.id)) {
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
        let mut stmt=self.connection.prepare("SELECT source_kind,external_transaction_id,import_batch_id,source_fields_digest FROM source_evidence WHERE transaction_id=?1 ORDER BY id").map_err(storage_error)?;
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
                    "manual" => SourceKind::Manual,
                    "recurring" => SourceKind::Recurring,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                },
                external_transaction_id: r.get(1)?,
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
        })
    }
    fn save_privacy_settings(
        &self,
        s: ledger_domain::PrivacySettings,
    ) -> Result<(), RepositoryError> {
        // Password verifier format and lock guard are deliberately deferred. Never enable an unguarded lock.
        if s.lock_enabled || s.password_verifier.is_some() {
            return Err(RepositoryError::InvalidData(
                "app lock is not implemented yet".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        for (key, value) in [
            ("hide_amounts", if s.hide_amounts { "1" } else { "0" }),
            ("lock_enabled", "0"),
        ] {
            tx.execute("INSERT INTO app_settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }
}
