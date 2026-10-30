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
                    transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest, original_external_transaction_id, merchant_order_id\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    transaction.id.as_uuid().to_string(),
                    source_kind_str(evidence.source_kind),
                    evidence.external_transaction_id,
                    evidence.import_batch_id.to_string(),
                    evidence.source_fields_digest.as_slice(),
                    evidence.original_external_transaction_id,
                    evidence.merchant_order_id,
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
        if transaction.fx_snapshot.is_some() {
            let mut statement = tx.prepare("SELECT id,display_name,kind,currency_code,currency_exponent,opening_balance_minor,opening_balance_effective_at_utc_micros FROM accounts").map_err(storage_error)?;
            let accounts = statement
                .query_map([], account_from_row)
                .map_err(storage_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(storage_error)?;
            ledger_domain::validate_transaction_shape(transaction, &accounts)
                .map_err(|error| RepositoryError::InvalidData(error.to_string()))?;
        }
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
        tx.execute(
            "DELETE FROM transaction_fx_snapshots WHERE transaction_id = ?1",
            [transaction.id.as_uuid().to_string()],
        )
        .map_err(storage_error)?;
        if let Some(snapshot) = &transaction.fx_snapshot {
            if snapshot
                .provider
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.chars().any(char::is_control))
                || snapshot.provider_sources.len() > 128
                || snapshot
                    .provider_sources
                    .iter()
                    .any(|value| !valid_fx_provider_source(value))
            {
                return Err(RepositoryError::InvalidData(
                    "invalid FX provider source".into(),
                ));
            }
            let source = match snapshot.source {
                ledger_domain::FxSnapshotSource::Reference => "reference",
                ledger_domain::FxSnapshotSource::Manual => "manual",
            };
            let sources = serde_json::to_string(&snapshot.provider_sources)
                .map_err(|error| RepositoryError::InvalidData(error.to_string()))?;
            tx.execute("INSERT INTO transaction_fx_snapshots(transaction_id,base_currency_code,quote_currency_code,rate,effective_date,source_kind,provider,provider_sources) VALUES(?1,'CNY',?2,?3,?4,?5,?6,?7)",params![transaction.id.as_uuid().to_string(),snapshot.quote_currency.code(),snapshot.rate,snapshot.effective_date.format("%Y-%m-%d").to_string(),source,snapshot.provider,sources]).map_err(map_insert_error)?;
        }
        tx.execute("INSERT INTO audit_events(id,occurred_at_utc_micros,action,transaction_id) VALUES(?1,?2,?3,?4)",params![Uuid::new_v4().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64,if insert {"created"} else {"updated"},transaction.id.as_uuid().to_string()]).map_err(storage_error)?;
        tx.execute("INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros,edit_version) VALUES(?1,?2,0) ON CONFLICT(transaction_id) DO UPDATE SET updated_at_utc_micros=excluded.updated_at_utc_micros,edit_version=edit_version+1",params![transaction.id.as_uuid().to_string(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64]).map_err(storage_error)?;
        Ok(())
    }
}

pub(super) fn valid_fx_provider_source(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}
