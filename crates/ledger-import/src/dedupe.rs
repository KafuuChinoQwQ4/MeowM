//! Privacy-preserving duplicate classification.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Local, Utc};
use ledger_domain::{SourceKind, TransactionKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingImport {
    pub transaction_id: Option<ledger_domain::TransactionId>,
    pub source_kind: SourceKind,
    pub external_transaction_id: Option<String>,
    pub merchant_order_id: Option<String>,
    pub source_fields_digest: Option<[u8; 32]>,
    pub occurred_at: DateTime<Utc>,
    pub kind: TransactionKind,
    pub amount_minor: i64,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub date_only: bool,
    pub account_ids: Vec<ledger_domain::AccountId>,
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
    classify_duplicate_for_account(row, existing, None, &HashSet::new())
}

pub(crate) fn classify_duplicate_for_account(
    row: &crate::NormalizedImportRow,
    existing: &[ExistingImport],
    account: Option<ledger_domain::AccountId>,
    claimed_idless: &HashSet<ledger_domain::TransactionId>,
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
    let eligible = |candidate: &&ExistingImport| {
        !conflicts_with_id(candidate)
            && candidate
                .transaction_id
                .is_none_or(|id| !conflicting_transactions.contains(&id))
            && (row.kind == TransactionKind::Transfer
                || account.is_none_or(|id| candidate.account_ids.contains(&id)))
    };
    // A statement without IDs can still be recognized on an unchanged re-import.
    if let Some(candidate) = existing.iter().filter(eligible).find(|candidate| {
        candidate.source_kind == row.source_kind
            && candidate.external_transaction_id.is_none()
            && row.external_transaction_id.is_none()
            && candidate.source_fields_digest == Some(row.source_fields_digest)
            && candidate
                .transaction_id
                .is_some_and(|id| !claimed_idless.contains(&id))
    }) {
        return DuplicateMatch::ExactDuplicate {
            transaction_id: candidate.transaction_id,
        };
    }
    let candidate = existing.iter().filter(eligible).find(|candidate| {
        let merchant_order_matches = row.merchant_order_id.as_deref().is_some_and(|id| {
            !id.trim().is_empty()
                && candidate.source_kind == row.source_kind
                && candidate.merchant_order_id.as_deref() == Some(id)
        });
        candidate.occurred_at.with_timezone(&Local).date_naive()
            == row.occurred_at.with_timezone(&Local).date_naive()
            && (candidate.date_only
                || merchant_order_matches
                || (candidate.occurred_at - row.occurred_at).abs() <= Duration::minutes(5))
            && candidate.kind == row.kind
            && candidate.amount_minor == row.amount_minor
            && (merchant_order_matches
                || merchant_matches(
                    row.merchant.as_deref(),
                    row.description.as_deref(),
                    candidate.merchant.as_deref(),
                    candidate.description.as_deref(),
                ))
    });
    candidate.map_or(DuplicateMatch::New, |candidate| {
        DuplicateMatch::SuspectedDuplicate {
            transaction_id: candidate.transaction_id,
        }
    })
}

/// Product text may contain a shop name while the provider merchant is an aggregator.
/// Only a complete prefix followed by a separator is accepted, never an arbitrary substring.
pub fn merchant_matches(
    a: Option<&str>,
    a_note: Option<&str>,
    b: Option<&str>,
    b_note: Option<&str>,
) -> bool {
    let normalize = ledger_app::classification::normalize_merchant;
    let a = a.map(normalize).filter(|s| !s.is_empty());
    let b = b.map(normalize).filter(|s| !s.is_empty());
    let prefix = |name: &str, text: Option<&str>| {
        text.map(normalize)
            .and_then(|text| text.strip_prefix(name).map(str::to_owned))
            .is_some_and(|suffix| {
                suffix.is_empty() || suffix.starts_with(['-', '－', '—', '·', ':', '：', ' '])
            })
    };
    a.as_ref().zip(b.as_ref()).is_some_and(|(a, b)| a == b)
        || a.as_deref().is_some_and(|a| prefix(a, b_note))
        || b.as_deref().is_some_and(|b| prefix(b, a_note))
        || (a.is_none()
            && b.is_none()
            && a_note
                .map(normalize)
                .filter(|s| !s.is_empty())
                .is_some_and(|a| b_note.map(normalize).as_deref() == Some(&a)))
}
