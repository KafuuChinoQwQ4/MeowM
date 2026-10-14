//! User-editable categories and explainable classification rules.

use ledger_domain::{Category, CategoryId, RuleId, Transaction};

use crate::{AppError, ClassificationRuleRepository};

/// A deterministic, user-visible rule for suggesting a category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassificationRule {
    pub id: RuleId,
    pub priority: u32,
    pub merchant_contains: Option<String>,
    pub description_contains: Option<String>,
    pub category_id: CategoryId,
    pub enabled: bool,
}

/// Applies enabled rules without changing the transaction itself.
pub struct ClassificationService<R> {
    repository: R,
}

impl<R: ClassificationRuleRepository> ClassificationService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn suggest(&self, transaction: &Transaction) -> Result<Option<CategoryId>, AppError> {
        let categories = self.repository.list_categories()?;
        let enabled_categories = categories
            .into_iter()
            .filter(|category| category.enabled)
            .map(|category| category.id)
            .collect::<std::collections::HashSet<_>>();
        let merchant = transaction.merchant.as_deref().map(str::to_lowercase);
        let description = transaction.description.as_deref().map(str::to_lowercase);

        let mut matches = self
            .repository
            .list_rules()?
            .into_iter()
            .filter(|rule| rule.enabled && enabled_categories.contains(&rule.category_id))
            .filter(|rule| {
                let merchant_matches = rule.merchant_contains.as_ref().is_none_or(|needle| {
                    merchant
                        .as_deref()
                        .is_some_and(|value| value.contains(&needle.to_lowercase()))
                });
                let description_matches = rule.description_contains.as_ref().is_none_or(|needle| {
                    description
                        .as_deref()
                        .is_some_and(|value| value.contains(&needle.to_lowercase()))
                });
                merchant_matches && description_matches
            })
            .collect::<Vec<_>>();

        matches.sort_by_key(|rule| (rule.priority, rule.id));
        Ok(matches.first().map(|rule| rule.category_id))
    }
}

/// Category CRUD use cases. Historical transaction references remain valid when
/// a category is disabled.
pub struct CategoryService<R> {
    repository: R,
}

impl<R: ClassificationRuleRepository> CategoryService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn create(&self, category: Category) -> Result<(), AppError> {
        self.repository.insert_category(category)?;
        Ok(())
    }

    pub fn rename(&self, category_id: CategoryId, name: String) -> Result<(), AppError> {
        let mut category = self
            .repository
            .get_category(category_id)?
            .ok_or(AppError::CategoryNotFound)?;
        category.name = name;
        self.repository.update_category(category)?;
        Ok(())
    }

    pub fn disable(&self, category_id: CategoryId) -> Result<(), AppError> {
        let mut category = self
            .repository
            .get_category(category_id)?
            .ok_or(AppError::CategoryNotFound)?;
        category.enabled = false;
        self.repository.update_category(category)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Category>, AppError> {
        Ok(self.repository.list_categories()?)
    }
}
