//! Accounts, account kinds, opening balances, and balance conventions.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountId(Uuid);

impl AccountId {
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

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Asset,
    Liability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub display_name: String,
    pub kind: AccountKind,
    pub currency: crate::CurrencyCode,
    pub opening_balance: Money,
    pub opening_balance_effective_at: DateTime<Utc>,
}

impl Account {
    /// Returns whether a posting uses this account's currency.
    pub fn currency_matches(&self, amount: &Money) -> bool {
        self.currency == *amount.currency()
    }
}
