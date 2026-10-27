use super::*;
use sha2::{Digest, Sha256};
impl SqliteLedgerRepository {
    pub(super) fn generate_recurring<T: TimeZone>(
        &self,
        today: NaiveDate,
        zone: &T,
    ) -> Result<Vec<TransactionId>, RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let mut generated = Vec::new();
        let mut processed = 0;
        for mut t in self.list_recurring_templates()? {
            if !t.enabled || t.ended() || t.next_due_date > today {
                continue;
            }
            validate_template(&t)?;
            let account = self
                .get_account(t.account_id)?
                .ok_or(RepositoryError::NotFound)?;
            if account.currency.code() != "CNY" || account.currency.minor_unit_exponent() != 2 {
                return Err(invalid("周期账单需要人民币平台"));
            }
            let initial = t.next_due_date;
            while t.next_due_date <= today && !t.ended() && processed < GENERATION_LIMIT {
                let due = t.next_due_date;
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurring_instances WHERE template_id=?1 AND scheduled_date=?2)",params![t.id.as_uuid().to_string(),due.to_string()],|r|r.get(0)).map_err(storage_error)?;
                if !exists {
                    let occurred_at = ledger_app::calendar::local_midnight(due, zone)
                        .map_err(|e| invalid(&e.to_string()))?;
                    let positive = t.kind == TransactionKind::Income;
                    let delta = if positive == (account.kind == AccountKind::Asset) {
                        t.amount_cny_minor
                    } else {
                        -t.amount_cny_minor
                    };
                    let candidate = Transaction {
                        id: TransactionId::new(),
                        occurred_at,
                        kind: t.kind,
                        status: TransactionStatus::Pending,
                        postings: vec![Posting {
                            account_id: t.account_id,
                            delta: Money::new(delta, account.currency.clone()),
                        }],
                        category_id: t.category_id,
                        original_transaction_id: None,
                        report_amount_cny: Some(Money::new(
                            t.amount_cny_minor,
                            account.currency.clone(),
                        )),
                        merchant: t.merchant.clone(),
                        description: t.description.clone(),
                    };
                    let identity = format!("{}:{due}", t.id.as_uuid());
                    let digest = Sha256::digest(format!(
                        "{identity}:{}:{}",
                        t.amount_cny_minor,
                        transaction_kind_str(t.kind)
                    ))
                    .into();
                    let evidence = SourceEvidence {
                        source_kind: SourceKind::Recurring,
                        external_transaction_id: Some(identity),
                        original_external_transaction_id: None,
                        import_batch_id: *t.id.as_uuid(),
                        source_fields_digest: digest,
                    };
                    self.validate_feature_transaction(&candidate)?;
                    Self::write_transaction_with_evidence(&tx, &candidate, Some(&evidence))?;
                    tx.execute("INSERT INTO recurring_instances(template_id,scheduled_date,transaction_id,skipped) VALUES(?1,?2,?3,0)",params![t.id.as_uuid().to_string(),due.to_string(),candidate.id.as_uuid().to_string()]).map_err(map_insert_error)?;
                    generated.push(candidate.id);
                }
                t.next_due_date = next_occurrence(t.cadence, t.interval, t.anchor_date, due)?;
                processed += 1;
            }
            if t.next_due_date != initial {
                tx.execute("UPDATE recurring_templates SET next_date=?2,edit_version=edit_version+1 WHERE id=?1",params![t.id.as_uuid().to_string(),t.next_due_date.to_string()]).map_err(storage_error)?;
            }
            if processed >= GENERATION_LIMIT {
                break;
            }
        }
        tx.commit().map_err(storage_error)?;
        Ok(generated)
    }
}
