use super::*;
use ledger_app::{
    BudgetRepository, ports::RecurringRepository, reconciliation::ReconciliationRepository,
};
impl SqliteLedgerRepository {
    pub(super) fn validate_snapshot(&self) -> Result<(), RepositoryError> {
        let violations: i64 = self
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .map_err(storage_error)?;
        let integrity: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(storage_error)?;
        if violations != 0 || integrity != "ok" {
            return Err(invalid());
        }
        let accounts = self.list_accounts()?;
        for a in &accounts {
            if a.display_name.trim().is_empty()
                || a.opening_balance.is_some() != a.opening_balance_effective_at.is_some()
            {
                return Err(invalid());
            }
        }
        for budget in self.list_budgets()? {
            budget.validate()?;
        }
        for template in self.list_recurring_templates()? {
            ledger_app::recurring::validate_template(&template)?;
        }
        self.list_categories()?;
        self.list_rules()?;
        for account in &accounts {
            for record in self.list_reconciliations(account.id)? {
                let session = &record.session;
                if record.snapshot_ready {
                    let delta = session
                        .actual_minor
                        .checked_sub(session.expected_minor)
                        .ok_or_else(invalid)?;
                    if record.reference.currency() != &account.currency {
                        return Err(invalid());
                    }
                    if let Some(id) = session.adjustment_id {
                        let t = self.get_transaction(id)?.ok_or_else(invalid)?;
                        if t.kind != TransactionKind::BalanceAdjustment
                            || t.status != TransactionStatus::Confirmed
                            || t.postings.len() != 1
                            || t.postings[0].account_id != account.id
                            || t.postings[0].delta.minor_units() != delta
                            || self.metadata(id)?.reversal_of.is_some()
                        {
                            return Err(invalid());
                        }
                    }
                }
            }
        }
        for mut t in self.list_transactions()? {
            // Ignored unresolved candidates are as incomplete as pending candidates.
            if t.status == TransactionStatus::Ignored {
                t.status = TransactionStatus::Pending;
            }
            self.validate_feature_transaction(&t)?;
            let metadata = self.metadata(t.id)?;
            if t.report_amount_cny.as_ref().is_some_and(|m| {
                m.currency() != &CurrencyCode::new("CNY", 2).expect("constant currency")
            }) {
                return Err(invalid());
            }
            if metadata.reversal_of.is_none() && metadata.report_override.is_some() {
                return Err(invalid());
            }
            if let Some(parent) = metadata.reversal_of {
                let original = self.get_transaction(parent)?.ok_or_else(invalid)?;
                let effect = ledger_domain::report_effect(&original).unwrap_or(
                    ledger_domain::ReportEffect {
                        income_cny_minor: 0,
                        expense_cny_minor: 0,
                    },
                );
                let expected = ledger_domain::ReportEffect {
                    income_cny_minor: effect.income_cny_minor.checked_neg().ok_or_else(invalid)?,
                    expense_cny_minor: effect
                        .expense_cny_minor
                        .checked_neg()
                        .ok_or_else(invalid)?,
                };
                if metadata.report_override != Some(expected)
                    || t.kind != TransactionKind::BalanceAdjustment
                    || t.status != TransactionStatus::Confirmed
                    || original.status != TransactionStatus::Confirmed
                    || t.original_transaction_id != Some(parent)
                    || t.postings.len() != original.postings.len()
                    || !t.postings.iter().zip(&original.postings).all(|(a, b)| {
                        a.account_id == b.account_id
                            && a.delta.currency() == b.delta.currency()
                            && a.delta.minor_units().checked_neg() == Some(b.delta.minor_units())
                    })
                {
                    return Err(invalid());
                }
            }
            ledger_app::FeatureRepository::source_evidence(self, t.id)?;
        }
        // Validate invariants not expressed by older SQLite CHECK constraints.
        let bad: bool = self.connection.query_row("SELECT
            EXISTS(SELECT 1 FROM transactions t LEFT JOIN transaction_metadata m ON m.transaction_id=t.id WHERE m.transaction_id IS NULL) OR
            EXISTS(SELECT 1 FROM postings WHERE ordinal<0) OR
            EXISTS(SELECT 1 FROM recurring_instances WHERE date(scheduled_date) IS NULL) OR
            EXISTS(SELECT 1 FROM recurring_templates WHERE date(next_date) IS NULL OR (anchor_date IS NOT NULL AND date(anchor_date) IS NULL) OR interval_months>120 OR (end_date IS NOT NULL AND (date(end_date) IS NULL OR end_date<anchor_date))) OR
            EXISTS(SELECT 1 FROM reconciliation_sessions WHERE snapshot_ready=1 AND status='closed' AND expected_minor<>actual_minor AND adjustment_id IS NULL) OR
            EXISTS(SELECT 1 FROM reconciliation_entries WHERE kind NOT IN ('income','expense','transfer','refund','balance_adjustment') OR status NOT IN ('confirmed','pending','ignored')) OR
            EXISTS(SELECT 1 FROM notification_settings WHERE enabled=1 AND account_id IS NULL)", [], |r| r.get(0)).map_err(storage_error)?;
        if bad {
            return Err(invalid());
        }
        Ok(())
    }
}
