//! Budget writes validate scopes within the same SQLite transaction.
use super::*;
use chrono::NaiveDate;
use ledger_app::{BudgetPlan, BudgetRepository};
use ledger_domain::BudgetId;
impl BudgetRepository for SqliteLedgerRepository {
    fn list_budgets(&self) -> Result<Vec<BudgetPlan>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT id,month,account_id,category_id,amount_cny_minor,enabled,from_date,until_date FROM budgets ORDER BY month,id").map_err(storage_error)?;
        stmt.query_map([], |r| {
            let month: String = r.get(1)?;
            let from: Option<String> = r.get(6)?;
            let until: Option<String> = r.get(7)?;
            let parse = |s: &str| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map_err(|e| to_sql_conversion_error(e.to_string()))
            };
            let from_date = parse(from.as_deref().unwrap_or(&format!("{month}-01")))?;
            let until_date = if let Some(until) = until {
                parse(&until)?
            } else {
                from_date
                    .checked_add_months(chrono::Months::new(1))
                    .ok_or_else(|| to_sql_conversion_error("预算日期超出范围".into()))?
            };
            let id: String = r.get(0)?;
            let account: Option<String> = r.get(2)?;
            let category: Option<String> = r.get(3)?;
            Ok(BudgetPlan {
                id: BudgetId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
                from_date,
                until_date,
                account_id: account
                    .map(|s| parse_uuid(&s).map(AccountId::from_uuid))
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                category_id: category
                    .map(|s| parse_uuid(&s).map(CategoryId::from_uuid))
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                amount_cny_minor: r.get(4)?,
                enabled: r.get(5)?,
                custom_period: month.starts_with("custom:"),
            })
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)
    }
    fn save_budget(&self, budget: BudgetPlan, create: bool) -> Result<(), RepositoryError> {
        budget.validate()?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let old: Option<Option<String>> = tx
            .query_row(
                "SELECT category_id FROM budgets WHERE id=?1",
                [budget.id.as_uuid().to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if create && old.is_some() {
            return Err(RepositoryError::Conflict);
        }
        if !create && old.is_none() {
            return Err(RepositoryError::NotFound);
        }
        if let Some(category) = budget.category_id {
            let enabled: Option<bool> = tx
                .query_row(
                    "SELECT enabled FROM categories WHERE id=?1",
                    [category.as_uuid().to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(storage_error)?;
            let same = old
                .as_ref()
                .is_some_and(|c| c.as_deref() == Some(&category.as_uuid().to_string()));
            if enabled.is_none() || (enabled == Some(false) && !same) {
                return Err(RepositoryError::InvalidData(
                    "不能选择已停用或不存在的分类".into(),
                ));
            }
        }
        let sql = if create {
            "INSERT INTO budgets(id,month,account_id,category_id,amount_cny_minor,enabled,from_date,until_date) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"
        } else {
            "UPDATE budgets SET month=?2,account_id=?3,category_id=?4,amount_cny_minor=?5,enabled=?6,from_date=?7,until_date=?8 WHERE id=?1"
        };
        tx.execute(
            sql,
            params![
                budget.id.as_uuid().to_string(),
                budget.period_key(),
                budget.account_id.map(|id| id.as_uuid().to_string()),
                budget.category_id.map(|id| id.as_uuid().to_string()),
                budget.amount_cny_minor,
                budget.enabled,
                budget.from_date.to_string(),
                budget.until_date.to_string()
            ],
        )
        .map_err(map_insert_error)?;
        tx.commit().map_err(storage_error)
    }
    fn archive_budget(&self, id: BudgetId) -> Result<(), RepositoryError> {
        let changed = self
            .connection
            .execute(
                "UPDATE budgets SET enabled=0 WHERE id=?1",
                [id.as_uuid().to_string()],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }
}
