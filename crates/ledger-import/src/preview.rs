//! Import preview and per-row duplicate decisions.

use crate::{
    DuplicateMatch, ExistingImport, NormalizedImportRow, dedupe::classify_duplicate_for_account,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewDecision {
    New,
    ExactDuplicate {
        transaction_id: Option<ledger_domain::TransactionId>,
    },
    SuspectedDuplicate {
        transaction_id: Option<ledger_domain::TransactionId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    pub row: NormalizedImportRow,
    pub decision: PreviewDecision,
    pub warnings: Vec<crate::ImportWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreviewCounts {
    pub new: usize,
    pub exact_duplicates: usize,
    pub suspected_duplicates: usize,
    pub warnings: usize,
    pub pending: usize,
}

impl PreviewCounts {
    pub fn new_count(&self) -> usize {
        self.new
    }
    pub fn exact_duplicate_count(&self) -> usize {
        self.exact_duplicates
    }
    pub fn suspected_duplicate_count(&self) -> usize {
        self.suspected_duplicates
    }
    pub fn warning_count(&self) -> usize {
        self.warnings
    }
    pub fn pending_count(&self) -> usize {
        self.pending
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    pub rows: Vec<PreviewRow>,
    pub counts: PreviewCounts,
}

impl ImportPreview {
    pub fn build(rows: &[NormalizedImportRow], existing: &[ExistingImport]) -> Self {
        Self::build_for_account(rows, existing, None)
    }
    pub fn build_for_account(
        rows: &[NormalizedImportRow],
        existing: &[ExistingImport],
        account: Option<ledger_domain::AccountId>,
    ) -> Self {
        let mut counts = PreviewCounts::default();
        let mut seen = existing.to_vec();
        let mut claimed_idless = std::collections::HashSet::new();
        let rows = rows
            .iter()
            .cloned()
            .map(|row| {
                let decision =
                    match classify_duplicate_for_account(&row, &seen, account, &claimed_idless) {
                        DuplicateMatch::New => {
                            counts.new += 1;
                            counts.pending += 1;
                            PreviewDecision::New
                        }
                        DuplicateMatch::ExactDuplicate { transaction_id } => {
                            if row.external_transaction_id.is_none()
                                && let Some(id) = transaction_id
                            {
                                claimed_idless.insert(id);
                            }
                            counts.exact_duplicates += 1;
                            PreviewDecision::ExactDuplicate { transaction_id }
                        }
                        DuplicateMatch::SuspectedDuplicate { transaction_id } => {
                            counts.suspected_duplicates += 1;
                            counts.pending += 1;
                            PreviewDecision::SuspectedDuplicate { transaction_id }
                        }
                    };
                seen.push(ExistingImport {
                    transaction_id: None,
                    source_kind: row.source_kind,
                    external_transaction_id: row.external_transaction_id.clone(),
                    merchant_order_id: row.merchant_order_id.clone(),
                    source_fields_digest: Some(row.source_fields_digest),
                    occurred_at: row.occurred_at,
                    kind: row.kind,
                    amount_minor: row.amount_minor,
                    merchant: row.merchant.clone(),
                    description: row.description.clone(),
                    date_only: false,
                    account_ids: account.into_iter().collect(),
                });
                counts.warnings += row.warnings.len();
                PreviewRow {
                    warnings: row.warnings.clone(),
                    row,
                    decision,
                }
            })
            .collect();
        Self { rows, counts }
    }

    pub fn new_count(&self) -> usize {
        self.counts.new
    }
    pub fn exact_duplicate_count(&self) -> usize {
        self.counts.exact_duplicates
    }
    pub fn suspected_duplicate_count(&self) -> usize {
        self.counts.suspected_duplicates
    }
    pub fn warning_count(&self) -> usize {
        self.counts.warnings
    }
    pub fn pending_count(&self) -> usize {
        self.counts.pending
    }
}
