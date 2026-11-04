use super::*;
use ledger_domain::crypto::{
    CryptoRecord, CryptoReviewStatus, ReviewedCryptoRecord, valid_spot_instrument,
};
use std::collections::HashSet;

fn invalid_crypto() -> RepositoryError {
    RepositoryError::InvalidData("invalid_crypto_record".into())
}

fn parse_record(
    key: String,
    payload: String,
    status: String,
    occurred_at: i64,
) -> Result<ReviewedCryptoRecord, RepositoryError> {
    let record: CryptoRecord = serde_json::from_str(&payload).map_err(|_| invalid_crypto())?;
    let status = match status.as_str() {
        "pending" => CryptoReviewStatus::Pending,
        "confirmed" => CryptoReviewStatus::Confirmed,
        "ignored" => CryptoReviewStatus::Ignored,
        _ => return Err(invalid_crypto()),
    };
    if !record.validate()
        || record.key != key
        || record.occurred_at != occurred_at
        || (status == CryptoReviewStatus::Confirmed && !record.confirmable())
    {
        return Err(invalid_crypto());
    }
    Ok(ReviewedCryptoRecord { record, status })
}

impl SqliteLedgerRepository {
    pub(super) fn apply_crypto_migration(&self) -> Result<(), RepositoryError> {
        if self.schema_version()? >= 15 {
            return Ok(());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute_batch(include_str!("../../migrations/0015_crypto.sql"))
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO schema_version VALUES(15,?1)",
            [Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    pub fn crypto_watchlist(&self) -> Result<Vec<String>, RepositoryError> {
        let rows: Vec<String> = self
            .connection
            .prepare("SELECT instrument FROM crypto_watchlist ORDER BY ordinal, instrument")
            .map_err(storage_error)?
            .query_map([], |r| r.get(0))
            .map_err(storage_error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(storage_error)?;
        if rows.len() > 32 || rows.iter().any(|r| !valid_spot_instrument(r)) {
            return Err(invalid_crypto());
        }
        Ok(rows)
    }

    pub fn save_crypto_watchlist(&self, instruments: &[String]) -> Result<(), RepositoryError> {
        if instruments.len() > 32
            || instruments.iter().any(|r| !valid_spot_instrument(r))
            || instruments.iter().collect::<HashSet<_>>().len() != instruments.len()
        {
            return Err(invalid_crypto());
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        tx.execute("DELETE FROM crypto_watchlist", [])
            .map_err(storage_error)?;
        for (ordinal, instrument) in instruments.iter().enumerate() {
            tx.execute(
                "INSERT INTO crypto_watchlist VALUES(?1,?2)",
                params![instrument, ordinal as i64],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }

    /// Atomic staging; a repeated read may refresh upstream status, never overwrite reviewed evidence.
    pub fn stage_crypto_records(&self, records: &[CryptoRecord]) -> Result<u64, RepositoryError> {
        if records.len() > 100 || records.iter().any(|r| !r.validate()) {
            return Err(invalid_crypto());
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let mut inserted = 0;
        for record in records {
            let payload = serde_json::to_string(record).map_err(|_| invalid_crypto())?;
            inserted += tx
                .execute(
                    "INSERT OR IGNORE INTO crypto_records VALUES(?1,?2,'pending',?3,?4)",
                    params![
                        record.key,
                        payload,
                        record.occurred_at,
                        Utc::now().timestamp_micros()
                    ],
                )
                .map_err(storage_error)? as u64;
            tx.execute("UPDATE crypto_records SET payload=?2,occurred_at=?3 WHERE record_key=?1 AND review_status!='confirmed'",
                params![record.key, payload, record.occurred_at]).map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        Ok(inserted)
    }

    pub fn crypto_records(
        &self,
        status: Option<CryptoReviewStatus>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<ReviewedCryptoRecord>, u64), RepositoryError> {
        if limit == 0 || limit > 100 {
            return Err(invalid_crypto());
        }
        let status = status.map(CryptoReviewStatus::as_str);
        let total: u64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM crypto_records WHERE (?1 IS NULL OR review_status=?1)",
                [status],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let rows = self.connection.prepare("SELECT record_key,payload,review_status,occurred_at FROM crypto_records WHERE (?1 IS NULL OR review_status=?1) ORDER BY occurred_at DESC,record_key LIMIT ?2 OFFSET ?3")
            .map_err(storage_error)?.query_map(params![status,limit,offset], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))
            .map_err(storage_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(storage_error)?;
        let records = rows
            .into_iter()
            .map(|(k, p, s, t)| parse_record(k, p, s, t))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((records, total))
    }

    pub fn review_crypto_records(
        &self,
        keys: &[String],
        status: CryptoReviewStatus,
    ) -> Result<(), RepositoryError> {
        if keys.is_empty() || keys.len() > 100 {
            return Err(invalid_crypto());
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        for key in keys {
            let (payload, old, time) = tx.query_row("SELECT payload,review_status,occurred_at FROM crypto_records WHERE record_key=?1", [key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
                .optional().map_err(storage_error)?.ok_or_else(invalid_crypto)?;
            let row = parse_record(key.clone(), payload, old, time)?;
            if status == CryptoReviewStatus::Confirmed && !row.record.confirmable() {
                return Err(invalid_crypto());
            }
            tx.execute(
                "UPDATE crypto_records SET review_status=?2 WHERE record_key=?1",
                params![key, status.as_str()],
            )
            .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)
    }

    pub(super) fn validate_crypto_snapshot(&self) -> Result<(), RepositoryError> {
        self.crypto_watchlist()?;
        let mut query = self
            .connection
            .prepare("SELECT record_key,payload,review_status,occurred_at FROM crypto_records")
            .map_err(storage_error)?;
        let mut rows = query.query([]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            parse_record(
                row.get(0).map_err(storage_error)?,
                row.get(1).map_err(storage_error)?,
                row.get(2).map_err(storage_error)?,
                row.get(3).map_err(storage_error)?,
            )?;
        }
        Ok(())
    }
}
