use super::*;
// Text fields are protected from spreadsheet formula execution, including after whitespace.
fn cell(value: &str, text: bool) -> String {
    let protect = text && value.trim_start().starts_with(['=', '+', '-', '@']);
    format!(
        "\"{}{}\"",
        if protect { "'" } else { "" },
        value.replace('"', "\"\"")
    )
}
pub(super) fn encode(rows: &[ExportRow], bom: bool) -> Result<Vec<u8>, RepositoryError> {
    let mut out = if bom {
        "\u{feff}".to_string()
    } else {
        String::new()
    };
    out.push_str("id,occurred_at,kind,status,amount_cny,income_cny,expense_cny,merchant,description,category_id,original_transaction_id,reversal_of,postings_json,sources_json\r\n");
    for row in rows {
        let postings = serde_json::to_string(&row.postings)
            .map_err(|_| RepositoryError::Storage("export encoding failed".into()))?;
        let sources = serde_json::to_string(&row.sources)
            .map_err(|_| RepositoryError::Storage("export encoding failed".into()))?;
        let fields = [
            row.id.as_str(),
            &row.occurred_at,
            &row.kind,
            &row.status,
            row.amount_cny.as_deref().unwrap_or(""),
            &row.income_cny,
            &row.expense_cny,
            row.merchant.as_deref().unwrap_or(""),
            row.description.as_deref().unwrap_or(""),
            row.category_id.as_deref().unwrap_or(""),
            row.original_transaction_id.as_deref().unwrap_or(""),
            row.reversal_of.as_deref().unwrap_or(""),
            &postings,
            &sources,
        ];
        out.push_str(
            &fields
                .iter()
                .enumerate()
                .map(|(i, s)| cell(s, !matches!(i, 4..=6)))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    Ok(out.into_bytes())
}
