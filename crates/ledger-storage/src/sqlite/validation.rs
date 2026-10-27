use super::*;

impl SqliteLedgerRepository {
    pub(super) fn validate_feature_transaction(
        &self,
        t: &Transaction,
    ) -> Result<(), RepositoryError> {
        ledger_domain::validate_transaction_shape(t, &self.list_accounts()?)
            .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
        if t.kind == TransactionKind::Refund {
            if let Some(parent_id) = t.original_transaction_id {
                let parent = self
                    .get_transaction(parent_id)?
                    .ok_or(RepositoryError::NotFound)?;
                let valid_status = if t.status == TransactionStatus::Pending {
                    matches!(
                        parent.status,
                        TransactionStatus::Pending | TransactionStatus::Confirmed
                    )
                } else {
                    parent.status == TransactionStatus::Confirmed
                };
                if parent.kind != TransactionKind::Expense || !valid_status {
                    return Err(RepositoryError::InvalidData("refund parent must be an expense; confirmation requires a confirmed parent".into()));
                }
            }
        }
        Ok(())
    }
    pub fn metadata(
        &self,
        id: TransactionId,
    ) -> Result<ledger_domain::TransactionMetadata, RepositoryError> {
        self.connection.query_row("SELECT updated_at_utc_micros,edit_version,reversal_of,report_income_minor,report_expense_minor FROM transaction_metadata WHERE transaction_id=?1",[id.as_uuid().to_string()],|r|{
            let at:i64=r.get(0)?; let reversal:Option<String>=r.get(2)?;let income:Option<i64>=r.get(3)?;
            Ok(ledger_domain::TransactionMetadata{transaction_id:id,updated_at:DateTime::from_timestamp_micros(at).ok_or(rusqlite::Error::InvalidQuery)?,edit_version:r.get(1)?,reversal_of:reversal.map(|v|Uuid::parse_str(&v).map(TransactionId::from_uuid)).transpose().map_err(|_|rusqlite::Error::InvalidQuery)?,report_override:income.map(|i|Ok::<_,rusqlite::Error>(ledger_domain::ReportEffect{income_cny_minor:i,expense_cny_minor:r.get(4)?})).transpose()?})
        }).map_err(storage_error)
    }
}
