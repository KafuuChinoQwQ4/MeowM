//! Privacy-preserving duplicate classification.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Local, Utc};
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
    let conflicts_with_id = |candidate: &ExistingImport| {
        candidate.source_kind == row.source_kind
            && matches!(
                (row.external_transaction_id.as_deref(), candidate.external_transaction_id.as_deref()),
                (Some(incoming), Some(saved)) if incoming != saved
            )
    };
    // A transaction can have several evidence rows, including an ID-less fallback.
    // A known conflicting platform ID must exclude all of them.
    let conflicting_transactions: HashSet<_> = existing
        .iter()
        .filter(|candidate| conflicts_with_id(candidate))
        .filter_map(|candidate| candidate.transaction_id)
        .collect();
    let candidate = existing.iter().find(|candidate| {
        !conflicts_with_id(candidate)
            && candidate
                .transaction_id
                .is_none_or(|id| !conflicting_transactions.contains(&id))
            && candidate.occurred_at.with_timezone(&Local).date_naive()
                == row.occurred_at.with_timezone(&Local).date_naive()
            && (candidate.occurred_at - row.occurred_at).abs() <= Duration::minutes(5)
            && candidate.kind == row.kind
            && candidate.amount_minor == row.amount_minor
            && row.merchant.as_deref().is_some_and(|merchant| {
                !merchant.trim().is_empty() && candidate.merchant.as_deref() == Some(merchant)
            })
    });
    candidate.map_or(DuplicateMatch::New, |candidate| {
        DuplicateMatch::SuspectedDuplicate {
            transaction_id: candidate.transaction_id,
        }
    })
}
