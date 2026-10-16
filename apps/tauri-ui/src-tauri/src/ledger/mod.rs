//! Local ledger context. Feature modules preserve the public command contracts.
mod accounts;
mod day;
mod import_stage;
mod imports;
mod parse;
mod pending;
mod query;
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
    Account as DomainAccount, AccountId, AccountKind, CurrencyCode, Money, Posting, Transaction,
    TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
pub(crate) use parse::{account_id, date, kind_label, money, parse_amount, time, transaction_id};
use parse::{cny, parse_source, source_label};
pub use pending::{CategoryOption, PendingEdit, ignore_pending, restore_pending};
pub use query::{CommandError, EntryPage, PageQuery};
use serde::Serialize;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;
pub struct Ledger {
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
            path: path.into(),
            privacy: std::cell::RefCell::new(privacy),
            delete_preview: std::cell::RefCell::new(None),
            restore_preview: std::cell::RefCell::new(None),
            preview: std::cell::RefCell::new(None),
        };
        if repo.list_accounts().map_err(|e| e.to_string())?.is_empty() {
            for name in ["微信", "支付宝", "现金"] {
                AccountService::new(ledger.repo()?)
                    .create_account(DomainAccount::new(name, AccountKind::Asset, cny()))
                    .map_err(|e| e.to_string())?;
            }
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
    pub(crate) fn repo(&self) -> Result<SqliteLedgerRepository> {
        SqliteLedgerRepository::open(&self.path).map_err(|e| e.to_string())
    }
}
