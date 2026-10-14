//! Privacy-preserving duplicate classification.

use chrono::{DateTime, Local, Utc};
use ledger_domain::{SourceKind, TransactionKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImport {
    pub transaction_id: Option<ledger_domain::TransactionId>,
    pub source_kind: SourceKind,
    pub external_transaction_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub kind: TransactionKind,
    pub amount_minor: i64,
    pub merchant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateMatch {
    New,
    ExactDuplicate {
        transaction_id: Option<ledger_domain::TransactionId>,
    },
    SuspectedDuplicate {
        transaction_id: Option<ledger_domain::TransactionId>,
    },
}

pub fn classify_duplicate(
    row: &crate::NormalizedImportRow,
    existing: &[ExistingImport],
) -> DuplicateMatch {
    if let Some(id) = row.external_transaction_id.as_deref() {
        if let Some(match_row) = existing.iter().find(|candidate| {
            candidate.source_kind == row.source_kind
                && candidate.external_transaction_id.as_deref() == Some(id)
        }) {
            return DuplicateMatch::ExactDuplicate {
                transaction_id: match_row.transaction_id,
            };
        }
    }
    let candidate = existing.iter().find(|candidate| {
        candidate.source_kind == row.source_kind
            && candidate.occurred_at.with_timezone(&Local).date_naive()
                == row.occurred_at.with_timezone(&Local).date_naive()
            && candidate.kind == row.kind
            && candidate.amount_minor == row.amount_minor
            && (candidate.merchant.is_none()
                || row.merchant.is_none()
                || candidate.merchant == row.merchant)
    });
    candidate.map_or(DuplicateMatch::New, |candidate| {
        DuplicateMatch::SuspectedDuplicate {
            transaction_id: candidate.transaction_id,
        }
    })
}
