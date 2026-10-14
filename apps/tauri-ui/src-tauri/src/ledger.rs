use chrono::{Local, NaiveDate, TimeZone, Utc};
use ledger_app::{AccountService, LedgerRepository, TransactionService};
use ledger_domain::{
    Account as DomainAccount, AccountId, AccountKind, CurrencyCode, Money, Posting, Transaction,
    TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use serde::Serialize;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;
#[derive(Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub category: String,
}
#[derive(Serialize)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub amount: String,
    pub note: String,
    pub time: String,
}
#[derive(Serialize)]
pub struct Reference {
    pub date: String,
    pub amount: String,
}
#[derive(Serialize)]
pub struct DayView {
    pub date: String,
    pub income: String,
    pub expense: String,
    pub net: String,
    pub balance: Option<String>,
    pub reference: Option<Reference>,
    pub entries: Vec<Entry>,
}

pub struct Ledger {
    path: PathBuf,
}
fn cny() -> CurrencyCode {
    CurrencyCode::new("CNY", 2).expect("constant currency")
}
fn money(value: i64) -> Money {
    Money::new(value, cny())
}
fn account_id(value: &str) -> Result<AccountId> {
    uuid::Uuid::parse_str(value)
        .map(AccountId::from_uuid)
        .map_err(|_| "无效的平台标识".into())
}
fn date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "请选择有效日期".into())
}
fn time(day: NaiveDate, closing: bool) -> Result<chrono::DateTime<Local>> {
    let naive = if closing {
        day.and_hms_micro_opt(23, 59, 59, 999_999)
    } else {
        day.and_hms_opt(12, 0, 0)
    }
    .ok_or("日期超出范围")?;
    Local
        .from_local_datetime(&naive)
        .single()
        .ok_or("该日期本地时间不可用".into())
}
fn parse_amount(value: &str, signed: bool) -> Result<i64> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let digits = if negative { &value[1..] } else { value };
    let parts: Vec<_> = digits.split('.').collect();
    if parts.len() > 2
        || parts[0].is_empty()
        || parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit()))
        || parts.get(1).is_some_and(|p| p.is_empty() || p.len() > 2)
        || (negative && !signed)
    {
        return Err("金额最多两位小数，收支金额必须为正数".into());
    }
    let whole: i64 = parts[0].parse().map_err(|_| "金额过大")?;
    let cents = match parts.get(1) {
        Some(p) => p.parse::<i64>().map_err(|_| "金额无效")? * if p.len() == 1 { 10 } else { 1 },
        None => 0,
    };
    let amount = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(cents))
        .ok_or("金额过大")?;
    if amount == 0 && !signed {
        return Err("收支金额必须大于零".into());
    }
    Ok(if negative { -amount } else { amount })
}
impl Ledger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let ledger = Self { path: path.into() };
        let repo = ledger.repo()?;
        if repo.list_accounts().map_err(|e| e.to_string())?.is_empty() {
            for name in ["微信", "支付宝", "现金"] {
                AccountService::new(ledger.repo()?)
                    .create_account(DomainAccount::new(name, AccountKind::Asset, cny()))
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(ledger)
    }
    fn repo(&self) -> Result<SqliteLedgerRepository> {
        SqliteLedgerRepository::open(&self.path).map_err(|e| e.to_string())
    }
    fn account(&self, id: &str) -> Result<DomainAccount> {
        let account = self
            .repo()?
            .get_account(account_id(id)?)
            .map_err(|e| e.to_string())?
            .ok_or("平台不存在")?;
        if account.currency != cny() {
            return Err("当前界面仅支持人民币平台".into());
        }
        Ok(account)
    }
    pub fn accounts(&self) -> Result<Vec<Account>> {
        let repo = self.repo()?;
        repo.list_accounts()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|a| {
                let category = repo
                    .platform_category(a.id)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| {
                        match a.display_name.as_str() {
                            "微信" | "支付宝" => "wallet",
                            "现金" => "cash",
                            _ => "other",
                        }
                        .into()
                    });
                Ok(Account {
                    id: a.id.as_uuid().to_string(),
                    name: a.display_name,
                    category,
                })
            })
            .collect()
    }
    #[cfg(test)]
    pub fn create_account(&self, name: &str) -> Result<Account> {
        self.create_platform(name, "other")
    }
    fn validate_platform(name: &str, category: &str) -> Result<()> {
        if name.is_empty() || name.chars().count() > 40 {
            return Err("平台名称需为 1–40 个字符".into());
        }
        if !["wallet", "bank", "securities", "exchange", "cash", "other"].contains(&category) {
            return Err("请选择有效的平台分类".into());
        }
        Ok(())
    }
    pub fn create_platform(&self, name: &str, category: &str) -> Result<Account> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        let id = AccountId::new();
        self.repo()?
            .save_platform(id, name, category, true)
            .map_err(|e| e.to_string())?;
        Ok(Account {
            id: id.as_uuid().to_string(),
            name: name.into(),
            category: category.into(),
        })
    }
    pub fn update_platform(&self, id: &str, name: &str, category: &str) -> Result<()> {
        let name = name.trim();
        Self::validate_platform(name, category)?;
        self.repo()?
            .save_platform(account_id(id)?, name, category, false)
            .map_err(|e| e.to_string())
    }
    pub fn save_entry(
        &self,
        id: &str,
        day: &str,
        amount: &str,
        note: &str,
        income: bool,
    ) -> Result<()> {
        let account = self.account(id)?;
        let amount = parse_amount(amount, false)?;
        if note.chars().count() > 200 {
            return Err("备注最多 200 个字符".into());
        }
        let sign = if income == (account.kind == AccountKind::Asset) {
            1
        } else {
            -1
        };
        TransactionService::new(self.repo()?)
            .confirm(Transaction {
                id: TransactionId::new(),
                occurred_at: time(date(day)?, false)?.with_timezone(&Utc),
                kind: if income {
                    TransactionKind::Income
                } else {
                    TransactionKind::Expense
                },
                status: TransactionStatus::Confirmed,
                postings: vec![Posting {
                    account_id: account.id,
                    delta: money(amount * sign),
                }],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(money(amount)),
                merchant: None,
                description: if note.trim().is_empty() {
                    None
                } else {
                    Some(note.trim().into())
                },
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn set_reference(&self, id: &str, day: &str, amount: &str) -> Result<()> {
        let id = self.account(id)?.id;
        let day = date(day)?;
        AccountService::new(self.repo()?)
            .set_day_closing_balance(
                id,
                money(parse_amount(amount, true)?),
                day,
                *time(day, true)?.offset(),
            )
            .map_err(|e| e.to_string())
    }
    pub fn clear_reference(&self, id: &str) -> Result<()> {
        AccountService::new(self.repo()?)
            .clear_opening_balance(self.account(id)?.id)
            .map_err(|e| e.to_string())
    }
    pub fn day(&self, id: &str, day: &str) -> Result<DayView> {
        let account = self.account(id)?;
        let day = date(day)?;
        let repo = self.repo()?;
        let balance = repo
            .balance_as_of(account.id, time(day, true)?.with_timezone(&Utc))
            .map_err(|e| e.to_string())?
            .map(|m| m.decimal_string());
        let mut income = 0_i64;
        let mut expense = 0_i64;
        let mut entries = Vec::new();
        for tx in repo
            .list_transactions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .rev()
        {
            if tx.status != TransactionStatus::Confirmed
                || tx.occurred_at.with_timezone(&Local).date_naive() != day
            {
                continue;
            }
            let Some(posting) = tx.postings.iter().find(|p| p.account_id == account.id) else {
                continue;
            };
            if let Some(effect) = ledger_domain::report_effect(&tx) {
                income = income
                    .checked_add(effect.income_cny_minor)
                    .ok_or("收入合计过大")?;
                expense = expense
                    .checked_add(effect.expense_cny_minor)
                    .ok_or("支出合计过大")?;
            }
            let kind = match tx.kind {
                TransactionKind::Income => "income",
                TransactionKind::Expense => "expense",
                TransactionKind::Refund => "refund",
                TransactionKind::Transfer => "transfer",
                TransactionKind::BalanceAdjustment => "adjustment",
            };
            entries.push(Entry {
                id: tx.id.as_uuid().to_string(),
                kind: kind.into(),
                amount: posting.delta.decimal_string(),
                note: tx.description.unwrap_or_default(),
                time: tx
                    .occurred_at
                    .with_timezone(&Local)
                    .format("%H:%M")
                    .to_string(),
            });
        }
        let reference = account
            .opening_balance
            .zip(account.opening_balance_effective_at)
            .map(|(amount, dt)| Reference {
                date: dt.with_timezone(&Local).date_naive().to_string(),
                amount: amount.decimal_string(),
            });
        Ok(DayView {
            date: day.to_string(),
            income: money(income).decimal_string(),
            expense: money(expense).decimal_string(),
            net: money(income.checked_sub(expense).ok_or("净变化过大")?).decimal_string(),
            balance,
            reference,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_management_persists_and_preserves_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let ledger = Ledger::open(&path).unwrap();
        let original = ledger
            .accounts()
            .unwrap()
            .into_iter()
            .find(|a| a.name == "微信")
            .unwrap();
        assert_eq!(original.category, "wallet");
        ledger
            .save_entry(&original.id, "2026-10-14", "12", "", true)
            .unwrap();
        ledger
            .update_platform(&original.id, "我的钱包", "wallet")
            .unwrap();
        let bank = ledger.create_platform("招商银行", "bank").unwrap();
        assert!(
            ledger
                .update_platform(&bank.id, "我的钱包", "bank")
                .is_err()
        );
        assert!(ledger.create_platform("无效", "invalid").is_err());
        assert!(ledger.update_platform(&bank.id, "", "bank").is_err());
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        let accounts = ledger.accounts().unwrap();
        assert_eq!(
            accounts.iter().find(|a| a.id == bank.id).unwrap().category,
            "bank"
        );
        assert_eq!(
            accounts.iter().find(|a| a.id == original.id).unwrap().name,
            "我的钱包"
        );
        assert_eq!(
            ledger.day(&original.id, "2026-10-14").unwrap().income,
            "12.00"
        );
        assert_eq!(accounts.len(), 4);
    }
    #[test]
    fn ipc_flow_preserves_daily_projection_and_data_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let ledger = Ledger::open(&path).unwrap();
        let id = ledger.accounts().unwrap()[0].id.clone();
        assert!(ledger.day(&id, "2026-01-04").unwrap().balance.is_none());
        ledger
            .save_entry(&id, "2026-01-04", "20", "工资", true)
            .unwrap();
        ledger
            .save_entry(&id, "2026-01-04", "5", "午饭", false)
            .unwrap();
        ledger.set_reference(&id, "2026-01-04", "100").unwrap();
        assert_eq!(
            ledger.day(&id, "2026-01-03").unwrap().balance.as_deref(),
            Some("85.00")
        );
        ledger
            .save_entry(&id, "2026-01-05", "30", "", true)
            .unwrap();
        ledger
            .save_entry(&id, "2026-01-05", "10", "", false)
            .unwrap();
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        let view = ledger.day(&id, "2026-01-05").unwrap();
        assert_eq!(view.balance.as_deref(), Some("120.00"));
        assert_eq!(view.entries.len(), 2);
        let other = ledger.create_account("银行卡").unwrap();
        assert!(
            ledger
                .day(&other.id, "2026-01-05")
                .unwrap()
                .entries
                .is_empty()
        );
        ledger.clear_reference(&id).unwrap();
        let view = ledger.day(&id, "2026-01-05").unwrap();
        assert!(view.balance.is_none());
        assert_eq!(view.net, "20.00");
    }
    #[test]
    fn invalid_command_input_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
        let id = ledger.accounts().unwrap()[0].id.clone();
        for amount in [
            "",
            "1.123",
            "-1",
            "0",
            "1e2",
            "NaN",
            "999999999999999999999",
        ] {
            assert!(
                ledger
                    .save_entry(&id, "2026-01-04", amount, "", true)
                    .is_err()
            );
        }
        assert!(
            ledger
                .save_entry("bad-id", "2026-01-04", "1", "", true)
                .is_err()
        );
        assert!(ledger.save_entry(&id, "2026-02-31", "1", "", true).is_err());
        assert!(ledger.day(&id, "2026-01-04").unwrap().entries.is_empty());
        ledger.set_reference(&id, "2026-01-04", "0").unwrap();
        assert_eq!(
            ledger.day(&id, "2026-01-04").unwrap().balance.as_deref(),
            Some("0.00")
        );
    }
}

#[derive(serde::Deserialize)]
pub struct PageQuery {
    pub status: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub from_date: Option<String>,
    pub until_date: Option<String>,
    pub merchant: Option<String>,
    pub keyword: Option<String>,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Serialize)]
pub struct PageEntry {
    pub id: String,
    pub status: String,
    pub kind: String,
    pub occurred_at: String,
    pub amount: Option<String>,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub edit_version: u32,
    pub reversal_of: Option<String>,
}
#[derive(Serialize)]
pub struct EntryPage {
    pub items: Vec<PageEntry>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}
impl CommandError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".into(),
            message: message.into(),
        }
    }
}
impl Ledger {
    pub fn page(&self, q: PageQuery) -> std::result::Result<EntryPage, CommandError> {
        use ledger_app::FeatureRepository;
        let midnight = |v: &str| -> Result<chrono::DateTime<Utc>> {
            let d = date(v)?;
            Local
                .from_local_datetime(&d.and_hms_opt(0, 0, 0).ok_or("日期超出范围")?)
                .earliest()
                .map(|t| t.with_timezone(&Utc))
                .ok_or("日期本地边界不可用".into())
        };
        let status = match q.status.as_deref() {
            None => None,
            Some("pending") => Some(TransactionStatus::Pending),
            Some("confirmed") => Some(TransactionStatus::Confirmed),
            Some("ignored") => Some(TransactionStatus::Ignored),
            _ => return Err(CommandError::invalid("无效的流水状态")),
        };
        let query = ledger_domain::PendingQuery {
            status,
            account_id: q
                .account_id
                .as_deref()
                .map(account_id)
                .transpose()
                .map_err(CommandError::invalid)?,
            category_id: q
                .category_id
                .as_deref()
                .map(|v| {
                    uuid::Uuid::parse_str(v)
                        .map(ledger_domain::CategoryId::from_uuid)
                        .map_err(|_| CommandError::invalid("无效的分类标识"))
                })
                .transpose()?,
            from_utc_inclusive: q
                .from_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            until_utc_exclusive: q
                .until_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            merchant: q.merchant,
            keyword: q.keyword,
            offset: q.offset,
            limit: q.limit,
        };
        let repo = self.repo().map_err(|_| CommandError {
            code: "storage_error".into(),
            message: "无法打开本地账本".into(),
        })?;
        let page = repo.query_transactions(query).map_err(|e| CommandError {
            code: match e {
                ledger_app::RepositoryError::InvalidData(_) => "invalid_input",
                ledger_app::RepositoryError::NotFound => "not_found",
                ledger_app::RepositoryError::Conflict => "conflict",
                _ => "storage_error",
            }
            .into(),
            message: "无法查询流水，请检查筛选条件或重试".into(),
        })?;
        Ok(EntryPage {
            total: page.total,
            offset: page.offset,
            limit: page.limit,
            items: page
                .items
                .into_iter()
                .map(|c| {
                    let t = c.transaction;
                    PageEntry {
                        id: t.id.as_uuid().to_string(),
                        status: match t.status {
                            TransactionStatus::Pending => "pending",
                            TransactionStatus::Confirmed => "confirmed",
                            TransactionStatus::Ignored => "ignored",
                        }
                        .into(),
                        kind: format!("{:?}", t.kind),
                        occurred_at: t.occurred_at.to_rfc3339(),
                        amount: t.report_amount_cny.map(|v| v.decimal_string()),
                        merchant: t.merchant,
                        description: t.description,
                        edit_version: c.metadata.edit_version,
                        reversal_of: c.metadata.reversal_of.map(|v| v.as_uuid().to_string()),
                    }
                })
                .collect(),
        })
    }
}
