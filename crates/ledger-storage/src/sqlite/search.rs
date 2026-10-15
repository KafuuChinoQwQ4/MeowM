use super::*;
use ledger_app::classification::normalize_merchant;

impl SqliteLedgerRepository {
    pub(super) fn search_transactions(
        &self,
        q: ledger_domain::PendingQuery,
    ) -> Result<ledger_domain::TransactionPage, RepositoryError> {
        if !(1..=200).contains(&q.limit)
            || q.from_utc_inclusive
                .zip(q.until_utc_exclusive)
                .is_some_and(|(a, b)| a >= b)
            || q.min_amount_cny_minor.is_some_and(|v| v < 0)
            || q.max_amount_cny_minor.is_some_and(|v| v < 0)
            || q.min_amount_cny_minor
                .zip(q.max_amount_cny_minor)
                .is_some_and(|(a, b)| a > b)
        {
            return Err(RepositoryError::InvalidData(
                "invalid page, amount or date range".into(),
            ));
        }
        let merchant = q
            .merchant
            .as_deref()
            .map(normalize_merchant)
            .filter(|v| !v.is_empty());
        let keyword = q
            .keyword
            .as_deref()
            .map(normalize_merchant)
            .filter(|v| !v.is_empty());
        let sql = "FROM transactions t LEFT JOIN merchant_aliases a ON a.alias_normalized=t.merchant_normalized \
            WHERE (?1 IS NULL OR t.status=?1) AND (?2 IS NULL OR t.occurred_at_utc_micros>=?2) \
            AND (?3 IS NULL OR t.occurred_at_utc_micros<?3) \
            AND (?4 IS NULL OR EXISTS(SELECT 1 FROM postings p WHERE p.transaction_id=t.id AND p.account_id=?4)) \
            AND (?5 IS NULL OR t.category_id=?5) \
            AND (?6 IS NULL OR instr(coalesce(a.canonical_normalized,t.merchant_normalized,''),?6)>0 OR instr(coalesce(t.merchant_normalized,''),?6)>0) \
            AND (?7 IS NULL OR instr(coalesce(a.canonical_normalized,t.merchant_normalized,''),?7)>0 \
             OR instr(coalesce(t.merchant_normalized,''),?7)>0 OR instr(lower(coalesce(t.description,'')),?7)>0 \
             OR EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND instr(lower(coalesce(e.external_transaction_id,'')),?7)>0)) \
            AND (?8 IS NULL OR EXISTS(SELECT 1 FROM source_evidence e WHERE e.transaction_id=t.id AND e.source_kind=?8)) \
            AND (?9 IS NULL OR t.report_amount_minor>=?9) AND (?10 IS NULL OR t.report_amount_minor<=?10)";
        let args = rusqlite::params![
            q.status.map(transaction_status_str),
            q.from_utc_inclusive.map(|v| v.timestamp_micros()),
            q.until_utc_exclusive.map(|v| v.timestamp_micros()),
            q.account_id.map(|v| v.as_uuid().to_string()),
            q.category_id.map(|v| v.as_uuid().to_string()),
            merchant,
            keyword,
            q.source_kind.map(source_kind_str),
            q.min_amount_cny_minor,
            q.max_amount_cny_minor
        ];
        let snapshot = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let total = self
            .connection
            .query_row(&format!("SELECT count(*) {sql}"), args, |r| r.get(0))
            .map_err(storage_error)?;
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT t.id {sql} ORDER BY t.occurred_at_utc_micros DESC,t.id LIMIT ?11 OFFSET ?12"
            ))
            .map_err(storage_error)?;
        let ids = statement
            .query_map(
                rusqlite::params![
                    q.status.map(transaction_status_str),
                    q.from_utc_inclusive.map(|v| v.timestamp_micros()),
                    q.until_utc_exclusive.map(|v| v.timestamp_micros()),
                    q.account_id.map(|v| v.as_uuid().to_string()),
                    q.category_id.map(|v| v.as_uuid().to_string()),
                    merchant,
                    keyword,
                    q.source_kind.map(source_kind_str),
                    q.min_amount_cny_minor,
                    q.max_amount_cny_minor,
                    q.limit,
                    q.offset
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let items = ids
            .into_iter()
            .map(|value| {
                let id = TransactionId::from_uuid(
                    Uuid::parse_str(&value)
                        .map_err(|_| RepositoryError::InvalidData("invalid ID".into()))?,
                );
                Ok(ledger_domain::PendingCandidate {
                    transaction: self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?,
                    metadata: self.metadata(id)?,
                })
            })
            .collect::<Result<_, RepositoryError>>()?;
        drop(statement);
        snapshot.commit().map_err(storage_error)?;
        Ok(ledger_domain::TransactionPage {
            items,
            total,
            offset: q.offset,
            limit: q.limit,
        })
    }
}
