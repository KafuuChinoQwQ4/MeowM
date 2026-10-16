use super::*;
impl SqliteLedgerRepository {
    pub(super) fn create_reconciliation(
        &self,
        account_id: AccountId,
        cutoff: Timestamp,
        actual: Money,
    ) -> Result<ReconciliationId, RepositoryError> {
        let tx = immediate(self)?;
        let account = self
            .get_account(account_id)?
            .ok_or(RepositoryError::NotFound)?;
        if actual.currency() != &account.currency {
            return Err(invalid("实际余额币种与平台不一致"));
        }
        let reference = account
            .opening_balance
            .ok_or_else(|| invalid("请先为该平台设置参考余额，再开始对账"))?;
        let reference_at = account
            .opening_balance_effective_at
            .ok_or_else(|| invalid("请先设置参考余额日期"))?;
        let expected = self
            .balance_as_of(account_id, cutoff)?
            .ok_or_else(|| invalid("请先设置参考余额"))?;
        actual
            .checked_sub(expected.clone())
            .map_err(|e| invalid(&e.to_string()))?;
        let period_from: Option<i64> = tx.query_row("SELECT MAX(cutoff_utc_micros) FROM reconciliation_sessions WHERE account_id=?1 AND status='closed' AND snapshot_ready=1 AND cutoff_utc_micros<?2", params![account_id.as_uuid().to_string(),cutoff.timestamp_micros()], |r| r.get(0)).map_err(storage_error)?;
        let id = ReconciliationId::new();
        tx.execute("INSERT INTO reconciliation_sessions(id,account_id,cutoff_utc_micros,expected_minor,actual_minor,status,created_at_utc_micros,reference_minor,reference_at_utc_micros,currency_code,currency_exponent,period_from_utc_micros,snapshot_ready) VALUES(?1,?2,?3,?4,?5,'open',?6,?7,?8,?9,?10,?11,1)", params![id.as_uuid().to_string(),account_id.as_uuid().to_string(),cutoff.timestamp_micros(),expected.minor_units(),actual.minor_units(),Utc::now().timestamp_micros(),reference.minor_units(),reference_at.timestamp_micros(),account.currency.code(),account.currency.minor_unit_exponent(),period_from]).map_err(storage_error)?;
        // Account-local rows, including non-effective candidates, are frozen as they were seen.
        let ids = tx.prepare("SELECT DISTINCT t.id FROM transactions t JOIN postings p ON p.transaction_id=t.id WHERE p.account_id=?1 AND t.occurred_at_utc_micros<=?2 AND (?3 IS NULL OR t.occurred_at_utc_micros>?3) ORDER BY t.occurred_at_utc_micros,t.id").map_err(storage_error)?.query_map(params![account_id.as_uuid().to_string(),cutoff.timestamp_micros(),period_from], |r| r.get::<_,String>(0)).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
        for raw_id in ids {
            let t = self
                .get_transaction(TransactionId::from_uuid(
                    parse_uuid(&raw_id).map_err(RepositoryError::InvalidData)?,
                ))?
                .ok_or(RepositoryError::NotFound)?;
            let delta = t
                .postings
                .iter()
                .filter(|p| p.account_id == account_id)
                .try_fold(Money::new(0, account.currency.clone()), |sum, p| {
                    sum.checked_add(p.delta.clone())
                })
                .map_err(|e| invalid(&e.to_string()))?;
            tx.execute("INSERT INTO reconciliation_entries(session_id,transaction_id,occurred_at_utc_micros,kind,status,delta_minor,merchant,description) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![id.as_uuid().to_string(),raw_id,t.occurred_at.timestamp_micros(),transaction_kind_str(t.kind),transaction_status_str(t.status),delta.minor_units(),t.merchant,t.description]).map_err(storage_error)?;
        }
        event(&tx, id, "started", None)?;
        tx.commit().map_err(storage_error)?;
        Ok(id)
    }
}
