//! Local-calendar templates and immutable occurrence identities.
use crate::{AccountId, CategoryId, RecurringTemplateId, TransactionId, TransactionKind};
use chrono::NaiveDate;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringCadence {
    Weekly,
    Monthly,
    Yearly,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringTemplate {
    pub id: RecurringTemplateId,
    pub account_id: AccountId,
    pub kind: TransactionKind,
    pub amount_cny_minor: i64,
    pub category_id: Option<CategoryId>,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub cadence: RecurringCadence,
    /// Number of weeks/months/years between occurrences.
    pub interval: u32,
    /// Never replaced by a clamped February occurrence.
    pub anchor_date: NaiveDate,
    pub next_due_date: NaiveDate,
    /// Inclusive. A next date after this means the template has ended.
    pub end_date: Option<NaiveDate>,
    pub enabled: bool,
    pub version: u32,
}
impl RecurringTemplate {
    pub fn ended(&self) -> bool {
        self.end_date.is_some_and(|end| self.next_due_date > end)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurringInstanceStatus {
    Pending,
    Confirmed,
    Ignored,
    Skipped,
}
#[derive(Debug, Clone)]
pub struct RecurringInstance {
    pub template_id: RecurringTemplateId,
    pub due_date: NaiveDate,
    pub transaction_id: Option<TransactionId>,
    /// Derived from the linked transaction in the same database snapshot.
    pub status: RecurringInstanceStatus,
    pub amount_cny_minor: Option<i64>,
}
#[derive(Debug, Clone)]
pub struct RecurringReminder {
    pub template_id: RecurringTemplateId,
    pub due_date: NaiveDate,
    pub transaction_id: Option<TransactionId>,
    pub account_id: AccountId,
    pub amount_cny_minor: i64,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub overdue: bool,
}
