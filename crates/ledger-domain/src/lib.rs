//! Platform-independent ledger types and invariants.

pub mod account;
pub mod category;
pub mod features;
pub mod money;
pub mod source;
pub mod transaction;
pub mod validation;

pub use account::{Account, AccountId, AccountKind};
pub use category::{Category, CategoryId, RuleId};
pub use features::*;
pub use money::{CurrencyCode, Money};
pub use source::{SourceEvidence, SourceKind};
pub use transaction::{Posting, Transaction, TransactionId, TransactionKind, TransactionStatus};
pub use validation::{ReportEffect, report_effect, validate_transaction_shape};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    InvalidCurrencyCode,
    InvalidCurrencyExponent,
    CurrencyMismatch,
    ArithmeticOverflow,
    MissingPosting,
    PostingAccountNotFound,
    InvalidTransferPostings,
    UnbalancedTransfer,
    InvalidPostingDirection,
    NegativeReportAmount,
    RefundOriginalRequired,
    SelfReferentialRefund,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrencyCode => {
                f.write_str("currency code must be three uppercase ASCII letters")
            }
            Self::InvalidCurrencyExponent => {
                f.write_str("currency exponent must be between 0 and 9")
            }
            Self::CurrencyMismatch => f.write_str("money currencies do not match"),
            Self::ArithmeticOverflow => f.write_str("money arithmetic overflowed"),
            Self::MissingPosting => f.write_str("confirmed transaction must have a posting"),
            Self::PostingAccountNotFound => f.write_str("posting references an unknown account"),
            Self::InvalidTransferPostings => {
                f.write_str("transfer must have postings for two distinct accounts")
            }
            Self::UnbalancedTransfer => {
                f.write_str("transfer postings must conserve the same currency amount")
            }
            Self::InvalidPostingDirection => {
                f.write_str("posting direction is inconsistent with its transaction and account")
            }
            Self::NegativeReportAmount => {
                f.write_str("report amount cannot be negative for income, expense, or refund")
            }
            Self::RefundOriginalRequired => {
                f.write_str("refund must reference its original transaction")
            }
            Self::SelfReferentialRefund => {
                f.write_str("refund cannot reference itself as its original transaction")
            }
        }
    }
}

impl std::error::Error for DomainError {}
