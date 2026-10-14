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
    preview: std::cell::RefCell<Option<PreviewSession>>,
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
fn parse_source(value: &str) -> Result<ledger_domain::SourceKind> {
    match value {
        "wechat" => Ok(ledger_domain::SourceKind::WeChat),
        "alipay" => Ok(ledger_domain::SourceKind::Alipay),
        "bank" => Ok(ledger_domain::SourceKind::Bank),
        "notification" => Ok(ledger_domain::SourceKind::Notification),
        "manual" => Ok(ledger_domain::SourceKind::Manual),
        _ => Err("无效的来源".into()),
    }
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
        let ledger = Self {
            path: path.into(),
            preview: std::cell::RefCell::new(None),
        };
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
    pub source: Option<String>,
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
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
    pub sources: Vec<String>,
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
            source_kind: q
                .source
                .as_deref()
                .map(parse_source)
                .transpose()
                .map_err(CommandError::invalid)?,
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
                    let sources = repo.source_evidence(t.id).map_err(|_| CommandError {
                        code: "storage_error".into(),
                        message: "无法读取流水来源，请重试".into(),
                    })?;
                    Ok::<PageEntry, CommandError>(PageEntry {
                        id: t.id.as_uuid().to_string(),
                        status: match t.status {
                            TransactionStatus::Pending => "pending",
                            TransactionStatus::Confirmed => "confirmed",
                            TransactionStatus::Ignored => "ignored",
                        }
                        .into(),
                        kind: kind_label(t.kind).into(),
                        occurred_at: t.occurred_at.to_rfc3339(),
                        amount: t.report_amount_cny.map(|v| v.decimal_string()),
                        merchant: t.merchant,
                        description: t.description,
                        edit_version: c.metadata.edit_version,
                        reversal_of: c.metadata.reversal_of.map(|v| v.as_uuid().to_string()),
                        account_id: t
                            .postings
                            .first()
                            .map(|p| p.account_id.as_uuid().to_string()),
                        category_id: t.category_id.map(|id| id.as_uuid().to_string()),
                        original_transaction_id: t
                            .original_transaction_id
                            .map(|id| id.as_uuid().to_string()),
                        sources: sources
                            .iter()
                            .map(|e| source_label(e.source_kind).into())
                            .collect(),
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?,
        })
    }
}

