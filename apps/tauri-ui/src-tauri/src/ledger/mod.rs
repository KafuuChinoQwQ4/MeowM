//! Local ledger context. Feature modules preserve the public command contracts.
mod accounts;
mod day;
mod import_stage;
mod imports;
mod parse;
mod pending;
mod query;
mod refunds;
#[cfg(test)]
mod tests;
pub use accounts::Account;
use chrono::{Local, NaiveDate, TimeZone, Utc};
pub use day::DayView;
pub use imports::BillPreview;
use imports::PreviewSession;
use ledger_app::privacy::PrivacyService;
use ledger_app::{AccountService, LedgerRepository, TransactionService};
use ledger_domain::{
    Account as DomainAccount, AccountId, AccountKind, Category, CategoryId, CurrencyCode, Money,
    Posting, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::{ExchangeRateObservation, SqliteLedgerRepository};
pub(crate) use parse::{
    account_id, date, kind_label, money, parse_amount, parse_currency_amount, time, transaction_id,
};
use parse::{cny, parse_source, source_label};
pub use pending::{CategoryOption, PendingEdit, ignore_pending, restore_pending};
pub use query::{CommandError, EntryPage, PageQuery};
use serde::Serialize;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;
pub struct Ledger {
    pub(crate) capture_session: std::cell::RefCell<Option<String>>,
    path: PathBuf,
    pub(crate) privacy: std::cell::RefCell<PrivacyService>,
    pub(crate) delete_preview: std::cell::RefCell<Option<crate::privacy_commands::DeleteSession>>,
    pub(crate) restore_preview: std::cell::RefCell<Option<crate::data_management::RestoreSession>>,
    preview: std::cell::RefCell<Option<PreviewSession>>,
}
impl Ledger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let repo = SqliteLedgerRepository::open(path).map_err(|e| e.to_string())?;
        let privacy = PrivacyService::load(&repo).map_err(|e| e.to_string())?;
        let ledger = Self {
            capture_session: std::cell::RefCell::new(None),
            path: path.into(),
            privacy: std::cell::RefCell::new(privacy),
            delete_preview: std::cell::RefCell::new(None),
            restore_preview: std::cell::RefCell::new(None),
            preview: std::cell::RefCell::new(None),
        };
        if repo.needs_default_platforms().map_err(|e| e.to_string())? {
            for name in ["微信", "支付宝", "现金"] {
                AccountService::new(ledger.repo()?)
                    .create_account(DomainAccount::new(name, AccountKind::Asset, cny()))
                    .map_err(|e| e.to_string())?;
            }
            // Seed a new ledger once; later launches must respect category deletions.
            ensure_default_categories(&ledger)?;
        }
        Ok(ledger)
    }
    pub(crate) fn clear_import_preview(&self) {
        self.preview.replace(None);
    }
    pub(crate) fn require_unlocked(&self) -> Result<()> {
        self.privacy
            .borrow_mut()
            .require_unlocked()
            .map_err(|e| e.to_string())
    }
    pub(crate) fn convert_amount(
        &self,
        amount: i64,
        currency: &CurrencyCode,
        day: NaiveDate,
        manual_rate: Option<&str>,
    ) -> Result<(Money, Option<ledger_domain::TransactionFxSnapshot>)> {
        if currency.code() == "CNY" {
            return Ok((money(amount), None));
        }
        let observation = self
            .repo()?
            .exchange_rate_at_or_before(currency.code(), day)
            .map_err(|e| e.to_string())?
            .filter(has_transaction_fx_evidence);
        let snapshot = if let Some(row) = observation {
            ledger_domain::TransactionFxSnapshot {
                quote_currency: currency.clone(),
                rate: row.rate,
                effective_date: row.effective_date,
                source: ledger_domain::FxSnapshotSource::Reference,
                provider: Some(row.provider),
                provider_sources: row.provider_sources,
            }
        } else {
            let rate = manual_rate.ok_or("该日期前无可用本地汇率，请填写手动汇率")?;
            ledger_domain::TransactionFxSnapshot {
                quote_currency: currency.clone(),
                rate: rate.into(),
                effective_date: day,
                source: ledger_domain::FxSnapshotSource::Manual,
                provider: None,
                provider_sources: vec![],
            }
        };
        let cny_amount = ledger_domain::convert_foreign_minor_to_cny_minor(
            amount,
            currency.minor_unit_exponent(),
            &snapshot.rate,
        )
        .map_err(|e| e.to_string())?;
        Ok((money(cny_amount), Some(snapshot)))
    }
    pub(crate) fn repo(&self) -> Result<SqliteLedgerRepository> {
        SqliteLedgerRepository::open(&self.path).map_err(|e| e.to_string())
    }
    pub(crate) fn database_path(&self) -> &Path {
        &self.path
    }
}

fn has_transaction_fx_evidence(observation: &ExchangeRateObservation) -> bool {
    !observation.provider.trim().is_empty()
        && !observation.provider.chars().any(char::is_control)
        && !observation.provider_sources.is_empty()
        && observation.provider_sources.len() <= 128
        && observation.provider_sources.iter().all(|source| {
            !source.is_empty()
                && source.len() <= 64
                && source
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

fn ensure_default_categories(ledger: &Ledger) -> Result<()> {
    use ledger_app::{ClassificationRuleRepository, classification::DEFAULT_CATEGORY_NAMES};
    let repo = ledger.repo()?;
    let existing = repo
        .list_categories()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|category| category.name)
        .collect::<std::collections::HashSet<_>>();
    for name in DEFAULT_CATEGORY_NAMES {
        if !existing.contains(*name) {
            repo.insert_category(Category {
                id: CategoryId::new(),
                name: (*name).into(),
                enabled: true,
            })
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
