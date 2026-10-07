//! Application boundary shared by the desktop and Android mobile screens.
use crate::*;
use chrono::{Datelike, Months, TimeZone, Utc};
use ledger_domain::*;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, Default)]
pub struct PendingDetails {
    pub amount: Option<Money>,
    pub source: String,
    pub account_hint: Option<String>,
    pub original_external_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MobileSettings {
    pub notification_wechat: bool,
    pub notification_alipay: bool,
    pub import_wechat: bool,
    pub import_alipay: bool,
    pub import_bank: bool,
    pub api_enabled: bool,
    pub api_wechat: bool,
    pub api_alipay: bool,
    pub api_bank: bool,
    pub backup_enabled: bool,
    pub default_account: Option<AccountId>,
}
pub trait MobileRepository:
    LedgerRepository + ClassificationRuleRepository + ImportRepository
{
    fn settings(&self) -> Result<MobileSettings, RepositoryError>;
    fn save_settings(&self, value: &MobileSettings) -> Result<(), RepositoryError>;
    fn pending_details(&self, id: TransactionId)
    -> Result<Option<PendingDetails>, RepositoryError>;
    fn capture_pending(
        &self,
        record: ImportRecord,
        details: PendingDetails,
    ) -> Result<usize, RepositoryError>;
    /// Compare-and-replace atomically; prevents stale editors from overwriting confirmation.
    fn replace_transaction(
        &self,
        expected: &Transaction,
        replacement: &Transaction,
    ) -> Result<(), RepositoryError>;
}
#[derive(Clone, Default)]
pub struct TransactionFilter {
    pub from: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub source: Option<String>,
    pub account_id: Option<AccountId>,
    pub category_id: Option<CategoryId>,
    pub status: Option<TransactionStatus>,
}
#[derive(Clone)]
pub struct TransactionDraft {
    pub occurred_at: Timestamp,
    pub kind: TransactionKind,
    pub account_id: AccountId,
    pub destination_id: Option<AccountId>,
    pub amount: Money,
    pub report_amount_cny: Option<Money>,
    pub category_id: Option<CategoryId>,
    pub original_transaction_id: Option<TransactionId>,
    pub merchant: Option<String>,
    pub memo: Option<String>,
}
#[derive(Clone)]
pub struct TransactionRow {
    pub transaction: Transaction,
    pub source: String,
    pub details: Option<PendingDetails>,
}
pub struct AccountBalance {
    pub account: Account,
    pub balance: Money,
}
pub struct MobileSnapshot {
    pub accounts: Vec<AccountBalance>,
    pub categories: Vec<Category>,
    pub transactions: Vec<TransactionRow>,
    pub pending: Vec<TransactionRow>,
    pub month: ReportSummary,
    pub refund_cny_minor: i64,
    pub merchants: Vec<(String, i64)>,
    pub trend: Vec<(String, i64, i64)>,
}
pub struct MobileLedger<R> {
    repository: R,
}
fn invalid(message: &str) -> AppError {
    RepositoryError::InvalidData(message.into()).into()
}
impl<R: MobileRepository> MobileLedger<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn repository(&self) -> &R {
        &self.repository
    }
    pub fn delete_data(&self, scope: DeletionScope, confirmed: bool) -> Result<(), AppError>
    where
        R: DeletionRepository,
    {
        if !confirmed {
            return Err(invalid("Explicit deletion confirmation is required"));
        }
        self.repository.delete_scope(scope)?;
        Ok(())
    }
    pub fn settings(&self) -> Result<MobileSettings, AppError> {
        Ok(self.repository.settings()?)
    }
    pub fn save_settings(&self, value: MobileSettings) -> Result<(), AppError> {
        if let Some(id) = value.default_account
            && self.repository.get_account(id)?.is_none()
        {
            return Err(AppError::AccountNotFound);
        }
        self.repository.save_settings(&value)?;
        Ok(())
    }
    pub fn create_account(
        &self,
        name: &str,
        kind: AccountKind,
        balance: Money,
        effective: Timestamp,
    ) -> Result<AccountId, AppError> {
        if name.trim().is_empty() {
            return Err(invalid("Account name is required"));
        }
        let id = AccountId::new();
        self.repository.insert_account(Account {
            id,
            display_name: name.trim().into(),
            kind,
            currency: balance.currency().clone(),
            opening_balance: balance,
            opening_balance_effective_at: effective,
        })?;
        Ok(id)
    }
    pub fn save_rule(&self, rule: ClassificationRule, new: bool) -> Result<(), AppError> {
        if rule
            .merchant_contains
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            && rule
                .description_contains
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err(invalid("A merchant or keyword is required"));
        }
        if new {
            self.repository.insert_rule(rule)?;
        } else {
            self.repository.update_rule(rule)?;
        }
        Ok(())
    }
    pub fn create_category(&self, name: &str) -> Result<CategoryId, AppError> {
        if name.trim().is_empty() {
            return Err(invalid("Category name is required"));
        }
        let id = CategoryId::new();
        self.repository.insert_category(Category {
            id,
            name: name.trim().into(),
            enabled: true,
        })?;
        Ok(id)
    }
    pub fn change_category(
        &self,
        id: CategoryId,
        name: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        if name.trim().is_empty() {
            return Err(invalid("Category name is required"));
        }
        let mut c = self
            .repository
            .get_category(id)?
            .ok_or(AppError::CategoryNotFound)?;
        c.name = name.trim().into();
        c.enabled = enabled;
        self.repository.update_category(c)?;
        Ok(())
    }
    fn category_valid(&self, id: Option<CategoryId>) -> Result<(), AppError> {
        if let Some(id) = id {
            let c = self
                .repository
                .get_category(id)?
                .ok_or(AppError::CategoryNotFound)?;
            if !c.enabled {
                return Err(AppError::CategoryDisabled);
            }
        }
        Ok(())
    }
    fn build(&self, id: TransactionId, d: TransactionDraft) -> Result<Transaction, AppError> {
        if d.amount.minor_units() <= 0 {
            return Err(invalid("Amount must be positive"));
        }
        self.category_valid(d.category_id)?;
        let a = self
            .repository
            .get_account(d.account_id)?
            .ok_or(AppError::AccountNotFound)?;
        if !a.currency_matches(&d.amount) {
            return Err(AppError::AccountCurrencyMismatch);
        }
        if let Some(m) = &d.report_amount_cny
            && (m.currency().code() != "CNY"
                || m.currency().minor_unit_exponent() != 2
                || m.minor_units() < 0)
        {
            return Err(invalid("Report amount must be CNY"));
        }
        let sign = match (d.kind, a.kind) {
            (TransactionKind::Expense, AccountKind::Asset)
            | (TransactionKind::Income | TransactionKind::Refund, AccountKind::Liability) => -1,
            (TransactionKind::Transfer, AccountKind::Asset) => -1,
            (TransactionKind::Transfer, AccountKind::Liability) => 1,
            _ => 1,
        };
        let mut postings = vec![Posting {
            account_id: a.id,
            delta: Money::new(sign * d.amount.minor_units(), a.currency.clone()),
        }];
        if d.kind == TransactionKind::Transfer {
            let to = self
                .repository
                .get_account(
                    d.destination_id
                        .ok_or_else(|| invalid("Destination account is required"))?,
                )?
                .ok_or(AppError::AccountNotFound)?;
            let sign = if to.kind == AccountKind::Asset { 1 } else { -1 };
            postings.push(Posting {
                account_id: to.id,
                delta: Money::new(sign * d.amount.minor_units(), to.currency),
            });
        }
        let t = Transaction {
            id,
            occurred_at: d.occurred_at,
            kind: d.kind,
            status: TransactionStatus::Confirmed,
            postings,
            category_id: d.category_id,
            original_transaction_id: d.original_transaction_id,
            report_amount_cny: if d.kind == TransactionKind::Transfer {
                None
            } else if d.amount.currency().code() == "CNY" {
                Some(d.amount)
            } else {
                d.report_amount_cny
            },
            merchant: d.merchant,
            description: d.memo,
        };
        validate_transaction_shape(&t, &self.repository.list_accounts()?)?;
        if t.kind == TransactionKind::Refund {
            let p = self
                .repository
                .get_transaction(
                    t.original_transaction_id
                        .ok_or(AppError::RefundParentInvalid)?,
                )?
                .ok_or(AppError::RefundParentInvalid)?;
            if p.kind != TransactionKind::Expense || p.status != TransactionStatus::Confirmed {
                return Err(AppError::RefundParentInvalid);
            }
        }
        Ok(t)
    }
    pub fn create_transaction(&self, d: TransactionDraft) -> Result<TransactionId, AppError> {
        let t = self.build(TransactionId::new(), d)?;
        let id = t.id;
        self.repository.insert_transaction(t)?;
        Ok(id)
    }
    pub fn confirm_pending(&self, id: TransactionId, d: TransactionDraft) -> Result<(), AppError> {
        let old = self
            .repository
            .get_transaction(id)?
            .ok_or(AppError::TransactionNotFound)?;
        let t = self.build(id, d)?;
        if old.status == TransactionStatus::Confirmed {
            return if old == t {
                Ok(())
            } else {
                Err(AppError::TransactionConflict)
            };
        }
        if old.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending);
        }
        self.repository.replace_transaction(&old, &t)?;
        Ok(())
    }
    pub fn ignore(&self, id: TransactionId) -> Result<(), AppError> {
        let old = self
            .repository
            .get_transaction(id)?
            .ok_or(AppError::TransactionNotFound)?;
        if old.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending);
        }
        let mut t = old.clone();
        t.status = TransactionStatus::Ignored;
        self.repository.replace_transaction(&old, &t)?;
        Ok(())
    }
    pub fn edit_annotations(
        &self,
        id: TransactionId,
        category: Option<CategoryId>,
        memo: Option<String>,
    ) -> Result<(), AppError> {
        self.category_valid(category)?;
        let old = self
            .repository
            .get_transaction(id)?
            .ok_or(AppError::TransactionNotFound)?;
        let mut t = old.clone();
        t.category_id = category;
        t.description = memo;
        self.repository.replace_transaction(&old, &t)?;
        Ok(())
    }
    pub fn snapshot(
        &self,
        now: Timestamp,
        filter: TransactionFilter,
    ) -> Result<MobileSnapshot, AppError> {
        self.snapshot_in_timezone(now, chrono::FixedOffset::east_opt(0).unwrap(), filter)
    }
    pub fn snapshot_in_timezone(
        &self,
        now: Timestamp,
        offset: chrono::FixedOffset,
        filter: TransactionFilter,
    ) -> Result<MobileSnapshot, AppError> {
        let accounts = self
            .repository
            .list_accounts()?
            .into_iter()
            .map(|account| {
                Ok(AccountBalance {
                    balance: self.repository.balance_as_of(account.id, now)?,
                    account,
                })
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        let evidence = self.repository.list_import_records()?;
        let mut rows = Vec::new();
        for t in self.repository.list_transactions()? {
            let details = self.repository.pending_details(t.id)?;
            let source = details
                .as_ref()
                .map(|d| d.source.clone())
                .unwrap_or_else(|| {
                    evidence
                        .iter()
                        .find(|e| e.transaction.id == t.id)
                        .map(|e| source_name(e.evidence.source_kind).into())
                        .unwrap_or_else(|| "manual".into())
                });
            rows.push(TransactionRow {
                transaction: t,
                source,
                details,
            });
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r.transaction.occurred_at));
        let pending = rows
            .iter()
            .filter(|r| r.transaction.status == TransactionStatus::Pending)
            .cloned()
            .collect();
        // Calendar boundaries follow the device's UTC offset supplied by the UI's local date.
        let local = now.with_timezone(&offset);
        let start_local = offset
            .with_ymd_and_hms(local.year(), local.month(), 1, 0, 0, 0)
            .single()
            .ok_or_else(|| invalid("Invalid calendar month"))?;
        let start = start_local.with_timezone(&Utc);
        let end_local = start_local
            .checked_add_months(Months::new(1))
            .ok_or_else(|| invalid("Invalid calendar month"))?;
        let end = end_local.with_timezone(&Utc);
        let mut month = ReportSummary::default();
        let mut refund = 0i64;
        let mut merchants = BTreeMap::<String, i64>::new();
        let mut trend = Vec::new();
        for i in (0..6).rev() {
            let from_local = start_local.checked_sub_months(Months::new(i)).unwrap();
            let until_local = from_local.checked_add_months(Months::new(1)).unwrap();
            let from = from_local.with_timezone(&Utc);
            let until = until_local.with_timezone(&Utc);
            let mut inc = 0i64;
            let mut exp = 0i64;
            for r in &rows {
                let t = &r.transaction;
                if t.occurred_at < from || t.occurred_at >= until {
                    continue;
                }
                if let Some(e) = report_effect(t) {
                    inc = add(inc, e.income_cny_minor)?;
                    exp = add(exp, e.expense_cny_minor)?;
                }
            }
            trend.push((
                from.with_timezone(&offset).format("%Y-%m").to_string(),
                inc,
                exp,
            ));
        }
        for r in &rows {
            let t = &r.transaction;
            if t.occurred_at < start || t.occurred_at >= end {
                continue;
            }
            if let Some(e) = report_effect(t) {
                month.income_cny_minor = add(month.income_cny_minor, e.income_cny_minor)?;
                month.expense_cny_minor = add(month.expense_cny_minor, e.expense_cny_minor)?;
                if let Some(id) = t.category_id {
                    let v = month.expense_by_category.entry(id).or_default();
                    *v = add(*v, e.expense_cny_minor)?;
                }
                if e.expense_cny_minor != 0 {
                    let v = merchants
                        .entry(t.merchant.clone().unwrap_or_else(|| "Unspecified".into()))
                        .or_default();
                    *v = add(*v, e.expense_cny_minor)?;
                }
                if t.kind == TransactionKind::Refund {
                    refund = add(refund, -e.expense_cny_minor)?;
                }
            }
        }
        month.net_cny_minor = month
            .income_cny_minor
            .checked_sub(month.expense_cny_minor)
            .ok_or_else(|| invalid("Report overflow"))?;
        let mut merchants: Vec<_> = merchants.into_iter().collect();
        merchants.sort_by_key(|m| std::cmp::Reverse(m.1));
        rows.retain(|r| {
            let t = &r.transaction;
            filter.from.is_none_or(|v| t.occurred_at >= v)
                && filter.until.is_none_or(|v| t.occurred_at < v)
                && filter.source.as_ref().is_none_or(|v| {
                    v == &r.source
                        || evidence.iter().any(|e| {
                            e.transaction.id == t.id && source_name(e.evidence.source_kind) == v
                        })
                })
                && filter
                    .account_id
                    .is_none_or(|v| t.postings.iter().any(|p| p.account_id == v))
                && filter.category_id.is_none_or(|v| t.category_id == Some(v))
                && filter.status.is_none_or(|v| t.status == v)
        });
        Ok(MobileSnapshot {
            accounts,
            categories: self.repository.list_categories()?,
            transactions: rows,
            pending,
            month,
            refund_cny_minor: refund,
            merchants,
            trend,
        })
    }
}
fn add(a: i64, b: i64) -> Result<i64, AppError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("Report amount overflow"))
}
fn source_name(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Alipay => "alipay",
        SourceKind::WeChat => "wechat",
        SourceKind::BankCsv => "bank",
        SourceKind::Notification => "notification",
        SourceKind::Manual => "manual",
    }
}

/// Import adapters can submit their preview through this application boundary.
impl<R: MobileRepository> ImportRepository for MobileLedger<R> {
    fn list_import_records(&self) -> Result<Vec<ImportRecord>, RepositoryError> {
        self.repository.list_import_records()
    }
    fn confirm_import_batch(
        &self,
        id: uuid::Uuid,
        mut records: Vec<ImportRecord>,
    ) -> Result<usize, RepositoryError> {
        let accounts = self.repository.list_accounts()?;
        for record in &records {
            if record.transaction.status != TransactionStatus::Confirmed {
                return Err(RepositoryError::InvalidData(
                    "Confirm the import preview before writing".into(),
                ));
            }
            validate_transaction_shape(&record.transaction, &accounts)
                .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
            if matches!(
                record.transaction.kind,
                TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund
            ) && record.transaction.report_amount_cny.is_none()
            {
                return Err(RepositoryError::InvalidData(
                    "A CNY report amount is required for imported foreign amounts".into(),
                ));
            }
        }
        // Expenses in this explicit atomic batch must exist before their refunds.
        records.sort_by_key(|record| record.transaction.kind == TransactionKind::Refund);
        self.repository.confirm_import_batch(id, records)
    }
}
