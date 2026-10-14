//! Compatible feature contracts; transaction constructors remain unchanged.
use crate::{AccountId, CategoryId, ReportEffect, Transaction, TransactionId, TransactionStatus};
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;
macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}
id!(BudgetId);
id!(RecurringTemplateId);
id!(ReconciliationId);
id!(AuditEventId);
#[derive(Debug, Clone, Default)]
pub struct PendingQuery {
    pub status: Option<TransactionStatus>,
    pub from_utc_inclusive: Option<DateTime<Utc>>,
    pub until_utc_exclusive: Option<DateTime<Utc>>,
    pub account_id: Option<AccountId>,
    pub category_id: Option<CategoryId>,
    pub merchant: Option<String>,
    pub keyword: Option<String>,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    pub transaction_id: TransactionId,
    pub updated_at: DateTime<Utc>,
    pub edit_version: u32,
    pub reversal_of: Option<TransactionId>,
    pub report_override: Option<ReportEffect>,
}
#[derive(Debug, Clone)]
pub struct PendingCandidate {
    pub transaction: Transaction,
    pub metadata: TransactionMetadata,
}
#[derive(Debug, Clone)]
pub struct TransactionPage {
    pub items: Vec<PendingCandidate>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Debug, Clone)]
pub struct BulkConfirmation {
    pub transaction_ids: Vec<TransactionId>,
}
#[derive(Debug, Clone)]
pub struct TransactionEdit {
    pub transaction: Transaction,
    pub expected_version: u32,
}
#[derive(Debug, Clone)]
pub struct Budget {
    pub id: BudgetId,
    pub month: NaiveDate,
    pub category_id: Option<CategoryId>,
    pub amount_cny_minor: i64,
}
impl Budget {
    pub fn validate(&self) -> bool {
        self.amount_cny_minor >= 0
    }
}
#[derive(Debug, Clone)]
pub struct RecurringTemplate {
    pub id: RecurringTemplateId,
    pub transaction: Transaction,
    pub next_date: NaiveDate,
    pub interval_months: u32,
    pub enabled: bool,
}
#[derive(Debug, Clone)]
pub struct ReconciliationSession {
    pub id: ReconciliationId,
    pub account_id: AccountId,
    pub cutoff_utc: DateTime<Utc>,
    pub expected_minor: i64,
    pub actual_minor: i64,
    pub adjustment_id: Option<TransactionId>,
}
impl ReconciliationSession {
    pub fn difference_minor(&self) -> Option<i64> {
        self.actual_minor.checked_sub(self.expected_minor)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrivacySettings {
    pub hide_amounts: bool,
    pub lock_enabled: bool,
    pub password_verifier: Option<String>,
}
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub occurred_at: DateTime<Utc>,
    pub action: String,
    pub transaction_id: Option<TransactionId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_rejects_negative_and_reconciliation_is_integer_checked() {
        let b = Budget {
            id: BudgetId::new(),
            month: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            category_id: None,
            amount_cny_minor: -1,
        };
        assert!(!b.validate());
        let r = ReconciliationSession {
            id: ReconciliationId::new(),
            account_id: AccountId::new(),
            cutoff_utc: DateTime::from_timestamp(0, 0).unwrap(),
            expected_minor: 100,
            actual_minor: 99,
            adjustment_id: None,
        };
        assert_eq!(r.difference_minor(), Some(-1));
        assert_eq!(
            ReconciliationSession {
                actual_minor: i64::MAX,
                expected_minor: -1,
                ..r
            }
            .difference_minor(),
            None
        );
    }
}
