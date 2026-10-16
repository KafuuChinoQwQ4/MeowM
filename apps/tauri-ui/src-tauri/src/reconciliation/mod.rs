//! Reconciliation IPC adapter; storage snapshots never depend on the display timezone.
pub mod commands;
#[cfg(test)]
mod tests;
mod views;
use crate::ledger::{Ledger, account_id, date, money, parse_amount, time, transaction_id};
use chrono::Utc;
use ledger_app::{RepositoryError, reconciliation::ReconciliationService};
use ledger_domain::ReconciliationId;
pub use views::{RecordView, SummaryView};
type Result<T> = std::result::Result<T, String>;
fn session_id(id: &str) -> Result<ReconciliationId> {
    uuid::Uuid::parse_str(id)
        .map(ReconciliationId::from_uuid)
        .map_err(|_| "无效的对账标识".into())
}
fn error(e: RepositoryError) -> String {
    match e {
        RepositoryError::Conflict => "会话已变化，请刷新后重试".into(),
        RepositoryError::NotFound => "对账会话或交易不存在，请刷新".into(),
        RepositoryError::InvalidData(s) => s,
        _ => "对账读写失败，请重试".into(),
    }
}
impl Ledger {
    pub fn reconciliation_list(&self, account: &str) -> Result<Vec<RecordView>> {
        ReconciliationService::new(&self.repo()?)
            .list(account_id(account)?)
            .map_err(error)
            .map(|r| r.into_iter().map(RecordView::from).collect())
    }
    pub fn reconciliation_summary(&self, id: &str) -> Result<SummaryView> {
        ReconciliationService::new(&self.repo()?)
            .summary(session_id(id)?)
            .map_err(error)
            .map(Into::into)
    }
    pub fn reconciliation_start(
        &self,
        account: &str,
        day: &str,
        actual: &str,
    ) -> Result<SummaryView> {
        ReconciliationService::new(&self.repo()?)
            .start(
                account_id(account)?,
                time(date(day)?, true)?.with_timezone(&Utc),
                money(parse_amount(actual, true)?),
            )
            .map_err(error)
            .map(Into::into)
    }
    pub fn reconciliation_review(
        &self,
        id: &str,
        entry: &str,
        reviewed: bool,
        version: u32,
    ) -> Result<()> {
        ReconciliationService::new(&self.repo()?)
            .mark_reviewed(session_id(id)?, transaction_id(entry)?, reviewed, version)
            .map_err(error)
    }
    pub fn reconciliation_adjust(&self, id: &str, version: u32, reason: &str) -> Result<String> {
        ReconciliationService::new(&self.repo()?)
            .add_adjustment(session_id(id)?, version, reason)
            .map_err(error)
            .map(|id| id.as_uuid().to_string())
    }
    pub fn reconciliation_close(&self, id: &str, version: u32) -> Result<()> {
        ReconciliationService::new(&self.repo()?)
            .close(session_id(id)?, version)
            .map_err(error)
    }
    pub fn reconciliation_reopen(&self, id: &str, version: u32) -> Result<()> {
        ReconciliationService::new(&self.repo()?)
            .reopen(session_id(id)?, version)
            .map_err(error)
    }
}
