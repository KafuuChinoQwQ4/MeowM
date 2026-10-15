//! Templates, occurrence receipts and candidates use one immediate SQLite transaction.
mod generation;
mod mapping;
mod writes;
use super::*;
use chrono::{NaiveDate, TimeZone};
use ledger_app::{
    ports::RecurringRepository,
    recurring::{GENERATION_LIMIT, invalid, next_occurrence, validate_template},
};
use ledger_domain::{
    RecurringCadence, RecurringInstance, RecurringInstanceStatus, RecurringReminder,
    RecurringTemplate, RecurringTemplateId,
};
use mapping::*;
impl RecurringRepository for SqliteLedgerRepository {
    fn list_recurring_templates(&self) -> Result<Vec<RecurringTemplate>, RepositoryError> {
        let mut stmt = self
            .connection
            .prepare(&format!(
                "SELECT {TEMPLATE_COLUMNS} FROM recurring_templates ORDER BY next_date,id"
            ))
            .map_err(storage_error)?;
        stmt.query_map([], read_template)
            .map_err(storage_error)?
            .collect::<Result<_, _>>()
            .map_err(storage_error)
    }
    fn save_recurring_template(
        &self,
        template: RecurringTemplate,
        expected_version: Option<u32>,
    ) -> Result<(), RepositoryError> {
        self.write_recurring(template, expected_version)
    }
    fn recurring_instances(
        &self,
        id: RecurringTemplateId,
    ) -> Result<Vec<RecurringInstance>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT i.template_id,i.scheduled_date,i.transaction_id,i.skipped,t.status,t.report_amount_minor FROM recurring_instances i LEFT JOIN transactions t ON t.id=i.transaction_id WHERE i.template_id=?1 ORDER BY i.scheduled_date DESC LIMIT 200").map_err(storage_error)?;
        let mut rows = stmt
            .query_map([id.as_uuid().to_string()], read_instance)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        rows.reverse();
        Ok(rows)
    }
    fn generate_recurring_due<T: TimeZone>(
        &self,
        today: NaiveDate,
        zone: &T,
    ) -> Result<Vec<TransactionId>, RepositoryError> {
        self.generate_recurring(today, zone)
    }
    fn skip_recurring_instance(
        &self,
        id: RecurringTemplateId,
        due: NaiveDate,
        version: u32,
    ) -> Result<(), RepositoryError> {
        self.skip_recurring(id, due, version)
    }
    fn recurring_reminders(
        &self,
        today: NaiveDate,
        until: NaiveDate,
    ) -> Result<Vec<RecurringReminder>, RepositoryError> {
        let snapshot = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        // Pending reminders use the candidate snapshot, never a subsequently edited template amount.
        let mut stmt=self.connection.prepare("SELECT i.template_id,i.scheduled_date,i.transaction_id,t.report_amount_minor,t.merchant,t.description,p.account_id FROM recurring_instances i JOIN transactions t ON t.id=i.transaction_id JOIN postings p ON p.transaction_id=t.id AND p.ordinal=0 WHERE t.status='pending' AND i.scheduled_date<=?1 ORDER BY i.scheduled_date,i.template_id LIMIT 200").map_err(storage_error)?;
        let mut result = stmt
            .query_map([until.to_string()], |r| {
                let due = parse_date(r.get(1)?)?;
                Ok(RecurringReminder {
                    template_id: template_id(r.get(0)?)?,
                    due_date: due,
                    transaction_id: Some(transaction_id(r.get(2)?)?),
                    amount_cny_minor: r.get(3)?,
                    merchant: r.get(4)?,
                    description: r.get(5)?,
                    account_id: AccountId::from_uuid(
                        parse_uuid(&r.get::<_, String>(6)?).map_err(to_sql_conversion_error)?,
                    ),
                    overdue: due < today,
                })
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        for t in self.list_recurring_templates()? {
            if t.enabled && !t.ended() && t.next_due_date <= until && result.len() < 200 {
                result.push(RecurringReminder {
                    template_id: t.id,
                    due_date: t.next_due_date,
                    transaction_id: None,
                    account_id: t.account_id,
                    amount_cny_minor: t.amount_cny_minor,
                    merchant: t.merchant,
                    description: t.description,
                    overdue: t.next_due_date < today,
                });
            }
        }
        result.sort_by_key(|r| (r.due_date, *r.template_id.as_uuid()));
        drop(stmt);
        snapshot.commit().map_err(storage_error)?;
        Ok(result)
    }
}
