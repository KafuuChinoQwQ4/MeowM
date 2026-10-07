//! Transactions, postings, transaction kinds, and status.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{AccountId, CategoryId, Money};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Income,
    Expense,
    Transfer,
    Refund,
    BalanceAdjustment,
}

impl TransactionKind {
    pub const fn requires_original_transaction(self) -> bool {
        matches!(self, Self::Refund)
    }

    pub const fn requires_two_distinct_postings(self) -> bool {
        matches!(self, Self::Transfer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Confirmed,
    Ignored,
}

impl TransactionStatus {
    pub const fn allows_empty_postings(self) -> bool {
        matches!(self, Self::Pending | Self::Ignored)
    }

    pub const fn is_effective(self) -> bool {
        matches!(self, Self::Confirmed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    pub account_id: AccountId,
    /// Signed change in the referenced account's currency.
    pub delta: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: TransactionId,
    pub occurred_at: DateTime<Utc>,
    pub kind: TransactionKind,
    pub status: TransactionStatus,
    pub postings: Vec<Posting>,
    pub category_id: Option<CategoryId>,
    pub original_transaction_id: Option<TransactionId>,
    pub report_amount_cny: Option<Money>,
    /// Optional normalized merchant/counterparty text used by classification rules.
    pub merchant: Option<String>,
    /// Optional normalized description or memo used by classification rules.
    pub description: Option<String>,
}
