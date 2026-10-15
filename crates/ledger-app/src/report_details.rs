//! Scoped report aggregation. No raw source evidence crosses this boundary.
use crate::{
    AppError, ClassificationRuleRepository, LedgerRepository, ReportQuery, ReportSummary,
    RepositoryError,
};
use chrono::Duration;
use ledger_domain::{
    AccountId, CategoryId, Transaction, TransactionKind, TransactionStatus, report_effect,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct ScopedReportQuery {
    pub range: ReportQuery,
    pub account_id: Option<AccountId>,
    pub category_id: Option<CategoryId>,
    pub merchant: Option<String>,
    pub merchant_offset: u32,
    pub merchant_limit: u32,
}
#[derive(Debug)]
pub struct CategoryTotal {
    pub id: Option<CategoryId>,
    pub name: String,
    pub expense_cny_minor: i64,
    pub share_percent: u32,
}
#[derive(Debug)]
pub struct MerchantTotal {
    pub name: String,
    pub income_cny_minor: i64,
    pub expense_cny_minor: i64,
}
#[derive(Debug)]
pub struct BalanceSnapshot {
    pub account_id: AccountId,
    pub name: String,
    pub amount_cny_minor: Option<i64>,
}
#[derive(Debug)]
pub struct DetailedReport {
    pub summary: ReportSummary,
    pub categories: Vec<CategoryTotal>,
    pub merchants: Vec<MerchantTotal>,
    pub merchant_total: usize,
    pub balances: Vec<BalanceSnapshot>,
    pub transaction_count: u64,
}
fn invalid(s: &str) -> AppError {
    RepositoryError::InvalidData(s.into()).into()
}
fn add(a: &mut i64, b: i64) -> Result<(), AppError> {
    *a = a
        .checked_add(b)
        .ok_or_else(|| invalid("报表金额超出范围"))?;
    Ok(())
}
pub(crate) fn effective_scope<R: LedgerRepository>(
    repo: &R,
    tx: &Transaction,
) -> Result<(Option<CategoryId>, String), AppError> {
    let mut category = tx.category_id;
    let mut merchant = tx.merchant.clone().filter(|s| !s.trim().is_empty());
    let mut current = tx.clone();
    // A reversal can point at a refund which points at the original expense.
    for _ in 0..3 {
        if !matches!(
            current.kind,
            TransactionKind::Refund | TransactionKind::BalanceAdjustment
        ) {
            break;
        }
        let Some(id) = current.original_transaction_id else {
            break;
        };
        let Some(parent) = repo.get_transaction(id)? else {
            break;
        };
        category = category.or(parent.category_id);
        if merchant.is_none() {
            merchant = parent.merchant.clone().filter(|s| !s.trim().is_empty());
        }
        current = parent;
    }
    let merchant = merchant
        .unwrap_or_else(|| "未知商户".into())
        .trim()
        .to_owned();
    Ok((category, merchant))
}
pub(crate) fn aggregate<R: LedgerRepository + ClassificationRuleRepository>(
    repo: &R,
    query: ScopedReportQuery,
) -> Result<DetailedReport, AppError> {
    let range = &query.range;
    if range.from_utc_inclusive >= range.until_utc_exclusive
        || !(1..=100).contains(&query.merchant_limit)
    {
        return Err(invalid("无效的报表范围或分页"));
    }
    let mut previous = None;
    for b in &range.month_boundaries_utc {
        if b.from_utc_inclusive >= b.until_utc_exclusive
            || previous.is_some_and(|end| b.from_utc_inclusive < end)
        {
            return Err(invalid("趋势区间不能重叠"));
        }
        previous = Some(b.until_utc_exclusive);
    }
    let mut summary = ReportSummary::default();
    for b in &range.month_boundaries_utc {
        summary.income_by_month.insert(b.label.clone(), 0);
        summary.expense_by_month.insert(b.label.clone(), 0);
    }
    let mut categories: BTreeMap<Option<CategoryId>, i64> = BTreeMap::new();
    let mut merchants: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut count = 0;
    for tx in repo.report_transactions(range.from_utc_inclusive, range.until_utc_exclusive)? {
        if tx.status != TransactionStatus::Confirmed
            || query
                .account_id
                .is_some_and(|id| !tx.postings.iter().any(|p| p.account_id == id))
        {
            continue;
        }
        let (category, merchant) = effective_scope(repo, &tx)?;
        if query.category_id.is_some_and(|id| category != Some(id))
            || query
                .merchant
                .as_ref()
                .is_some_and(|m| !merchant.to_lowercase().contains(&m.to_lowercase()))
        {
            continue;
        }
        let Some(effect) = repo.report_override(tx.id)?.or_else(|| report_effect(&tx)) else {
            continue;
        };
        add(&mut summary.income_cny_minor, effect.income_cny_minor)?;
        add(&mut summary.expense_cny_minor, effect.expense_cny_minor)?;
        if effect.income_cny_minor == 0 && effect.expense_cny_minor == 0 {
            continue;
        }
        count += 1;
        if effect.expense_cny_minor != 0 {
            add(
                categories.entry(category).or_default(),
                effect.expense_cny_minor,
            )?;
            if let Some(id) = category {
                add(
                    summary.expense_by_category.entry(id).or_default(),
                    effect.expense_cny_minor,
                )?;
            }
        }
        let m = merchants.entry(merchant).or_default();
        add(&mut m.0, effect.income_cny_minor)?;
        add(&mut m.1, effect.expense_cny_minor)?;
        for b in &range.month_boundaries_utc {
            if tx.occurred_at >= b.from_utc_inclusive && tx.occurred_at < b.until_utc_exclusive {
                add(
                    summary.income_by_month.entry(b.label.clone()).or_default(),
                    effect.income_cny_minor,
                )?;
                add(
                    summary.expense_by_month.entry(b.label.clone()).or_default(),
                    effect.expense_cny_minor,
                )?;
                break;
            }
        }
    }
    summary.net_cny_minor = summary
        .income_cny_minor
        .checked_sub(summary.expense_cny_minor)
        .ok_or_else(|| invalid("报表净额超出范围"))?;
    let names = repo.list_categories()?;
    let positive: i128 = categories.values().map(|v| i128::from((*v).max(0))).sum();
    let mut categories: Vec<_> = categories
        .into_iter()
        .map(|(id, amount)| CategoryTotal {
            id,
            name: id
                .and_then(|id| names.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                .unwrap_or_else(|| "未分类".into()),
            expense_cny_minor: amount,
            share_percent: if positive > 0 {
                ((i128::from(amount.max(0)) * 100) / positive) as u32
            } else {
                0
            },
        })
        .collect();
    categories.sort_by(|a, b| {
        b.expense_cny_minor
            .cmp(&a.expense_cny_minor)
            .then(a.name.cmp(&b.name))
    });
    let mut merchants: Vec<_> = merchants
        .into_iter()
        .map(|(name, (income, expense))| MerchantTotal {
            name,
            income_cny_minor: income,
            expense_cny_minor: expense,
        })
        .collect();
    merchants.sort_by(|a, b| {
        b.expense_cny_minor
            .cmp(&a.expense_cny_minor)
            .then(a.name.cmp(&b.name))
    });
    let merchant_total = merchants.len();
    let merchants = merchants
        .into_iter()
        .skip(query.merchant_offset as usize)
        .take(query.merchant_limit as usize)
        .collect();
    let cutoff = range
        .until_utc_exclusive
        .checked_sub_signed(Duration::microseconds(1))
        .ok_or_else(|| invalid("日期超出范围"))?;
    let mut balances = Vec::new();
    for account in repo.list_accounts()? {
        if query.account_id.is_none_or(|id| id == account.id) {
            balances.push(BalanceSnapshot {
                account_id: account.id,
                name: account.display_name,
                amount_cny_minor: repo
                    .balance_as_of(account.id, cutoff)?
                    .map(|m| m.minor_units()),
            });
        }
    }
    Ok(DetailedReport {
        summary,
        categories,
        merchants,
        merchant_total,
        balances,
        transaction_count: count,
    })
}
