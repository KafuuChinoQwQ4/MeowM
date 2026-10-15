use super::*;

impl SqliteLedgerRepository {
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
    ) -> Result<(), RepositoryError> {
        if !["wallet", "bank", "securities", "exchange", "cash", "other"].contains(&category) {
            return Err(RepositoryError::InvalidData(
                "invalid platform category".into(),
            ));
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
            tx.execute("INSERT INTO accounts(id,display_name,kind,currency_code,currency_exponent) VALUES (?1,?2,'asset','CNY',2)", params![id,name]).map_err(storage_error)?;
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
