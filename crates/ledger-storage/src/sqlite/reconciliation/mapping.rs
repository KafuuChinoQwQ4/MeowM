use super::*;
pub(super) const RECORD_SELECT: &str = "SELECT id,account_id,cutoff_utc_micros,expected_minor,actual_minor,adjustment_id,status,version,created_at_utc_micros,reference_minor,reference_at_utc_micros,currency_code,currency_exponent,period_from_utc_micros,snapshot_ready FROM reconciliation_sessions";
pub(super) fn record_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ReconciliationRecord> {
    let id: String = r.get(0)?;
    let account: String = r.get(1)?;
    let adjustment: Option<String> = r.get(5)?;
    let status: String = r.get(6)?;
    let status = match status.as_str() {
        "open" => ReconciliationStatus::Open,
        "reviewed" => ReconciliationStatus::Reviewed,
        "closed" => ReconciliationStatus::Closed,
        _ => {
            return Err(to_sql_conversion_error(
                "invalid reconciliation state".into(),
            ));
        }
    };
    let code: String = r.get(11)?;
    Ok(ReconciliationRecord {
        session: ReconciliationSession {
            id: ReconciliationId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
            account_id: AccountId::from_uuid(
                parse_uuid(&account).map_err(to_sql_conversion_error)?,
            ),
            cutoff_utc: parse_timestamp(r.get(2)?).map_err(to_sql_conversion_error)?,
            expected_minor: r.get(3)?,
            actual_minor: r.get(4)?,
            adjustment_id: adjustment
                .map(|id| parse_uuid(&id).map(TransactionId::from_uuid))
                .transpose()
                .map_err(to_sql_conversion_error)?,
        },
        status,
        version: r.get(7)?,
        created_at: parse_timestamp(r.get(8)?).map_err(to_sql_conversion_error)?,
        reference: Money::new(
            r.get(9)?,
            CurrencyCode::new(&code, r.get(12)?)
                .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        ),
        reference_at: parse_timestamp(r.get(10)?).map_err(to_sql_conversion_error)?,
        period_from: r
            .get::<_, Option<i64>>(13)?
            .map(parse_timestamp)
            .transpose()
            .map_err(to_sql_conversion_error)?,
        snapshot_ready: r.get(14)?,
    })
}
pub(super) fn entry_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ReconciliationEntry> {
    let id: String = r.get(0)?;
    let kind: String = r.get(2)?;
    let status: String = r.get(3)?;
    Ok(ReconciliationEntry {
        transaction_id: TransactionId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        occurred_at: parse_timestamp(r.get(1)?).map_err(to_sql_conversion_error)?,
        kind: parse_transaction_kind(&kind).map_err(to_sql_conversion_error)?,
        status: parse_transaction_status(&status).map_err(to_sql_conversion_error)?,
        delta_minor: r.get(4)?,
        merchant: r.get(5)?,
        description: r.get(6)?,
        reviewed: r.get(7)?,
    })
}
