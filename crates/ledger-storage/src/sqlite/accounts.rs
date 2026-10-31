use super::*;

impl SqliteLedgerRepository {
    /// An explicitly emptied ledger must stay empty, including after backup restoration.
    pub fn needs_default_platforms(&self) -> Result<bool, RepositoryError> {
        self.connection
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM accounts)
                 AND NOT EXISTS(SELECT 1 FROM audit_events WHERE action='platform_deleted')",
                [],
                |row| row.get(0),
            )
            .map_err(storage_error)
    }

    /// Delete only unused platforms. The immediate transaction also protects against
    /// background capture linking a record between validation and deletion.
    pub fn delete_platform(&self, id: AccountId) -> Result<(), RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage_error)?;
        let id = id.as_uuid().to_string();
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
                [&id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !exists {
            return Err(RepositoryError::NotFound);
        }
        for (query, message) in [
            (
                "SELECT EXISTS(SELECT 1 FROM postings WHERE account_id=?1)",
                "该平台有关联流水（含待确认或已忽略记录），无法删除。",
            ),
            (
                "SELECT EXISTS(SELECT 1 FROM budgets WHERE account_id=?1)",
                "该平台有关联预算，请先调整预算的平台范围。",
            ),
            (
                "SELECT EXISTS(SELECT 1 FROM recurring_templates WHERE account_id=?1)",
                "该平台有关联周期账单，请先将账单改到其他平台。",
            ),
            (
                "SELECT EXISTS(SELECT 1 FROM reconciliation_sessions WHERE account_id=?1)",
                "该平台有关联对账记录，无法删除。",
            ),
            (
                "SELECT EXISTS(SELECT 1 FROM notification_settings WHERE account_id=?1)
                 OR EXISTS(SELECT 1 FROM screen_settings WHERE account_id=?1)",
                "该平台仍被采集设置使用，请先在「平台与采集」中更换或清除默认平台。",
            ),
        ] {
            let referenced: bool = tx
                .query_row(query, [&id], |row| row.get(0))
                .map_err(storage_error)?;
            if referenced {
                return Err(RepositoryError::InvalidData(message.into()));
            }
        }
        // platform_metadata is removed by the account foreign key's cascade.
        tx.execute("DELETE FROM accounts WHERE id=?1", [&id])
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO audit_events(id,occurred_at_utc_micros,action)
             VALUES(?1,?2,'platform_deleted')",
            params![Uuid::new_v4().to_string(), Utc::now().timestamp_micros()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }

    pub fn platform_category(&self, id: AccountId) -> Result<Option<String>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT category FROM platform_metadata WHERE account_id=?1",
                [id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }

    /// Stores platform identity and classification together, preserving postings and balances.
    pub fn save_platform(
        &self,
        id: AccountId,
        name: &str,
        category: &str,
        create: bool,
        currency: &CurrencyCode,
    ) -> Result<(), RepositoryError> {
        if !["wallet", "bank", "securities", "exchange", "cash", "other"].contains(&category) {
            return Err(RepositoryError::InvalidData(
                "invalid platform category".into(),
            ));
        }
        if !ledger_domain::supported_currencies()
            .iter()
            .any(|c| c.code == currency.code() && c.exponent == currency.minor_unit_exponent())
        {
            return Err(RepositoryError::InvalidData("unsupported currency".into()));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let id = id.as_uuid().to_string();
        let duplicate: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE display_name=?1 AND id<>?2)",
                params![name, id],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if duplicate {
            return Err(RepositoryError::InvalidData("平台名称已存在".into()));
        }
        if create {
            tx.execute("INSERT INTO accounts(id,display_name,kind,currency_code,currency_exponent) VALUES (?1,?2,'asset',?3,?4)", params![id,name,currency.code(),currency.minor_unit_exponent()]).map_err(storage_error)?;
        } else if tx
            .execute(
                "UPDATE accounts SET display_name=?2 WHERE id=?1",
                params![id, name],
            )
            .map_err(storage_error)?
            == 0
        {
            return Err(RepositoryError::InvalidData("平台不存在".into()));
        }
        tx.execute("INSERT INTO platform_metadata(account_id,category) VALUES (?1,?2) ON CONFLICT(account_id) DO UPDATE SET category=excluded.category", params![id,category]).map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}
