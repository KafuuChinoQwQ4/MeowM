//! User-editable categories and explainable classification rules.

use ledger_domain::{Category, CategoryId, RuleId, Transaction, TransactionId};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSuggestion {
    pub category_id: CategoryId,
    pub rule_id: RuleId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulePreview {
    pub match_count: u64,
    pub pending_count: u64,
    pub pending_ids: Vec<TransactionId>,
}

pub fn normalize_merchant(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
        Ok(self
            .suggest_details(transaction, None)?
            .map(|item| item.category_id))
    }

    pub fn suggest_details(
        &self,
        transaction: &Transaction,
        canonical_merchant: Option<&str>,
    ) -> Result<Option<RuleSuggestion>, AppError> {
        Ok(select_suggestion(
            &self.repository.list_categories()?,
            &self.repository.list_rules()?,
            transaction,
            canonical_merchant,
        ))
    }
}

pub fn select_suggestion(
    categories: &[Category],
    rules: &[ClassificationRule],
    transaction: &Transaction,
    canonical_merchant: Option<&str>,
) -> Option<RuleSuggestion> {
    let active = categories
        .iter()
        .filter(|category| category.enabled)
        .map(|category| category.id)
        .collect::<std::collections::HashSet<_>>();
    let merchant = canonical_merchant
        .map(str::to_owned)
        .or_else(|| transaction.merchant.as_deref().map(normalize_merchant));
    let description = transaction.description.as_deref().map(normalize_merchant);
    let mut rules = rules.to_vec();
    rules.sort_by_key(|rule| (rule.priority, rule.id));
    rules
        .into_iter()
        .filter(|rule| rule.enabled && active.contains(&rule.category_id))
        .find(|rule| rule_matches(rule, merchant.as_deref(), description.as_deref()))
        .map(|rule| RuleSuggestion {
            category_id: rule.category_id,
            rule_id: rule.id,
            reason: format!(
                "规则优先级 {}：商户 {}；备注 {}",
                rule.priority,
                rule.merchant_contains.as_deref().unwrap_or("任意"),
                rule.description_contains.as_deref().unwrap_or("任意")
            ),
        })
}

pub fn rule_matches(
    rule: &ClassificationRule,
    merchant: Option<&str>,
    description: Option<&str>,
) -> bool {
    let merchant_ok = rule.merchant_contains.as_ref().is_none_or(|needle| {
        merchant.is_some_and(|value| value.contains(&normalize_merchant(needle)))
    });
    let description_ok = rule.description_contains.as_ref().is_none_or(|needle| {
        description.is_some_and(|value| value.contains(&normalize_merchant(needle)))
    });
    merchant_ok && description_ok
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
