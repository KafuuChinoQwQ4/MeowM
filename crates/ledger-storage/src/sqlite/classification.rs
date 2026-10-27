use super::*;

impl SqliteLedgerRepository {
    pub fn delete_category(&self, id: CategoryId) -> Result<(), RepositoryError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let id = id.as_uuid().to_string();
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)",
                [&id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !exists {
            return Err(RepositoryError::NotFound);
        }
        let used: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM transactions WHERE category_id = ?1)
                    OR EXISTS(SELECT 1 FROM classification_rules WHERE category_id = ?1)
                    OR EXISTS(SELECT 1 FROM budgets WHERE category_id = ?1)
                    OR EXISTS(SELECT 1 FROM recurring_templates WHERE category_id = ?1)",
                [&id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if used {
            return Err(RepositoryError::InvalidData(
                "分类已被流水、分类规则、预算或周期账单使用，请先调整关联或停用分类。".into(),
            ));
        }
        transaction
            .execute("DELETE FROM categories WHERE id = ?1", [&id])
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }
}

impl ClassificationRuleRepository for SqliteLedgerRepository {
    fn insert_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let enabled: Option<i64> = self
            .connection
            .query_row(
                "SELECT enabled FROM categories WHERE id = ?1",
                [rule.category_id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        match enabled {
            None => return Err(RepositoryError::NotFound),
            Some(0) => {
                return Err(RepositoryError::InvalidData(
                    "disabled category cannot receive new rules".into(),
                ));
            }
            Some(1) => {}
            Some(value) => {
                return Err(RepositoryError::InvalidData(format!(
                    "invalid category enabled value {value}"
                )));
            }
        }
        self.connection
            .execute(
                "INSERT INTO classification_rules(id, priority, merchant_contains, description_contains, category_id, enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rule.id.as_uuid().to_string(),
                    i64::from(rule.priority),
                    rule.merchant_contains,
                    rule.description_contains,
                    rule.category_id.as_uuid().to_string(),
                    if rule.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn list_rules(&self) -> Result<Vec<ClassificationRule>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT id, priority, merchant_contains, description_contains, category_id, enabled FROM classification_rules ORDER BY priority, id",
        ).map_err(storage_error)?;
        statement
            .query_map([], classification_rule_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }

    fn update_rule(&self, rule: ClassificationRule) -> Result<(), RepositoryError> {
        let rule_id = rule.id.as_uuid().to_string();
        let category_id = rule.category_id.as_uuid().to_string();
        let existing_category_id: Option<String> = self
            .connection
            .query_row(
                "SELECT category_id FROM classification_rules WHERE id = ?1",
                [&rule_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let Some(existing_category_id) = existing_category_id else {
            return Err(RepositoryError::NotFound);
        };
        let enabled: Option<i64> = self
            .connection
            .query_row(
                "SELECT enabled FROM categories WHERE id = ?1",
                [&category_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        match enabled {
            None => return Err(RepositoryError::NotFound),
            Some(0) if rule.enabled || existing_category_id != category_id => {
                return Err(RepositoryError::InvalidData(
                    "disabled category cannot receive active rules".into(),
                ));
            }
            Some(0 | 1) => {}
            Some(value) => {
                return Err(RepositoryError::InvalidData(format!(
                    "invalid category enabled value {value}"
                )));
            }
        }
        let changed = self
            .connection
            .execute(
                "UPDATE classification_rules SET priority = ?2, merchant_contains = ?3, description_contains = ?4, category_id = ?5, enabled = ?6 WHERE id = ?1",
                params![
                    rule_id,
                    i64::from(rule.priority),
                    rule.merchant_contains,
                    rule.description_contains,
                    category_id,
                    if rule.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn insert_category(&self, category: Category) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO categories(id, name, enabled) VALUES (?1, ?2, ?3)",
                params![
                    category.id.as_uuid().to_string(),
                    category.name,
                    if category.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    fn get_category(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, name, enabled FROM categories WHERE id = ?1",
                [id.as_uuid().to_string()],
                category_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    fn update_category(&self, category: Category) -> Result<(), RepositoryError> {
        let changed = self
            .connection
            .execute(
                "UPDATE categories SET name = ?2, enabled = ?3 WHERE id = ?1",
                params![
                    category.id.as_uuid().to_string(),
                    category.name,
                    if category.enabled { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            Err(RepositoryError::NotFound)
        } else {
            Ok(())
        }
    }

    fn list_categories(&self) -> Result<Vec<Category>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name, enabled FROM categories ORDER BY rowid")
            .map_err(storage_error)?;
        statement
            .query_map([], category_from_row)
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)
    }
}
