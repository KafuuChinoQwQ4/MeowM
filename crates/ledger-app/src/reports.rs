//! Local-calendar income, expense, and account-balance report queries.

use std::collections::BTreeMap;

use ledger_domain::{CategoryId, report_effect};

use crate::{AppError, LedgerRepository, Timestamp};

/// One local calendar month represented as a half-open UTC range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthBoundary {
    pub label: String,
    pub from_utc_inclusive: Timestamp,
    pub until_utc_exclusive: Timestamp,
}

impl MonthBoundary {
    pub fn new(
        label: impl Into<String>,
        from_utc_inclusive: Timestamp,
        until_utc_exclusive: Timestamp,
    ) -> Self {
        Self {
            label: label.into(),
            from_utc_inclusive,
            until_utc_exclusive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportQuery {
    pub from_utc_inclusive: Timestamp,
    pub until_utc_exclusive: Timestamp,
    pub month_boundaries_utc: Vec<MonthBoundary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportSummary {
    pub income_cny_minor: i64,
    pub expense_cny_minor: i64,
    pub net_cny_minor: i64,
    pub expense_by_category: BTreeMap<CategoryId, i64>,
    pub income_by_month: BTreeMap<String, i64>,
    pub expense_by_month: BTreeMap<String, i64>,
}

pub struct ReportService<R> {
    repository: R,
}

impl<R: LedgerRepository> ReportService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn summarize(&self, query: ReportQuery) -> Result<ReportSummary, AppError> {
        let mut summary = ReportSummary::default();
        for transaction in self.repository.list_transactions()? {
            if transaction.occurred_at < query.from_utc_inclusive
                || transaction.occurred_at >= query.until_utc_exclusive
            {
                continue;
            }
            let Some(effect) = report_effect(&transaction) else {
                continue;
            };
            summary.income_cny_minor = summary
                .income_cny_minor
                .checked_add(effect.income_cny_minor)
                .ok_or_else(|| overflow_error("income report"))?;
            summary.expense_cny_minor = summary
                .expense_cny_minor
                .checked_add(effect.expense_cny_minor)
                .ok_or_else(|| overflow_error("expense report"))?;

            if effect.expense_cny_minor != 0
                && let Some(category_id) = transaction.category_id
            {
                let value = summary.expense_by_category.entry(category_id).or_default();
                *value = value
                    .checked_add(effect.expense_cny_minor)
                    .ok_or_else(|| overflow_error("category report"))?;
            }

            for boundary in &query.month_boundaries_utc {
                if transaction.occurred_at >= boundary.from_utc_inclusive
                    && transaction.occurred_at < boundary.until_utc_exclusive
                {
                    if effect.income_cny_minor != 0 {
                        let value = summary
                            .income_by_month
                            .entry(boundary.label.clone())
                            .or_default();
                        *value = value
                            .checked_add(effect.income_cny_minor)
                            .ok_or_else(|| overflow_error("monthly income report"))?;
                    }
                    if effect.expense_cny_minor != 0 {
                        let value = summary
                            .expense_by_month
                            .entry(boundary.label.clone())
                            .or_default();
                        *value = value
                            .checked_add(effect.expense_cny_minor)
                            .ok_or_else(|| overflow_error("monthly expense report"))?;
                    }
                    // Boundaries are supplied by the UI as non-overlapping month
                    // ranges. Stop after the first match if malformed input overlaps.
                    break;
                }
            }
        }
        summary.net_cny_minor = summary
            .income_cny_minor
            .checked_sub(summary.expense_cny_minor)
            .ok_or_else(|| overflow_error("net report"))?;
        Ok(summary)
    }
}

fn overflow_error(area: &str) -> AppError {
    AppError::Repository(crate::RepositoryError::InvalidData(format!(
        "{area} amount overflowed"
    )))
}
