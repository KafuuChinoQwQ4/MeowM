use super::*;

impl SqliteLedgerRepository {
    /// Atomically inserts a transaction, its postings, and optional import provenance.
    /// When evidence is present, its import batch is created in the same SQL transaction.
    pub fn insert_transaction_with_evidence(
        &self,
        transaction: Transaction,
        evidence: Option<SourceEvidence>,
    ) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        Self::write_transaction_with_evidence(&tx, &transaction, evidence.as_ref())?;
        tx.commit().map_err(storage_error)
    }

    pub(super) fn write_transaction_with_evidence(
        tx: &SqlTransaction<'_>,
        transaction: &Transaction,
        evidence: Option<&SourceEvidence>,
    ) -> Result<(), RepositoryError> {
        Self::write_transaction(tx, transaction, true)?;
        if let Some(evidence) = evidence {
            tx.execute(
                "INSERT INTO import_batches(id) VALUES (?1) ON CONFLICT(id) DO NOTHING",
                [evidence.import_batch_id.to_string()],
            )
            .map_err(storage_error)?;
            tx.execute(
                "INSERT INTO source_evidence(\
                    transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest, original_external_transaction_id\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    transaction.id.as_uuid().to_string(),
                    source_kind_str(evidence.source_kind),
                    evidence.external_transaction_id,
                    evidence.import_batch_id.to_string(),
                    evidence.source_fields_digest.as_slice(),
                    evidence.original_external_transaction_id,
                ],
            )
            .map_err(map_insert_error)?;
        }
        Ok(())
    }

    pub(super) fn write_transaction(
        tx: &SqlTransaction<'_>,
        transaction: &Transaction,
        insert: bool,
    ) -> Result<(), RepositoryError> {
        if transaction.kind == TransactionKind::Refund
            && let Some(parent) = transaction.original_transaction_id
        {
            let reversed: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM transaction_metadata WHERE reversal_of=?1)",
                    [parent.as_uuid().to_string()],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if reversed {
                return Err(RepositoryError::InvalidData(
                    "refund parent was reversed".into(),
                ));
            }
        }
        let values = transaction_values(transaction);
        let changed = if insert {
            tx.execute(
                "INSERT INTO transactions(id, occurred_at_utc_micros, kind, status, category_id, original_transaction_id, report_amount_minor, report_currency_code, report_currency_exponent, merchant, description, merchant_normalized)\
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![values.id, values.occurred_at, values.kind, values.status, values.category_id, values.original_transaction_id, values.report_amount_minor, values.report_currency_code, values.report_currency_exponent, values.merchant, values.description, transaction.merchant.as_deref().map(ledger_app::classification::normalize_merchant)],
            )
        } else {
            tx.execute(
                "UPDATE transactions SET occurred_at_utc_micros = ?2, kind = ?3, status = ?4, category_id = ?5, original_transaction_id = ?6, report_amount_minor = ?7, report_currency_code = ?8, report_currency_exponent = ?9, merchant = ?10, description = ?11, merchant_normalized = ?12 WHERE id = ?1",
                params![values.id, values.occurred_at, values.kind, values.status, values.category_id, values.original_transaction_id, values.report_amount_minor, values.report_currency_code, values.report_currency_exponent, values.merchant, values.description, transaction.merchant.as_deref().map(ledger_app::classification::normalize_merchant)],
            )
        }
        .map_err(map_insert_error)?;

        if !insert && changed == 0 {
            return Err(RepositoryError::NotFound);
        }
        tx.execute(
            "DELETE FROM postings WHERE transaction_id = ?1",
            [transaction.id.as_uuid().to_string()],
        )
        .map_err(storage_error)?;
        for (ordinal, posting) in transaction.postings.iter().enumerate() {
            tx.execute(
                "INSERT INTO postings(transaction_id, ordinal, account_id, delta_minor, currency_code, currency_exponent) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    transaction.id.as_uuid().to_string(),
                    ordinal as i64,
                    posting.account_id.as_uuid().to_string(),
                    posting.delta.minor_units(),
                    posting.delta.currency().code(),
                    posting.delta.currency().minor_unit_exponent(),
                ],
            )
            .map_err(map_insert_error)?;
        }
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,?3,?4)",params![Uuid::new_v4().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64,if insert {"created"} else {"updated"},transaction.id.as_uuid().to_string()]).map_err(storage_error)?;
        tx.execute("INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros,edit_version) VALUES(?1,?2,0) ON CONFLICT(transaction_id) DO UPDATE SET updated_at_utc_micros=excluded.updated_at_utc_micros,edit_version=edit_version+1",params![transaction.id.as_uuid().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64]).map_err(storage_error)?;
        Ok(())
    }
}
