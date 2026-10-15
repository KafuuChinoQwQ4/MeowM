use super::*;
use ledger_app::classification::{RulePreview, RuleSuggestion, normalize_merchant, rule_matches};

impl SqliteLedgerRepository {
    pub fn list_merchant_aliases(&self) -> Result<Vec<(String, String)>, RepositoryError> {
        let mut stmt=self.connection.prepare("SELECT alias_normalized,canonical_normalized FROM merchant_aliases ORDER BY alias_normalized").map_err(storage_error)?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(storage_error)?
            .collect::<Result<_, _>>()
            .map_err(storage_error)
    }
    pub fn save_merchant_alias(&self, alias: &str, canonical: &str) -> Result<(), RepositoryError> {
        let alias = normalize_merchant(alias);
        let canonical = normalize_merchant(canonical);
        if alias.is_empty() || canonical.is_empty() || alias.len() > 120 || canonical.len() > 120 {
            return Err(RepositoryError::InvalidData(
                "invalid merchant alias".into(),
            ));
        }
        self.connection.execute("INSERT INTO merchant_aliases(alias_normalized,canonical_normalized) VALUES(?1,?2) ON CONFLICT(alias_normalized) DO UPDATE SET canonical_normalized=excluded.canonical_normalized",params![alias,canonical]).map_err(storage_error)?;
        Ok(())
    }
    pub fn delete_merchant_alias(&self, alias: &str) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "DELETE FROM merchant_aliases WHERE alias_normalized=?1",
                [normalize_merchant(alias)],
            )
            .map_err(storage_error)?;
        Ok(())
    }
    pub fn canonical_merchant(&self, merchant: &str) -> Result<String, RepositoryError> {
        let normalized = normalize_merchant(merchant);
        Ok(self
            .connection
            .query_row(
                "SELECT canonical_normalized FROM merchant_aliases WHERE alias_normalized=?1",
                [&normalized],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .unwrap_or(normalized))
    }
    pub fn suggest_pending(
        &self,
        id: TransactionId,
    ) -> Result<Option<RuleSuggestion>, RepositoryError> {
        let t = self.get_transaction(id)?.ok_or(RepositoryError::NotFound)?;
        if t.status != TransactionStatus::Pending || t.category_id.is_some() {
            return Ok(None);
        }
        let canonical = t
            .merchant
            .as_deref()
            .map(|value| self.canonical_merchant(value))
            .transpose()?;
        Ok(ledger_app::classification::select_suggestion(
            &self.list_categories()?,
            &self.list_rules()?,
            &t,
            canonical.as_deref(),
        ))
    }
    pub fn preview_rule(&self, rule: &ClassificationRule) -> Result<RulePreview, RepositoryError> {
        let category = self
            .get_category(rule.category_id)?
            .ok_or(RepositoryError::NotFound)?;
        if rule.enabled && !category.enabled {
            return Err(RepositoryError::InvalidData("disabled category".into()));
        }
        let categories = self.list_categories()?;
        let old_rules = self.list_rules()?;
        let mut proposed = old_rules
            .iter()
            .filter(|existing| existing.id != rule.id)
            .cloned()
            .collect::<Vec<_>>();
        proposed.push(rule.clone());
        let mut count = 0;
        let mut pending_count = 0;
        let mut pending = Vec::new();
        for t in self.list_transactions()? {
            let canonical = t
                .merchant
                .as_deref()
                .map(|value| self.canonical_merchant(value))
                .transpose()?;
            let description = t.description.as_deref().map(normalize_merchant);
            if rule.enabled && rule_matches(rule, canonical.as_deref(), description.as_deref()) {
                count += 1;
            }
            if t.status == TransactionStatus::Pending && t.category_id.is_none() {
                let before = ledger_app::classification::select_suggestion(
                    &categories,
                    &old_rules,
                    &t,
                    canonical.as_deref(),
                );
                let after = ledger_app::classification::select_suggestion(
                    &categories,
                    &proposed,
                    &t,
                    canonical.as_deref(),
                );
                if before.as_ref().map(|s| (s.category_id, s.rule_id))
                    != after.as_ref().map(|s| (s.category_id, s.rule_id))
                {
                    pending_count += 1;
                    if pending.len() < 20 {
                        pending.push(t.id)
                    }
                }
            }
        }
        pending.sort_by_key(|id| *id.as_uuid());
        Ok(RulePreview {
            match_count: count,
            pending_count,
            pending_ids: pending,
        })
    }
}
