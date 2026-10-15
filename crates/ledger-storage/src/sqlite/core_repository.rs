use super::*;

impl LedgerRepository for SqliteLedgerRepository {
    fn report_transactions(
        &self,
        from: Timestamp,
        until: Timestamp,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT id FROM transactions WHERE status='confirmed' AND occurred_at_utc_micros>=?1 AND occurred_at_utc_micros<?2 ORDER BY occurred_at_utc_micros,id").map_err(storage_error)?;
        let ids = stmt
            .query_map(
                params![from.timestamp_micros(), until.timestamp_micros()],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        ids.into_iter()
            .map(|id| {
                self.get_transaction(TransactionId::from_uuid(
                    parse_uuid(&id).map_err(RepositoryError::InvalidData)?,
                ))?
                .ok_or(RepositoryError::NotFound)
            })
            .collect()
    }

    fn report_override(
        &self,
        id: TransactionId,
    ) -> Result<Option<ledger_domain::ReportEffect>, RepositoryError> {
        self.connection.query_row("SELECT m.report_income_minor,m.report_expense_minor FROM transaction_metadata m JOIN transactions t ON t.id=m.transaction_id WHERE m.transaction_id=?1 AND t.status='confirmed' AND m.report_income_minor IS NOT NULL",[id.as_uuid().to_string()],|r|Ok(ledger_domain::ReportEffect{income_cny_minor:r.get(0)?,expense_cny_minor:r.get(1)?})).optional().map_err(storage_error)
    }
    fn insert_account(&self, account: Account) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO accounts(id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account.id.as_uuid().to_string(), account.display_name,
                    account_kind_str(account.kind), account.currency.code(),
                    account.currency.minor_unit_exponent(), account.opening_balance.as_ref().map(Money::minor_units),
                    account.opening_balance_effective_at.as_ref().map(DateTime::timestamp_micros),
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn get_account(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros FROM accounts WHERE id = ?1",
                [id.as_uuid().to_string()],
                account_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    fn list_accounts(&self) -> Result<Vec<Account>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, display_name, kind, currency_code, currency_exponent, opening_balance_minor, opening_balance_effective_at_utc_micros FROM accounts ORDER BY rowid",
        ).map_err(storage_error)?;
        statement
            .query_map([], account_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }

    fn update_account(&self, account: Account) -> Result<(), RepositoryError> {
        let changed = self.connection.execute(
            "UPDATE accounts SET display_name = ?2, kind = ?3, currency_code = ?4, currency_exponent = ?5, opening_balance_minor = ?6, opening_balance_effective_at_utc_micros = ?7 WHERE id = ?1",
            params![
                account.id.as_uuid().to_string(), account.display_name,
                account_kind_str(account.kind), account.currency.code(),
                account.currency.minor_unit_exponent(), account.opening_balance.as_ref().map(Money::minor_units),
                account.opening_balance_effective_at.as_ref().map(DateTime::timestamp_micros),
            ],
        ).map_err(map_insert_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn insert_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        self.insert_transaction_with_evidence(transaction, None)
    }

    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        let row = self.connection.query_row(
            "SELECT id, occurred_at_utc_micros, kind, status, category_id, original_transaction_id, report_amount_minor, report_currency_code, report_currency_exponent, merchant, description FROM transactions WHERE id = ?1",
            [id.as_uuid().to_string()],
            transaction_from_row,
        ).optional().map_err(storage_error)?;
        let Some((mut transaction, report_data)) = row else {
            return Ok(None);
        };
        transaction.report_amount_cny = report_data?;
        let mut statement = self.connection.prepare(
            "SELECT account_id, delta_minor, currency_code, currency_exponent FROM postings WHERE transaction_id = ?1 ORDER BY ordinal",
        ).map_err(storage_error)?;
        transaction.postings = statement
            .query_map([id.as_uuid().to_string()], posting_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        Ok(Some(transaction))
    }

    fn list_transactions(&self) -> Result<Vec<Transaction>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM transactions ORDER BY occurred_at_utc_micros, rowid")
            .map_err(storage_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        ids.into_iter()
            .map(|id| {
                let id = parse_uuid(&id)
                    .map(TransactionId::from_uuid)
                    .map_err(RepositoryError::InvalidData)?;
                self.get_transaction(id)?.ok_or_else(|| {
                    RepositoryError::Storage("transaction disappeared while listing".into())
                })
            })
            .collect()
    }

    fn update_transaction(&self, transaction: Transaction) -> Result<(), RepositoryError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        Self::write_transaction(&tx, &transaction, false)?;
        tx.commit().map_err(storage_error)
    }

    fn balance_as_of(
        &self,
        account_id: AccountId,
        cutoff_utc: Timestamp,
    ) -> Result<Option<Money>, RepositoryError> {
        let account = self
            .get_account(account_id)?
            .ok_or(RepositoryError::NotFound)?;
        let (Some(mut balance), Some(reference)) = (
            account.opening_balance,
            account.opening_balance_effective_at,
        ) else {
            return Ok(None);
        };
        let reference = reference.timestamp_micros();
        let cutoff = cutoff_utc.timestamp_micros();
        let (lower, upper, direction) = if cutoff >= reference {
            (reference, cutoff, 1_i64)
        } else {
            (cutoff, reference, -1_i64)
        };
        let mut statement = self.connection.prepare(
            "SELECT p.delta_minor, p.currency_code, p.currency_exponent FROM postings p JOIN transactions t ON t.id = p.transaction_id WHERE p.account_id = ?1 AND t.status = 'confirmed' AND t.occurred_at_utc_micros > ?2 AND t.occurred_at_utc_micros <= ?3 ORDER BY t.occurred_at_utc_micros, t.rowid, p.ordinal",
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![account_id.as_uuid().to_string(), lower, upper],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u8>(2)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        for row in rows {
            let (minor, code, exponent) = row.map_err(storage_error)?;
            let money = Money::new(
                minor,
                CurrencyCode::new(&code, exponent)
                    .map_err(|e| RepositoryError::InvalidData(e.to_string()))?,
            );
            balance = if direction > 0 {
                balance.checked_add(money)
            } else {
                balance.checked_sub(money)
            }
            .map_err(|e| RepositoryError::InvalidData(e.to_string()))?;
        }
        Ok(Some(balance))
    }
}
