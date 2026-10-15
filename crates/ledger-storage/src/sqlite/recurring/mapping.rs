use super::*;
pub(super) const TEMPLATE_COLUMNS: &str = "id,account_id,category_id,kind,amount_cny_minor,note,merchant,frequency,anchor_date,end_date,next_date,interval_months,enabled,edit_version";
pub(super) fn parse_date(s: String) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| to_sql_conversion_error(e.to_string()))
}
pub(super) fn template_id(s: String) -> rusqlite::Result<RecurringTemplateId> {
    parse_uuid(&s)
        .map(RecurringTemplateId::from_uuid)
        .map_err(to_sql_conversion_error)
}
pub(super) fn transaction_id(s: String) -> rusqlite::Result<TransactionId> {
    parse_uuid(&s)
        .map(TransactionId::from_uuid)
        .map_err(to_sql_conversion_error)
}
pub(super) fn cadence_str(c: RecurringCadence) -> &'static str {
    match c {
        RecurringCadence::Weekly => "weekly",
        RecurringCadence::Monthly => "monthly",
        RecurringCadence::Yearly => "yearly",
    }
}
pub(super) fn read_template(r: &rusqlite::Row<'_>) -> rusqlite::Result<RecurringTemplate> {
    let category: Option<String> = r.get(2)?;
    let end: Option<String> = r.get(9)?;
    Ok(RecurringTemplate {
        id: template_id(r.get(0)?)?,
        account_id: AccountId::from_uuid(
            parse_uuid(&r.get::<_, String>(1)?).map_err(to_sql_conversion_error)?,
        ),
        category_id: category
            .map(|s| {
                parse_uuid(&s)
                    .map(CategoryId::from_uuid)
                    .map_err(to_sql_conversion_error)
            })
            .transpose()?,
        kind: match r.get::<_, String>(3)?.as_str() {
            "income" => TransactionKind::Income,
            "expense" => TransactionKind::Expense,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        amount_cny_minor: r.get(4)?,
        description: r.get(5)?,
        merchant: r.get(6)?,
        cadence: match r.get::<_, String>(7)?.as_str() {
            "weekly" => RecurringCadence::Weekly,
            "monthly" => RecurringCadence::Monthly,
            "yearly" => RecurringCadence::Yearly,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        anchor_date: parse_date(r.get(8)?)?,
        end_date: end.map(parse_date).transpose()?,
        next_due_date: parse_date(r.get(10)?)?,
        interval: r.get(11)?,
        enabled: r.get(12)?,
        version: r.get(13)?,
    })
}
pub(super) fn read_instance(r: &rusqlite::Row<'_>) -> rusqlite::Result<RecurringInstance> {
    let id: Option<String> = r.get(2)?;
    let skipped: bool = r.get(3)?;
    let status: Option<String> = r.get(4)?;
    Ok(RecurringInstance {
        template_id: template_id(r.get(0)?)?,
        due_date: parse_date(r.get(1)?)?,
        transaction_id: id.map(transaction_id).transpose()?,
        amount_cny_minor: r.get(5)?,
        status: if skipped {
            RecurringInstanceStatus::Skipped
        } else {
            match status.as_deref() {
                Some("pending") => RecurringInstanceStatus::Pending,
                Some("confirmed") => RecurringInstanceStatus::Confirmed,
                Some("ignored") => RecurringInstanceStatus::Ignored,
                _ => return Err(rusqlite::Error::InvalidQuery),
            }
        },
    })
}
