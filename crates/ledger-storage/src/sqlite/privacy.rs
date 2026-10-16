//! Consent-driven cleanup. Preview and delete use the same affected source definition.
use super::*;
use ledger_app::{ports::PrivacyRepository, privacy::SourceDeletePreview};

impl PrivacyRepository for SqliteLedgerRepository {
    fn preview_notification_delete(&self) -> Result<SourceDeletePreview, RepositoryError> {
        let (pending, confirmed, blocked): (i64, i64, i64) = self.connection.query_row(
            "SELECT
              COALESCE(SUM(CASE WHEN t.status!='confirmed' THEN 1 ELSE 0 END),0),
              COALESCE(SUM(CASE WHEN t.status='confirmed' THEN 1 ELSE 0 END),0),
              COALESCE(SUM(CASE WHEN t.status!='confirmed' AND (
                EXISTS(SELECT 1 FROM reconciliation_entries r WHERE r.transaction_id=t.id)
                OR EXISTS(SELECT 1 FROM transactions child WHERE child.original_transaction_id=t.id)
                OR EXISTS(SELECT 1 FROM transaction_metadata rev WHERE rev.reversal_of=t.id)
                OR EXISTS(SELECT 1 FROM recurring_instances i WHERE i.transaction_id=t.id)
                OR EXISTS(SELECT 1 FROM import_receipt_rows i WHERE i.transaction_id=t.id)
                OR EXISTS(SELECT 1 FROM source_evidence other WHERE other.transaction_id=t.id AND other.source_kind!='notification')
              ) THEN 1 ELSE 0 END),0)
             FROM transactions t WHERE EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind='notification')",
            [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).map_err(storage_error)?;
        let receipts: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM notification_receipts", [], |r| {
                r.get(0)
            })
            .map_err(storage_error)?;
        Ok(SourceDeletePreview {
            pending: pending as usize,
            confirmed: confirmed as usize,
            receipts: receipts as usize,
            blocked: blocked as usize,
        })
    }
    fn delete_notification_data(&self, expected: [u8; 32]) -> Result<(), RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        if self.snapshot_inner()?.fingerprint()? != expected {
            return Err(RepositoryError::Conflict);
        }
        let preview = self.preview_notification_delete()?;
        if preview.blocked > 0 {
            return Err(RepositoryError::InvalidData(
                "通知候选被对账或其他来源引用；请先处理关联记录，或选择删除全部数据".into(),
            ));
        }
        let ids: Vec<(String,String)> = tx.prepare("SELECT t.id,t.status FROM transactions t WHERE EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind='notification')").map_err(storage_error)?
            .query_map([],|r|Ok((r.get(0)?,r.get(1)?))).map_err(storage_error)?
            .collect::<rusqlite::Result<_>>().map_err(storage_error)?;
        tx.execute("UPDATE notification_settings SET enabled=0", [])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM notification_status", [])
            .map_err(storage_error)?;
        tx.execute("DELETE FROM notification_receipts", [])
            .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM source_evidence WHERE source_kind='notification'",
            [],
        )
        .map_err(storage_error)?;
        for (id, status) in ids {
            if status == "confirmed" {
                tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,'notification_source_deleted',?3)",
                    params![Uuid::new_v4().to_string(),Utc::now().timestamp_micros(),id]).map_err(storage_error)?;
            } else {
                tx.execute("UPDATE audit_events SET transaction_id=NULL, action='notification_candidate_deleted' WHERE transaction_id=?1", [&id])
                    .map_err(storage_error)?;
                tx.execute("DELETE FROM postings WHERE transaction_id=?1", [&id])
                    .map_err(storage_error)?;
                tx.execute(
                    "DELETE FROM transaction_metadata WHERE transaction_id=?1",
                    [&id],
                )
                .map_err(storage_error)?;
                tx.execute("DELETE FROM transactions WHERE id=?1", [&id])
                    .map_err(storage_error)?;
            }
        }
        tx.execute("DELETE FROM import_batches WHERE id NOT IN (SELECT import_batch_id FROM source_evidence) AND id NOT IN (SELECT import_batch_id FROM audit_events WHERE import_batch_id IS NOT NULL) AND id NOT IN (SELECT batch_id FROM import_receipt_rows)",[]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
    fn delete_all_data(&self, expected: [u8; 32]) -> Result<(), RepositoryError> {
        self.delete_all_with_defaults(expected)
    }
}
