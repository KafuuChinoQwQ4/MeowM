use super::*;

impl SqliteLedgerRepository {
    pub fn reverse_with_reason(
        &self,
        id: TransactionId,
        at: Timestamp,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        if reason.trim().is_empty() || reason.chars().count() > 200 {
            return Err(RepositoryError::InvalidData(
                "invalid reversal reason".into(),
            ));
        }
        self.reverse_named(id, TransactionId::new(), at, reason)
    }
    pub(super) fn reverse_named(
        &self,
        id: TransactionId,
        new_id: TransactionId,
        at: Timestamp,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        if let Some(existing) = tx
            .query_row(
                "SELECT transaction_id FROM transaction_metadata WHERE reversal_of=?1",
                [id.as_uuid().to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?
        {
            return Uuid::parse_str(&existing)
                .map(TransactionId::from_uuid)
                .map_err(|_| RepositoryError::InvalidData("invalid ID".into()));
        }
        let original = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if original.status != TransactionStatus::Confirmed
            || self.metadata(id)?.reversal_of.is_some()
        {
            return Err(RepositoryError::Conflict);
        }
        if self.has_active_refunds(id)? {
            return Err(RepositoryError::InvalidData(
                "reverse related refunds first".into(),
            ));
        }
        let effect =
            ledger_domain::report_effect(&original).unwrap_or(ledger_domain::ReportEffect {
                income_cny_minor: 0,
                expense_cny_minor: 0,
            });
        let mut reverse = original.clone();
        reverse.id = new_id;
        reverse.occurred_at = at;
        reverse.kind = TransactionKind::BalanceAdjustment;
        reverse.original_transaction_id = Some(id);
        reverse.report_amount_cny = None;
        for p in &mut reverse.postings {
            p.delta = Money::new(
                p.delta
                    .minor_units()
                    .checked_neg()
                    .ok_or_else(|| RepositoryError::InvalidData("reversal overflow".into()))?,
                p.delta.currency().clone(),
            );
        }
        self.validate_feature_transaction(&reverse)?;
        Self::write_transaction(&tx, &reverse, true)?;
        tx.execute("UPDATE transaction_metadata SET reversal_of=?2,report_income_minor=?3,report_expense_minor=?4 WHERE transaction_id=?1",params![new_id.as_uuid().to_string(),id.as_uuid().to_string(),effect.income_cny_minor.checked_neg().ok_or(RepositoryError::Conflict)?,effect.expense_cny_minor.checked_neg().ok_or(RepositoryError::Conflict)?]).map_err(storage_error)?;
        tx.execute(
            "UPDATE transactions SET description=?2 WHERE id=?1",
            params![new_id.as_uuid().to_string(), reason],
        )
        .map_err(storage_error)?;
        tx.execute(
            "UPDATE audit_events SET action='reversed' WHERE transaction_id=?1",
            [new_id.as_uuid().to_string()],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,'reversed',?3)", params![Uuid::new_v4().to_string(), at.timestamp_micros(), id.as_uuid().to_string()]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(new_id)
    }
}

impl SqliteLedgerRepository {
    pub fn audit_history(&self, id: TransactionId) -> Result<Vec<(String, i64)>, RepositoryError> {
        self.connection.prepare("SELECT action,occurred_at_utc_micros FROM audit_events WHERE transaction_id=?1 ORDER BY occurred_at_utc_micros,id").map_err(storage_error)?
          .query_map([id.as_uuid().to_string()], |r| Ok((r.get(0)?,r.get(1)?))).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)
    }
    pub fn related_transactions(
        &self,
        id: TransactionId,
    ) -> Result<Vec<(TransactionId, bool)>, RepositoryError> {
        let id_text = id.as_uuid().to_string();
        let mut statement = self.connection.prepare(
            "SELECT t.id, m.reversal_of FROM transactions t LEFT JOIN transaction_metadata m ON m.transaction_id=t.id WHERE t.original_transaction_id=?1 ORDER BY t.occurred_at_utc_micros,t.id"
        ).map_err(storage_error)?;
        statement
            .query_map([&id_text], |row| {
                let raw_id: String = row.get(0)?;
                let reversal_of: Option<String> = row.get(1)?;
                let parsed = Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok((
                    TransactionId::from_uuid(parsed),
                    reversal_of.as_deref() == Some(id_text.as_str()),
                ))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }
    pub(crate) fn has_active_refunds(&self, id: TransactionId) -> Result<bool, RepositoryError> {
        self.connection.query_row("SELECT EXISTS(SELECT 1 FROM transactions t WHERE original_transaction_id=?1 AND kind='refund' AND status='confirmed' AND NOT EXISTS(SELECT 1 FROM transaction_metadata m WHERE m.reversal_of=t.id))",[id.as_uuid().to_string()],|r|r.get(0)).map_err(storage_error)
    }
}
