use super::*;

impl SqliteLedgerRepository {
    /// Inserts an import batch that may be referenced by source evidence.
    pub fn create_import_batch(&self, id: Uuid) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO import_batches(id) VALUES (?1)",
                [id.to_string()],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    /// Attaches provenance metadata without retaining notification or bill text.
    pub fn insert_source_evidence(
        &self,
        transaction_id: TransactionId,
        evidence: SourceEvidence,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO source_evidence(\
                    transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    transaction_id.as_uuid().to_string(),
                    source_kind_str(evidence.source_kind),
                    evidence.external_transaction_id,
                    evidence.import_batch_id.to_string(),
                    evidence.source_fields_digest.as_slice(),
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }
}
impl ledger_app::ImportRepository for SqliteLedgerRepository {
    fn find_source_transaction(
        &self,
        source: SourceKind,
        external_id: &str,
    ) -> Result<Option<TransactionId>, RepositoryError> {
        let id: Option<String> = self.connection.query_row(
            "SELECT transaction_id FROM source_evidence WHERE source_kind=?1 AND external_transaction_id=?2",
            params![source_kind_str(source), external_id], |row| row.get(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| {
            Uuid::parse_str(&id)
                .map(TransactionId::from_uuid)
                .map_err(|_| RepositoryError::InvalidData("invalid transaction ID".into()))
        })
        .transpose()
    }

    fn insert_pending_batch(
        &self,
        batch_id: Uuid,
        items: Vec<(Transaction, SourceEvidence)>,
    ) -> Result<Vec<TransactionId>, RepositoryError> {
        if items.is_empty() || items.len() > 10_000 {
            return Err(RepositoryError::InvalidData(
                "import batch must contain 1..10000 candidates".into(),
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let prior: Vec<(String, Vec<u8>)> = {
            let mut stmt = tx.prepare("SELECT transaction_id,source_fields_digest FROM import_receipt_rows WHERE batch_id=?1 ORDER BY ordinal").map_err(storage_error)?;
            let rows = stmt
                .query_map([batch_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(storage_error)?;
            rows.collect::<Result<_, _>>().map_err(storage_error)?
        };
        if !prior.is_empty() {
            if prior.len() != items.len()
                || prior
                    .iter()
                    .zip(&items)
                    .any(|((_, digest), (_, evidence))| {
                        digest.as_slice() != evidence.source_fields_digest
                    })
            {
                return Err(RepositoryError::Conflict);
            }
            return prior
                .into_iter()
                .map(|(id, _)| {
                    Uuid::parse_str(&id)
                        .map(TransactionId::from_uuid)
                        .map_err(|_| RepositoryError::InvalidData("invalid receipt ID".into()))
                })
                .collect();
        }
        tx.execute(
            "INSERT INTO import_batches(id) VALUES(?1)",
            [batch_id.to_string()],
        )
        .map_err(map_insert_error)?;
        let mut receipt = Vec::with_capacity(items.len());
        for (ordinal, (transaction, evidence)) in items.iter().enumerate() {
            if transaction.status != TransactionStatus::Pending
                || evidence.import_batch_id != batch_id
                || evidence
                    .external_transaction_id
                    .as_deref()
                    .is_some_and(|id| id.trim().is_empty())
            {
                return Err(RepositoryError::InvalidData(
                    "invalid pending import candidate".into(),
                ));
            }
            self.validate_feature_transaction(transaction)?;
            let existing = evidence
                .external_transaction_id
                .as_deref()
                .map(|external_id| self.find_source_transaction(evidence.source_kind, external_id))
                .transpose()?
                .flatten();
            let id = if let Some(id) = existing {
                id
            } else {
                Self::write_transaction_with_evidence(&tx, transaction, Some(evidence))?;
                transaction.id
            };
            tx.execute("INSERT INTO import_receipt_rows(batch_id,ordinal,transaction_id,source_fields_digest) VALUES(?1,?2,?3,?4)", params![batch_id.to_string(), ordinal as i64, id.as_uuid().to_string(), evidence.source_fields_digest.as_slice()]).map_err(map_insert_error)?;
            receipt.push(id);
        }
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}
