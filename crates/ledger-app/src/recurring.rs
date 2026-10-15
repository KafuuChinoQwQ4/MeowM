//! Recurring use cases. Calendar arithmetic and input policy stay outside SQLite and UI.
mod calendar;
use crate::{RepositoryError, ports::RecurringRepository};
pub use calendar::next_occurrence;
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use ledger_domain::{
    RecurringReminder, RecurringTemplate, RecurringTemplateId, TransactionId, TransactionKind,
};
pub const GENERATION_LIMIT: usize = 200;
pub fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidData(message.into())
}
pub fn validate_template(t: &RecurringTemplate) -> Result<(), RepositoryError> {
    if !matches!(t.kind, TransactionKind::Income | TransactionKind::Expense)
        || t.amount_cny_minor <= 0
    {
        return Err(invalid("周期账单仅支持正金额的收入或支出"));
    }
    if !(1..=120).contains(&t.interval)
        || !(1900..=9998).contains(&t.anchor_date.year())
        || !(1900..=9999).contains(&t.next_due_date.year())
        || t.next_due_date < t.anchor_date
        || t.end_date
            .is_some_and(|end| end < t.anchor_date || end.year() > 9998)
    {
        return Err(invalid("周期日期或重复间隔无效"));
    }
    if t.merchant.as_ref().is_some_and(|s| s.chars().count() > 100)
        || t.description
            .as_ref()
            .is_some_and(|s| s.chars().count() > 200)
    {
        return Err(invalid("商户最多 100 字，备注最多 200 字"));
    }
    Ok(())
}
pub struct RecurringService<'a, R> {
    repository: &'a R,
}
impl<'a, R: RecurringRepository> RecurringService<'a, R> {
    pub fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    pub fn create(&self, template: RecurringTemplate) -> Result<(), RepositoryError> {
        validate_template(&template)?;
        if template.ended() || template.anchor_date != template.next_due_date {
            return Err(invalid("首次到期日必须在结束日期之前或当天"));
        }
        self.repository.save_recurring_template(template, None)
    }
    pub fn update(
        &self,
        template: RecurringTemplate,
        expected_version: u32,
    ) -> Result<(), RepositoryError> {
        validate_template(&template)?;
        self.repository
            .save_recurring_template(template, Some(expected_version))
    }
    pub fn list(&self) -> Result<Vec<RecurringTemplate>, RepositoryError> {
        self.repository.list_recurring_templates()
    }
    pub fn instances(
        &self,
        id: RecurringTemplateId,
    ) -> Result<Vec<ledger_domain::RecurringInstance>, RepositoryError> {
        self.repository.recurring_instances(id)
    }
    pub fn pause(&self, id: RecurringTemplateId, version: u32) -> Result<(), RepositoryError> {
        self.set_enabled(id, false, version)
    }
    pub fn resume(&self, id: RecurringTemplateId, version: u32) -> Result<(), RepositoryError> {
        self.set_enabled(id, true, version)
    }
    fn set_enabled(
        &self,
        id: RecurringTemplateId,
        enabled: bool,
        version: u32,
    ) -> Result<(), RepositoryError> {
        let mut t = self
            .list()?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or(RepositoryError::NotFound)?;
        t.enabled = enabled;
        self.update(t, version)
    }
    pub fn generate_due<T: TimeZone>(
        &self,
        today: NaiveDate,
        zone: &T,
    ) -> Result<Vec<TransactionId>, RepositoryError> {
        self.repository.generate_recurring_due(today, zone)
    }
    pub fn skip_instance(
        &self,
        id: RecurringTemplateId,
        due: NaiveDate,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.repository.skip_recurring_instance(id, due, version)
    }
    pub fn reminders(&self, today: NaiveDate) -> Result<Vec<RecurringReminder>, RepositoryError> {
        let until = today
            .checked_add_signed(Duration::days(7))
            .ok_or_else(|| invalid("日期超出范围"))?;
        self.repository.recurring_reminders(today, until)
    }
}
