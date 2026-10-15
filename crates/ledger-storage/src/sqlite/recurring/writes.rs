use super::*;
impl SqliteLedgerRepository {
    pub(super) fn write_recurring(
        &self,
        mut t: RecurringTemplate,
        expected: Option<u32>,
    ) -> Result<(), RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let old = self
            .list_recurring_templates()?
            .into_iter()
            .find(|old| old.id == t.id);
        match (&old, expected) {
            (None, None) => {
                t.version = 0;
                if t.anchor_date != t.next_due_date || t.ended() {
                    return Err(invalid("首次到期日或结束日期无效"));
                }
            }
            (Some(old), Some(version)) if old.version == version => {
                t.version = version
                    .checked_add(1)
                    .ok_or_else(|| invalid("版本超出范围"))?;
                if old.next_due_date != t.next_due_date
                    || old.cadence != t.cadence
                    || old.interval != t.interval
                {
                    let last:Option<String>=tx.query_row("SELECT max(scheduled_date) FROM recurring_instances WHERE template_id=?1",[t.id.as_uuid().to_string()],|r|r.get(0)).map_err(storage_error)?;
                    if last.is_some_and(|date| t.next_due_date.to_string() <= date) {
                        return Err(invalid("新的到期日必须晚于已经生成或跳过的实例"));
                    }
                    t.anchor_date = t.next_due_date;
                } else {
                    t.anchor_date = old.anchor_date;
                }
            }
            (None, Some(_)) => return Err(RepositoryError::NotFound),
            _ => return Err(RepositoryError::Conflict),
        }
        validate_template(&t)?;
        let account = self
            .get_account(t.account_id)?
            .ok_or(RepositoryError::NotFound)?;
        if account.currency.code() != "CNY" || account.currency.minor_unit_exponent() != 2 {
            return Err(invalid("请选择人民币平台"));
        }
        if let Some(id) = t.category_id {
            let category = self.get_category(id)?.ok_or(RepositoryError::NotFound)?;
            if !category.enabled && old.as_ref().is_none_or(|old| old.category_id != Some(id)) {
                return Err(invalid("不能选择已停用的分类"));
            }
        }
        let sql = if old.is_some() {
            "UPDATE recurring_templates SET account_id=?2,category_id=?3,kind=?4,amount_cny_minor=?5,note=?6,merchant=?7,frequency=?8,anchor_date=?9,end_date=?10,next_date=?11,interval_months=?12,enabled=?13,edit_version=?14 WHERE id=?1"
        } else {
            "INSERT INTO recurring_templates(id,account_id,category_id,kind,amount_cny_minor,note,merchant,frequency,anchor_date,end_date,next_date,interval_months,enabled,edit_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"
        };
        tx.execute(
            sql,
            params![
                t.id.as_uuid().to_string(),
                t.account_id.as_uuid().to_string(),
                t.category_id.map(|id| id.as_uuid().to_string()),
                transaction_kind_str(t.kind),
                t.amount_cny_minor,
                t.description,
                t.merchant,
                cadence_str(t.cadence),
                t.anchor_date.to_string(),
                t.end_date.map(|d| d.to_string()),
                t.next_due_date.to_string(),
                t.interval,
                t.enabled,
                t.version
            ],
        )
        .map_err(map_insert_error)?;
        tx.commit().map_err(storage_error)
    }
    pub(super) fn skip_recurring(
        &self,
        id: RecurringTemplateId,
        due: NaiveDate,
        version: u32,
    ) -> Result<(), RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let skipped:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurring_instances WHERE template_id=?1 AND scheduled_date=?2 AND skipped=1)",params![id.as_uuid().to_string(),due.to_string()],|r|r.get(0)).map_err(storage_error)?;
        if skipped {
            return Ok(());
        }
        let t = self
            .list_recurring_templates()?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if t.version != version || t.next_due_date != due {
            return Err(RepositoryError::Conflict);
        }
        if !t.enabled || t.ended() {
            return Err(invalid("只能跳过启用模板的下一次到期"));
        }
        let next = next_occurrence(t.cadence, t.interval, t.anchor_date, due)?;
        tx.execute("INSERT INTO recurring_instances(template_id,scheduled_date,transaction_id,skipped) VALUES(?1,?2,NULL,1)",params![id.as_uuid().to_string(),due.to_string()]).map_err(map_insert_error)?;
        tx.execute(
            "UPDATE recurring_templates SET next_date=?2,edit_version=edit_version+1 WHERE id=?1",
            params![id.as_uuid().to_string(), next.to_string()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}
