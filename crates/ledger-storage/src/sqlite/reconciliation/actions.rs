use super::*;
impl SqliteLedgerRepository {
    pub(super) fn review_snapshot(
        &self,
        id: ReconciliationId,
        entry: TransactionId,
        reviewed: bool,
        version: u32,
    ) -> Result<(), RepositoryError> {
        let tx = immediate(self)?;
        let s = self.required_reconciliation(id)?;
        editable(&s, version)?;
        let old = s
            .entries
            .iter()
            .find(|e| e.transaction_id == entry)
            .ok_or(RepositoryError::NotFound)?;
        if old.reviewed == reviewed {
            return Ok(());
        }
        tx.execute("UPDATE reconciliation_entries SET reviewed=?3 WHERE session_id=?1 AND transaction_id=?2",params![id.as_uuid().to_string(),entry.as_uuid().to_string(),reviewed]).map_err(storage_error)?;
        let all = s.entries.iter().all(|e| {
            if e.transaction_id == entry {
                reviewed
            } else {
                e.reviewed
            }
        });
        tx.execute(
            "UPDATE reconciliation_sessions SET status=?2,version=version+1 WHERE id=?1",
            params![
                id.as_uuid().to_string(),
                if all { "reviewed" } else { "open" }
            ],
        )
        .map_err(storage_error)?;
        event(
            &tx,
            id,
            if reviewed {
                "entry_reviewed"
            } else {
                "entry_unreviewed"
            },
            Some(&entry.as_uuid().to_string()),
        )?;
        tx.commit().map_err(storage_error)
    }
    pub(super) fn adjust_snapshot(
        &self,
        id: ReconciliationId,
        version: u32,
        reason: &str,
    ) -> Result<TransactionId, RepositoryError> {
        let reason = reason.trim();
        if reason.is_empty() || reason.chars().count() > 200 {
            return Err(invalid("请填写 1 至 200 个字符的调整原因"));
        }
        let tx = immediate(self)?;
        let s = self.required_reconciliation(id)?;
        if s.record.status == ReconciliationStatus::Closed {
            return Err(invalid("对账已关闭，请先重新打开"));
        }
        if let Some(existing) = s.record.session.adjustment_id {
            // A retry of the same reason returns the receipt even if the caller lost the new version.
            if s.events
                .iter()
                .any(|e| e.action == "adjusted" && e.reason.as_deref() == Some(reason))
            {
                return Ok(existing);
            }
            return Err(invalid("该会话已生成调整，请新建对账处理新的差额"));
        }
        editable(&s, version)?;
        self.unchanged_balance(&s)?;
        if !s.all_reviewed() {
            return Err(invalid("请先逐项核对快照中的交易"));
        }
        let session = &s.record.session;
        if session.cutoff_utc <= s.record.reference_at {
            return Err(invalid("生成调整的截止日期必须晚于参考余额日期"));
        }
        let difference = session
            .difference_minor()
            .ok_or_else(|| invalid("差额超出金额范围"))?;
        if difference == 0 {
            return Err(invalid("余额一致，无需生成调整"));
        }
        let adjustment = Transaction {
            id: TransactionId::new(),
            occurred_at: session.cutoff_utc,
            kind: TransactionKind::BalanceAdjustment,
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: session.account_id,
                delta: Money::new(difference, s.record.reference.currency().clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: Some(format!("对账调整：{reason}")),
        };
        self.validate_feature_transaction(&adjustment)?;
        Self::write_transaction(&tx, &adjustment, true)?;
        tx.execute(
            "UPDATE reconciliation_sessions SET adjustment_id=?2,version=version+1 WHERE id=?1",
            params![
                id.as_uuid().to_string(),
                adjustment.id.as_uuid().to_string()
            ],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,?3,?4)",params![Uuid::new_v4().to_string(),Utc::now().timestamp_micros(),format!("reconciliation_adjustment:{}",id.as_uuid()),adjustment.id.as_uuid().to_string()]).map_err(storage_error)?;
        event(&tx, id, "adjusted", Some(reason))?;
        self.unchanged_balance(&self.required_reconciliation(id)?)?;
        tx.commit().map_err(storage_error)?;
        Ok(adjustment.id)
    }
    pub(super) fn change_reconciliation_state(
        &self,
        id: ReconciliationId,
        version: u32,
        reopen: bool,
    ) -> Result<(), RepositoryError> {
        let tx = immediate(self)?;
        let s = self.required_reconciliation(id)?;
        if !s.record.snapshot_ready {
            return Err(invalid("旧会话缺少完整快照，请新建对账"));
        }
        if !reopen && s.record.status == ReconciliationStatus::Closed {
            return Ok(());
        }
        if s.record.version != version {
            return Err(RepositoryError::Conflict);
        }
        if reopen {
            if s.record.status != ReconciliationStatus::Closed {
                return Err(invalid("只有已关闭对账可以重新打开"));
            }
        } else {
            if !s.all_reviewed() {
                return Err(invalid("请先逐项核对快照中的交易"));
            }
            if s.record.session.difference_minor() != Some(0)
                && s.record.session.adjustment_id.is_none()
            {
                return Err(invalid("仍有差额，请核实后明确确认调整"));
            }
            self.unchanged_balance(&s)?;
        }
        let status = if !reopen {
            "closed"
        } else if s.all_reviewed() {
            "reviewed"
        } else {
            "open"
        };
        tx.execute(
            "UPDATE reconciliation_sessions SET status=?2,version=version+1 WHERE id=?1",
            params![id.as_uuid().to_string(), status],
        )
        .map_err(storage_error)?;
        event(&tx, id, if reopen { "reopened" } else { "closed" }, None)?;
        tx.commit().map_err(storage_error)
    }
}
