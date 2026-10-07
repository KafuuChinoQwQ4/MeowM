//! Stable source-scoped IDs and conservative fuzzy matching.
use crate::ImportSource;
use ledger_app::ImportRecord;
use ledger_domain::{SourceKind, Transaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateMatch {
    ExactExternalId,
    StrongCandidate,
    NoMatch,
}

pub fn match_transaction(
    source: ImportSource,
    external_id: Option<&str>,
    transaction: &Transaction,
    account_id: ledger_domain::AccountId,
    existing: &[ImportRecord],
) -> DuplicateMatch {
    let source_kind = source_kind(source);
    if let Some(id) = external_id
        && existing.iter().any(|item| {
            item.evidence.source_kind == source_kind
                && item.evidence.external_transaction_id.as_deref() == Some(id)
        })
    {
        return DuplicateMatch::ExactExternalId;
    }

    let candidate = existing.iter().any(|item| {
        let old = &item.transaction;
        let merchants_match = match (old.merchant.as_deref(), transaction.merchant.as_deref()) {
            (Some(left), Some(right)) => normalize_merchant(left) == normalize_merchant(right),
            _ => false,
        };
        old.kind == transaction.kind
            && old.report_amount_cny == transaction.report_amount_cny
            && old.report_amount_cny.is_some()
            && ((old.postings.len() == 1
                && transaction.postings.len() == 1
                && old.postings[0].account_id == account_id
                && transaction.postings[0].account_id == account_id
                && old.postings[0].delta == transaction.postings[0].delta)
                || (item.evidence.source_kind == SourceKind::Notification
                    && old.status == ledger_domain::TransactionStatus::Pending
                    && old.postings.is_empty()))
            && merchants_match
            && (old.occurred_at - transaction.occurred_at).abs() <= chrono::Duration::minutes(5)
    });
    if candidate {
        DuplicateMatch::StrongCandidate
    } else {
        DuplicateMatch::NoMatch
    }
}

pub(crate) fn source_kind(source: ImportSource) -> SourceKind {
    match source {
        ImportSource::WeChat => SourceKind::WeChat,
        ImportSource::Alipay => SourceKind::Alipay,
        ImportSource::BankCsv => SourceKind::BankCsv,
    }
}

fn normalize_merchant(value: &str) -> String {
    value.trim().to_lowercase()
}