pub fn ignore_pending(ledger: &Ledger, ids: Vec<String>) -> Result<()> {
    use ledger_app::FeatureRepository;
    let repo = ledger.repo()?;
    let ids = ids
        .into_iter()
        .map(|id| {
            uuid::Uuid::parse_str(&id)
                .map(ledger_domain::TransactionId::from_uuid)
                .map_err(|_| "无效的流水标识".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    repo.ignore_bulk(&ids).map_err(|e| e.to_string())
}
pub fn restore_pending(ledger: &Ledger, id: String) -> Result<()> {
    use ledger_app::FeatureRepository;
    let id = uuid::Uuid::parse_str(&id)
        .map(ledger_domain::TransactionId::from_uuid)
        .map_err(|_| "无效的流水标识".to_owned())?;
    ledger
        .repo()?
        .restore_pending_atomic(id)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct BillPreview {
    pub preview_id: String,
    pub rows: Vec<BillRow>,
    pub errors: Vec<BillIssue>,
    pub new_count: usize,
    pub exact_duplicates: usize,
    pub suspected_duplicates: usize,
    pub pending_count: usize,
}
#[derive(Serialize)]
pub struct BillIssue {
    pub row: usize,
    pub message: String,
}
#[derive(Serialize)]
pub struct BillRow {
    pub id: String,
    pub row: usize,
    pub date: String,
    pub amount: String,
    pub kind: String,
    pub merchant: String,
    pub description: String,
    pub decision: String,
    pub duplicate_id: Option<String>,
    pub warnings: Vec<String>,
}
struct PreviewSession {
    id: uuid::Uuid,
    rows: Vec<(
        String,
        ledger_import::NormalizedImportRow,
        ledger_import::PreviewDecision,
    )>,
}
#[derive(serde::Deserialize)]
pub struct PendingEdit {
    pub id: String,
    pub expected_version: u32,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub kind: String,
    pub merchant: String,
    pub description: String,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
}
#[derive(Serialize)]
pub struct CategoryOption {
    pub id: String,
    pub name: String,
}

fn transaction_id(value: &str) -> Result<TransactionId> {
    uuid::Uuid::parse_str(value)
        .map(TransactionId::from_uuid)
        .map_err(|_| "无效的流水标识".into())
}
fn source_label(source: ledger_domain::SourceKind) -> &'static str {
    match source {
        ledger_domain::SourceKind::WeChat => "wechat",
        ledger_domain::SourceKind::Alipay => "alipay",
        ledger_domain::SourceKind::Bank => "bank",
        ledger_domain::SourceKind::Notification => "notification",
        ledger_domain::SourceKind::Manual => "manual",
    }
}
impl Ledger {
    pub fn bill_preview(&self, provider: &str, bytes: Vec<u8>) -> Result<BillPreview> {
        use ledger_app::FeatureRepository;
        if bytes.len() > 5 * 1024 * 1024 {
            return Err("账单文件不能超过 5 MiB".into());
        }
        let parsed = match provider {
            "wechat" => ledger_import::parse_wechat_csv(&bytes),
            "alipay" => ledger_import::parse_alipay_csv(&bytes),
            "bank" => ledger_import::parse_bank_csv(&bytes),
            _ => return Err("请选择账单来源".into()),
        }
        .map_err(|error| error.to_string())?;
        if parsed.rows.len() > 1000 {
            return Err("每次最多预览 1000 行，请拆分文件".into());
        }
        let repo = self.repo()?;
        let mut existing = Vec::new();
        let mut seen_transactions = std::collections::HashSet::new();
        for row in &parsed.rows {
            // Page nearby candidates in Rust. Provenance is optional for advisory
            // matching; stable source IDs are indexed separately for exact matches.
            let mut offset = 0;
            loop {
                let page = repo
                    .query_transactions(ledger_domain::PendingQuery {
                        from_utc_inclusive: Some(row.occurred_at - chrono::Duration::hours(24)),
                        until_utc_exclusive: Some(row.occurred_at + chrono::Duration::hours(24)),
                        offset,
                        limit: 200,
                        ..Default::default()
                    })
                    .map_err(|error| error.to_string())?;
                let count = page.items.len() as u32;
                for candidate in page.items {
                    let tx = candidate.transaction;
                    if !seen_transactions.insert(tx.id) {
                        continue;
                    }
                    if let Some(amount) = &tx.report_amount_cny {
                        existing.push(ledger_import::ExistingImport {
                            transaction_id: Some(tx.id),
                            source_kind: ledger_domain::SourceKind::Manual,
                            external_transaction_id: None,
                            occurred_at: tx.occurred_at,
                            kind: tx.kind,
                            amount_minor: amount.minor_units(),
                            merchant: tx.merchant.clone(),
                        });
                        for evidence in repo
                            .source_evidence(tx.id)
                            .map_err(|error| error.to_string())?
                        {
                            existing.push(ledger_import::ExistingImport {
                                transaction_id: Some(tx.id),
                                source_kind: evidence.source_kind,
                                external_transaction_id: evidence.external_transaction_id,
                                occurred_at: tx.occurred_at,
                                kind: tx.kind,
                                amount_minor: amount.minor_units(),
                                merchant: tx.merchant.clone(),
                            });
                        }
                    }
                }
                offset = offset.checked_add(count).ok_or("候选流水数量超出范围")?;
                if count == 0 || u64::from(offset) >= page.total {
                    break;
                }
            }
            if let Some(external_id) = &row.external_transaction_id {
                use ledger_app::ImportRepository;
                if let Some(id) = repo
                    .find_source_transaction(row.source_kind, external_id)
                    .map_err(|error| error.to_string())?
                {
                    if !existing.iter().any(|item| {
                        item.source_kind == row.source_kind
                            && item.external_transaction_id.as_deref() == Some(external_id)
                    }) {
                        if let Some(tx) = repo
                            .get_transaction(id)
                            .map_err(|error| error.to_string())?
                        {
                            existing.push(ledger_import::ExistingImport {
                                transaction_id: Some(id),
                                source_kind: row.source_kind,
                                external_transaction_id: Some(external_id.clone()),
                                occurred_at: tx.occurred_at,
                                kind: tx.kind,
                                amount_minor: tx
                                    .report_amount_cny
                                    .as_ref()
                                    .map(Money::minor_units)
                                    .unwrap_or_default(),
                                merchant: tx.merchant,
                            });
                        }
                    }
                }
            }
        }
        let preview = ledger_import::ImportPreview::build(&parsed.rows, &existing);
        let preview_id = uuid::Uuid::new_v4();
        let rows = preview
            .rows
            .iter()
            .map(|item| {
                let id = uuid::Uuid::new_v4().to_string();
                (id, item.row.clone(), item.decision.clone())
            })
            .collect::<Vec<_>>();
        let result = BillPreview {
            preview_id: preview_id.to_string(),
            rows: rows
                .iter()
                .map(|(id, row, decision)| {
                    let (status, duplicate_id) = match decision {
                        ledger_import::PreviewDecision::New => ("new", None),
                        ledger_import::PreviewDecision::ExactDuplicate { transaction_id } => (
                            "exact_duplicate",
                            transaction_id.map(|id| id.as_uuid().to_string()),
                        ),
                        ledger_import::PreviewDecision::SuspectedDuplicate { transaction_id } => (
                            "suspected_duplicate",
                            transaction_id.map(|id| id.as_uuid().to_string()),
                        ),
                    };
                    BillRow {
                        id: id.clone(),
                        row: row.row_number,
                        date: row
                            .occurred_at
                            .with_timezone(&Local)
                            .date_naive()
                            .to_string(),
                        amount: Money::new(row.amount_minor, row.currency.clone()).decimal_string(),
                        kind: kind_label(row.kind).into(),
                        merchant: row.merchant.clone().unwrap_or_default(),
                        description: row.description.clone().unwrap_or_default(),
                        decision: status.into(),
                        duplicate_id,
                        warnings: row
                            .warnings
                            .iter()
                            .map(|warning| warning.message.clone())
                            .collect(),
                    }
                })
                .collect(),
            errors: parsed
                .errors
                .iter()
                .chain(parsed.warnings.iter())
                .map(|issue| BillIssue {
                    row: issue.row,
                    message: issue.message.clone(),
                })
                .collect(),
            new_count: preview.counts.new,
            exact_duplicates: preview.counts.exact_duplicates,
            suspected_duplicates: preview.counts.suspected_duplicates,
            pending_count: preview.counts.pending,
        };
        self.preview.replace(Some(PreviewSession {
            id: preview_id,
            rows,
        }));
        Ok(result)
    }
    pub fn stage_preview(
        &self,
        preview_id: &str,
        account: &str,
        ids: Vec<String>,
        edits: Vec<PendingEdit>,
    ) -> Result<Vec<String>> {
        let session = self.preview.borrow();
        let session = session.as_ref().ok_or("预览已失效，请重新选择文件")?;
        if session.id.to_string() != preview_id {
            return Err("预览已失效，请重新选择文件".into());
        }
        if ids.is_empty()
            || ids.len() > 1000
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return Err("请选择有效候选行".into());
        }
        if edits.iter().any(|edit| !ids.contains(&edit.id))
            || edits
                .iter()
                .map(|edit| &edit.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != edits.len()
        {
            return Err("修订字段与候选行不匹配".into());
        }
        let account = self.account(account)?;
        let mut items = Vec::new();
        for id in ids {
            let (_, row, decision) = session
                .rows
                .iter()
                .find(|item| item.0 == id)
                .ok_or("候选行不存在")?;
            if matches!(
                decision,
                ledger_import::PreviewDecision::ExactDuplicate { .. }
            ) {
                return Err("精确重复行不能再次导入".into());
            }
            let amount = row.amount_minor;
            let positive = matches!(row.kind, TransactionKind::Income | TransactionKind::Refund);
            let sign = if positive == (account.kind == AccountKind::Asset) {
                1
            } else {
                -1
            };
            let mut tx = Transaction {
                id: transaction_id(&id)?,
                occurred_at: row.occurred_at,
                kind: row.kind,
                status: TransactionStatus::Pending,
                postings: vec![Posting {
                    account_id: account.id,
                    delta: money(amount * sign),
                }],
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: Some(money(amount)),
                merchant: row.merchant.clone(),
                description: row.description.clone(),
            };
            if let Some(edit) = edits.iter().find(|edit| edit.id == id) {
                tx = self.apply_pending_edit(tx, edit)?;
            }
            items.push((
                tx,
                ledger_domain::SourceEvidence {
                    source_kind: row.source_kind,
                    external_transaction_id: row.external_transaction_id.clone(),
                    import_batch_id: session.id,
                    source_fields_digest: row.source_fields_digest,
                },
            ));
        }
        ledger_app::PendingService::new(self.repo()?)
            .stage_batch(session.id, items)
            .map(|ids| ids.iter().map(|id| id.as_uuid().to_string()).collect())
            .map_err(|error| error.to_string())
    }
    fn apply_pending_edit(&self, mut tx: Transaction, edit: &PendingEdit) -> Result<Transaction> {
        let account = self.account(&edit.account_id)?;
        if edit.merchant.chars().count() > 100 || edit.description.chars().count() > 200 {
            return Err("商户最多 100 字，备注最多 200 字".into());
        }
        tx.kind = match edit.kind.as_str() {
            "income" => TransactionKind::Income,
            "expense" => TransactionKind::Expense,
            "refund" => TransactionKind::Refund,
            _ => return Err("无效的候选类型".into()),
        };
        let amount = parse_amount(&edit.amount, false)?;
        let positive = matches!(tx.kind, TransactionKind::Income | TransactionKind::Refund);
        let sign = if positive == (account.kind == AccountKind::Asset) {
            1
        } else {
            -1
        };
        let proposed_date = date(&edit.date)?;
        if tx.occurred_at.with_timezone(&Local).date_naive() != proposed_date {
            tx.occurred_at = time(proposed_date, false)?.with_timezone(&Utc);
        }
        tx.postings = vec![Posting {
            account_id: account.id,
            delta: money(amount * sign),
        }];
        tx.report_amount_cny = Some(money(amount));
        tx.merchant = (!edit.merchant.trim().is_empty()).then(|| edit.merchant.trim().into());
        tx.description =
            (!edit.description.trim().is_empty()).then(|| edit.description.trim().into());
        tx.original_transaction_id = edit
            .original_transaction_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(transaction_id)
            .transpose()?;
        tx.category_id = edit
            .category_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(|id| {
                uuid::Uuid::parse_str(id)
                    .map(ledger_domain::CategoryId::from_uuid)
                    .map_err(|_| "无效分类".to_owned())
            })
            .transpose()?;
        Ok(tx)
    }
    pub fn confirm_pending(&self, ids: Vec<String>, edits: Vec<PendingEdit>) -> Result<()> {
        let repo = self.repo()?;
        let mut domain_edits = Vec::new();
        for edit in edits {
            let id = transaction_id(&edit.id)?;
            let tx = repo
                .get_transaction(id)
                .map_err(|error| error.to_string())?
                .ok_or("流水不存在")?;
            domain_edits.push(ledger_domain::TransactionEdit {
                transaction: self.apply_pending_edit(tx, &edit)?,
                expected_version: edit.expected_version,
            });
        }
        ledger_app::PendingService::new(repo)
            .confirm(
                ids.iter()
                    .map(|id| transaction_id(id))
                    .collect::<Result<Vec<_>>>()?,
                domain_edits,
            )
            .map_err(|error| error.to_string())
    }
    pub fn edit_pending(&self, edit: PendingEdit) -> Result<()> {
        use ledger_app::FeatureRepository;
        let repo = self.repo()?;
        let tx = repo
            .get_transaction(transaction_id(&edit.id)?)
            .map_err(|error| error.to_string())?
            .ok_or("流水不存在")?;
        if tx.status != TransactionStatus::Pending {
            return Err("只能修改待确认记录".into());
        }
        repo.edit_atomic(ledger_domain::TransactionEdit {
            transaction: self.apply_pending_edit(tx, &edit)?,
            expected_version: edit.expected_version,
        })
        .map_err(|error| error.to_string())
    }
    pub fn categories(&self) -> Result<Vec<CategoryOption>> {
        use ledger_app::ClassificationRuleRepository;
        self.repo()?
            .list_categories()
            .map(|categories| {
                categories
                    .into_iter()
                    .filter(|category| category.enabled)
                    .map(|category| CategoryOption {
                        id: category.id.as_uuid().to_string(),
                        name: category.name,
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    }
}
fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Refund => "refund",
        TransactionKind::Transfer => "transfer",
        TransactionKind::BalanceAdjustment => "adjustment",
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;
    #[test]
    fn bill_preview_stage_and_pending_confirmation_use_real_sqlite_without_raw_text() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending.sqlite");
        let ledger = Ledger::open(&path).unwrap();
        let account = ledger.accounts().unwrap()[0].id.clone();
        ledger.set_reference(&account, "2026-10-01", "100").unwrap();
        let raw = "header private marker\n交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,私密原始列\n2026-10-02 08:00:00,消费,商户,午餐,支出,12.30,test-wx-1,RAW_PRIVATE_TEST_SENTINEL\ninvalid-date,消费,商户,,支出,1,test-wx-bad,RAW_PRIVATE_TEST_SENTINEL\n";
        let preview = ledger
            .bill_preview("wechat", raw.as_bytes().to_vec())
            .unwrap();
        assert_eq!(preview.rows.len(), 1);
        assert_eq!(preview.errors.len(), 1);
        let selected = vec![preview.rows[0].id.clone()];
        let ids = ledger
            .stage_preview(&preview.preview_id, &account, selected.clone(), vec![])
            .unwrap();
        assert_eq!(
            ledger
                .stage_preview(&preview.preview_id, &account, selected, vec![])
                .unwrap(),
            ids
        );
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "0.00");
        assert_eq!(
            ledger
                .day(&account, "2026-10-02")
                .unwrap()
                .balance
                .as_deref(),
            Some("100.00")
        );
        ledger.confirm_pending(ids.clone(), vec![]).unwrap();
        ledger.confirm_pending(ids, vec![]).unwrap();
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "12.30");
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "12.30");
        let preview = ledger
            .bill_preview("wechat", raw.as_bytes().to_vec())
            .unwrap();
        assert_eq!(preview.exact_duplicates, 1);
        assert!(
            ledger
                .stage_preview(
                    &preview.preview_id,
                    &account,
                    vec![preview.rows[0].id.clone()],
                    vec![]
                )
                .is_err()
        );
        let persisted = std::fs::read(&path).unwrap();
        assert!(
            !persisted
                .windows("RAW_PRIVATE_TEST_SENTINEL".len())
                .any(|window| window == b"RAW_PRIVATE_TEST_SENTINEL")
        );
    }
    #[test]
    fn preview_matches_manual_records_even_beyond_first_neighbor_page() {
        let directory = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&directory.path().join("manual-preview.sqlite")).unwrap();
        let account = ledger.accounts().unwrap()[0].id.clone();
        // Earlier same-day target sorts behind over one full page of later rows.
        ledger
            .save_entry(&account, "2026-10-02", "12.30", "manual lunch", false)
            .unwrap();
        for _ in 0..205 {
            ledger
                .save_entry(&account, "2026-10-03", "1.00", "other", false)
                .unwrap();
        }
        let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 13:00:00,消费,商户,午餐,支出,12.30,manual-later-import\n";
        let preview = ledger
            .bill_preview("wechat", raw.as_bytes().to_vec())
            .unwrap();
        assert_eq!(preview.new_count, 0);
        assert_eq!(preview.exact_duplicates, 0);
        assert_eq!(preview.suspected_duplicates, 1);
        assert!(preview.rows[0].duplicate_id.is_some());
    }

    #[test]
    fn imported_refund_requires_explicit_parent_at_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&directory.path().join("refund.sqlite")).unwrap();
        let account = ledger.accounts().unwrap()[0].id.clone();
        let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02,退款,商户,退款,收入,5,w-refund\n";
        let preview = ledger
            .bill_preview("wechat", raw.as_bytes().to_vec())
            .unwrap();
        let ids = ledger
            .stage_preview(
                &preview.preview_id,
                &account,
                vec![preview.rows[0].id.clone()],
                vec![],
            )
            .unwrap();
        ledger
            .edit_pending(PendingEdit {
                id: ids[0].clone(),
                expected_version: 0,
                account_id: account.clone(),
                date: "2026-10-02".into(),
                amount: "4.50".into(),
                kind: "refund".into(),
                merchant: "corrected merchant".into(),
                description: "corrected refund".into(),
                category_id: None,
                original_transaction_id: None,
            })
            .unwrap();
        assert!(ledger.confirm_pending(ids.clone(), vec![]).is_err());
        assert_eq!(
            ledger
                .repo()
                .unwrap()
                .get_transaction(transaction_id(&ids[0]).unwrap())
                .unwrap()
                .unwrap()
                .status,
            TransactionStatus::Pending
        );
    }
}
