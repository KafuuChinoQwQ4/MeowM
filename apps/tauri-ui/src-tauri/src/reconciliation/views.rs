use crate::ledger::kind_label;
use chrono::Local;
use ledger_app::reconciliation::*;
use ledger_domain::{Money, TransactionStatus};
use serde::Serialize;
#[derive(Serialize)]
pub struct RecordView {
    pub id: String,
    pub account_id: String,
    pub cutoff: String,
    pub date: String,
    pub expected: String,
    pub actual: String,
    pub difference: String,
    pub adjustment_id: Option<String>,
    pub status: &'static str,
    pub version: u32,
    pub created_at: String,
    pub reference: String,
    pub reference_at: String,
    pub period_from: Option<String>,
    pub snapshot_ready: bool,
}
impl From<ReconciliationRecord> for RecordView {
    fn from(r: ReconciliationRecord) -> Self {
        let s = r.session;
        let currency = r.reference.currency().clone();
        let fmt = |n| Money::new(n, currency.clone()).decimal_string();
        Self {
            id: s.id.as_uuid().to_string(),
            account_id: s.account_id.as_uuid().to_string(),
            cutoff: s.cutoff_utc.to_rfc3339(),
            date: s.cutoff_utc.with_timezone(&Local).date_naive().to_string(),
            expected: fmt(s.expected_minor),
            actual: fmt(s.actual_minor),
            difference: s
                .difference_minor()
                .map(fmt)
                .unwrap_or_else(|| "超出范围".into()),
            adjustment_id: s.adjustment_id.map(|id| id.as_uuid().to_string()),
            status: match r.status {
                ReconciliationStatus::Open => "open",
                ReconciliationStatus::Reviewed => "reviewed",
                ReconciliationStatus::Closed => "closed",
            },
            version: r.version,
            created_at: r.created_at.to_rfc3339(),
            reference: r.reference.decimal_string(),
            reference_at: r.reference_at.to_rfc3339(),
            period_from: r.period_from.map(|t| t.to_rfc3339()),
            snapshot_ready: r.snapshot_ready,
        }
    }
}
#[derive(Serialize)]
pub struct EntryView {
    pub id: String,
    pub occurred_at: String,
    pub kind: &'static str,
    pub status: &'static str,
    pub delta: String,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub reviewed: bool,
}
#[derive(Serialize)]
pub struct EventView {
    pub action: String,
    pub occurred_at: String,
    pub reason: Option<String>,
}
#[derive(Serialize)]
pub struct SummaryView {
    pub record: RecordView,
    pub entries: Vec<EntryView>,
    pub events: Vec<EventView>,
    pub pending_count: usize,
}
impl From<ReconciliationSummary> for SummaryView {
    fn from(s: ReconciliationSummary) -> Self {
        let pending_count = s.pending_count();
        let currency = s.record.reference.currency().clone();
        Self {
            record: s.record.into(),
            pending_count,
            entries: s
                .entries
                .into_iter()
                .map(|e| EntryView {
                    id: e.transaction_id.as_uuid().to_string(),
                    occurred_at: e.occurred_at.to_rfc3339(),
                    kind: kind_label(e.kind),
                    status: match e.status {
                        TransactionStatus::Pending => "pending",
                        TransactionStatus::Confirmed => "confirmed",
                        TransactionStatus::Ignored => "ignored",
                    },
                    delta: Money::new(e.delta_minor, currency.clone()).decimal_string(),
                    merchant: e.merchant,
                    description: e.description,
                    reviewed: e.reviewed,
                })
                .collect(),
            events: s
                .events
                .into_iter()
                .map(|e| EventView {
                    action: e.action,
                    occurred_at: e.occurred_at.to_rfc3339(),
                    reason: e.reason,
                })
                .collect(),
        }
    }
}
