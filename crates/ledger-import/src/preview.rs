//! Import preview and per-row duplicate decisions.

use crate::{DuplicateMatch, ExistingImport, NormalizedImportRow, classify_duplicate};

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
        let mut counts = PreviewCounts::default();
        let mut seen = existing.to_vec();
        let rows = rows
            .iter()
            .cloned()
            .map(|row| {
                let decision = match classify_duplicate(&row, &seen) {
                    DuplicateMatch::New => {
                        counts.new += 1;
                        counts.pending += 1;
                        PreviewDecision::New
                    }
                    DuplicateMatch::ExactDuplicate { transaction_id } => {
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
                    occurred_at: row.occurred_at,
                    kind: row.kind,
                    amount_minor: row.amount_minor,
                    merchant: row.merchant.clone(),
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
