//! Recurring ledger IPC boundary. Calendar and persistence stay in core crates.
pub mod commands;
mod input;
#[cfg(test)]
mod tests;
mod views;
use crate::ledger::{Ledger, account_id, date, kind_label, money, parse_amount};
use chrono::{Local, NaiveDate, TimeZone};
pub use input::RecurringInput;
use ledger_app::{ClassificationRuleRepository, RepositoryError, recurring::RecurringService};
use ledger_domain::*;
use serde::Serialize;
pub use views::{InstanceView, RecurringState};
type Result<T> = std::result::Result<T, String>;
fn template_id(id: &str) -> Result<RecurringTemplateId> {
    uuid::Uuid::parse_str(id)
        .map(RecurringTemplateId::from_uuid)
        .map_err(|_| "无效的周期账单标识".into())
}
fn recurring_error(error: RepositoryError) -> String {
    match error {
        RepositoryError::Conflict => "记录已变化，请刷新后重试".into(),
        RepositoryError::NotFound => "周期账单不存在，请刷新后重试".into(),
        RepositoryError::InvalidData(message) => message,
        _ => "周期账单读写失败，请重试".into(),
    }
}
impl Ledger {
    pub fn recurring_state(&self) -> Result<RecurringState> {
        self.recurring_state_at(Local::now().date_naive(), &Local)
    }
    pub(crate) fn recurring_state_at<T: TimeZone>(
        &self,
        today: NaiveDate,
        zone: &T,
    ) -> Result<RecurringState> {
        let repo = self.repo()?;
        let service = RecurringService::new(&repo);
        let generated_count = service
            .generate_due(today, zone)
            .map_err(recurring_error)?
            .len();
        let templates = service.list().map_err(recurring_error)?;
        let has_more = templates
            .iter()
            .any(|t| t.enabled && !t.ended() && t.next_due_date <= today);
        let names = repo.list_categories().map_err(recurring_error)?;
        let reminders = service
            .reminders(today)
            .map_err(recurring_error)?
            .into_iter()
            .map(views::ReminderView::from)
            .collect();
        Ok(RecurringState {
            templates: templates
                .into_iter()
                .map(|t| views::TemplateView::new(t, &names))
                .collect(),
            reminders,
            generated_count,
            has_more,
        })
    }
    pub fn set_recurring_enabled(&self, id: &str, version: u32, enabled: bool) -> Result<()> {
        let repo = self.repo()?;
        let service = RecurringService::new(&repo);
        let id = template_id(id)?;
        if enabled {
            service.resume(id, version)
        } else {
            service.pause(id, version)
        }
        .map_err(recurring_error)
    }
    pub fn skip_recurring(&self, id: &str, due_date: &str, version: u32) -> Result<()> {
        RecurringService::new(&self.repo()?)
            .skip_instance(template_id(id)?, date(due_date)?, version)
            .map_err(recurring_error)
    }
    pub fn recurring_instance_views(&self, id: &str) -> Result<Vec<InstanceView>> {
        RecurringService::new(&self.repo()?)
            .instances(template_id(id)?)
            .map_err(recurring_error)
            .map(|rows| rows.into_iter().map(InstanceView::from).collect())
    }
}
