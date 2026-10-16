//! Snapshot reads and atomic reconciliation commands, separated from general ledger editing.
mod actions;
mod mapping;
mod snapshot;
use super::*;
use ledger_app::reconciliation::*;
use ledger_domain::{ReconciliationId, ReconciliationSession};
use mapping::*;
use rusqlite::TransactionBehavior;

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidData(message.into())
}
fn immediate(repo: &SqliteLedgerRepository) -> Result<SqlTransaction<'_>, RepositoryError> {
    SqlTransaction::new_unchecked(&repo.connection, TransactionBehavior::Immediate)
        .map_err(storage_error)
}
fn event(
    tx: &SqlTransaction<'_>,
    id: ReconciliationId,
    action: &str,
    reason: Option<&str>,
) -> Result<(), RepositoryError> {
    tx.execute("INSERT INTO reconciliation_events(session_id,occurred_at_utc_micros,action,reason) VALUES(?1,?2,?3,?4)", params![id.as_uuid().to_string(), Utc::now().timestamp_micros(), action, reason]).map_err(storage_error)?;
    Ok(())
}
fn editable(s: &ReconciliationSummary, version: u32) -> Result<(), RepositoryError> {
    if !s.record.snapshot_ready {
        return Err(invalid("旧会话缺少完整快照，请新建对账"));
    }
    if s.record.status == ReconciliationStatus::Closed {
        return Err(invalid("对账已关闭，请先重新打开"));
    }
    if s.record.version != version {
        return Err(RepositoryError::Conflict);
    }
    Ok(())
}
impl SqliteLedgerRepository {
    fn reconciliation_summary(
        &self,
        id: ReconciliationId,
    ) -> Result<Option<ReconciliationSummary>, RepositoryError> {
        let record = self
            .connection
            .query_row(
                &format!("{RECORD_SELECT} WHERE id=?1"),
                [id.as_uuid().to_string()],
                record_from_row,
            )
            .optional()
            .map_err(storage_error)?;
        let Some(record) = record else {
            return Ok(None);
        };
        let entries = self.connection.prepare("SELECT transaction_id,occurred_at_utc_micros,kind,status,delta_minor,merchant,description,reviewed FROM reconciliation_entries WHERE session_id=?1 ORDER BY occurred_at_utc_micros,transaction_id").map_err(storage_error)?.query_map([id.as_uuid().to_string()], entry_from_row).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
        let events = self.connection.prepare("SELECT action,occurred_at_utc_micros,reason FROM reconciliation_events WHERE session_id=?1 ORDER BY id").map_err(storage_error)?.query_map([id.as_uuid().to_string()], |r| Ok(ReconciliationEvent { action: r.get(0)?, occurred_at: parse_timestamp(r.get(1)?).map_err(to_sql_conversion_error)?, reason: r.get(2)? })).map_err(storage_error)?.collect::<Result<Vec<_>,_>>().map_err(storage_error)?;
        Ok(Some(ReconciliationSummary {
            record,
            entries,
            events,
        }))
    }
    fn unchanged_balance(&self, s: &ReconciliationSummary) -> Result<(), RepositoryError> {
        let r = &s.record;
        let session = &r.session;
        let account = self
            .get_account(session.account_id)?
            .ok_or(RepositoryError::NotFound)?;
        if account.currency != *r.reference.currency()
            || account.opening_balance.as_ref() != Some(&r.reference)
            || account.opening_balance_effective_at != Some(r.reference_at)
        {
            return Err(invalid("参考余额已变化，请新建对账"));
        }
        let expected = if session.adjustment_id.is_some() {
            session.actual_minor
        } else {
            session.expected_minor
        };
        if self
            .balance_as_of(session.account_id, session.cutoff_utc)?
            .as_ref()
            .map(Money::minor_units)
            != Some(expected)
        {
            return Err(invalid(
                "截止时刻的账本余额已变化，请新建对账；旧快照保持不变",
            ));
        }
        Ok(())
    }
    fn required_reconciliation(
        &self,
        id: ReconciliationId,
    ) -> Result<ReconciliationSummary, RepositoryError> {
        self.reconciliation_summary(id)?
            .ok_or(RepositoryError::NotFound)
    }
}
impl ReconciliationRepository for SqliteLedgerRepository {
    fn start_reconciliation(
        &self,
        account: AccountId,
        cutoff: Timestamp,
        actual: Money,
    ) -> Result<ReconciliationId, RepositoryError> {
        self.create_reconciliation(account, cutoff, actual)
    }
    fn list_reconciliations(
        &self,
        account: AccountId,
    ) -> Result<Vec<ReconciliationRecord>, RepositoryError> {
        self.connection
            .prepare(&format!(
                "{RECORD_SELECT} WHERE account_id=?1 ORDER BY created_at_utc_micros DESC,id"
            ))
            .map_err(storage_error)?
            .query_map([account.as_uuid().to_string()], record_from_row)
            .map_err(storage_error)?
            .collect::<Result<_, _>>()
            .map_err(storage_error)
    }
    fn reconciliation(
        &self,
        id: ReconciliationId,
    ) -> Result<Option<ReconciliationSummary>, RepositoryError> {
        // A read transaction prevents mixing versions from concurrent SQLite connections.
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let summary = self.reconciliation_summary(id)?;
        tx.commit().map_err(storage_error)?;
        Ok(summary)
    }
    fn review_reconciliation(
        &self,
        id: ReconciliationId,
        entry: TransactionId,
        reviewed: bool,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.review_snapshot(id, entry, reviewed, version)
    }
    fn adjust_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        self.adjust_snapshot(id, version, reason)
    }
    fn close_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.change_reconciliation_state(id, version, false)
    }
    fn reopen_reconciliation(
        &self,
        id: ReconciliationId,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.change_reconciliation_state(id, version, true)
    }
}
