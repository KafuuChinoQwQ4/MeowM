//! Application use cases over the ledger domain and repository ports.

pub mod accounts;
pub mod budgets;
pub mod calendar;
pub mod classification;
pub mod notifications;
pub mod ports;
pub mod recurring;
pub mod reminders;
mod report_details;
pub mod reports;
pub mod transactions;

use std::fmt;

use ledger_domain::DomainError;

pub use accounts::AccountService;
pub use classification::{CategoryService, ClassificationRule, ClassificationService};
pub use ports::{
    ClassificationRuleRepository, FeatureRepository, ImportRepository, LedgerRepository,
};
pub use reports::{MonthBoundary, ReportQuery, ReportService, ReportSummary};
pub use transactions::{PendingService, TransactionService};

pub type Timestamp = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    NotFound,
    Conflict,
    InvalidData(String),
    Storage(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("record was not found"),
            Self::Conflict => f.write_str("record conflicts with an existing record"),
            Self::InvalidData(message) | Self::Storage(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RepositoryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Repository(RepositoryError),
    Domain(DomainError),
    AccountNotFound,
    TransactionNotFound,
    TransactionConflict,
    RefundParentInvalid,
    AccountCurrencyMismatch,
    TransactionNotPending,
    CategoryNotFound,
    CategoryDisabled,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => write!(f, "repository error: {error}"),
            Self::Domain(error) => write!(f, "domain error: {error}"),
            Self::AccountNotFound => f.write_str("account was not found"),
            Self::TransactionNotFound => f.write_str("transaction was not found"),
            Self::TransactionConflict => {
                f.write_str("transaction ID already has different content")
            }
            Self::RefundParentInvalid => {
                f.write_str("refund parent is missing or is not a confirmed expense")
            }
            Self::AccountCurrencyMismatch => {
                f.write_str("opening balance currency does not match account")
            }
            Self::TransactionNotPending => f.write_str("only pending transactions can be ignored"),
            Self::CategoryNotFound => f.write_str("category was not found"),
            Self::CategoryDisabled => f.write_str("disabled category cannot receive new rules"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<RepositoryError> for AppError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<DomainError> for AppError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

pub use budgets::{BudgetPlan, BudgetProgress, BudgetService};
pub use calendar::CalendarRange;
pub use ports::BudgetRepository;
pub use reminders::{ReminderEvent, ReminderService};
pub use report_details::{DetailedReport, ScopedReportQuery};
