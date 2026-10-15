use super::views::*;
use crate::{LedgerState, access, ledger::Ledger};
use ledger_app::{ClassificationRule, ClassificationRuleRepository, LedgerRepository};
use ledger_domain::{Category, CategoryId, RuleId};
use serde::Deserialize;
use tauri::State;
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Deserialize)]
pub struct RuleInput {
    pub id: Option<String>,
    pub priority: u32,
    pub merchant_contains: Option<String>,
    pub description_contains: Option<String>,
    pub category_id: String,
    pub enabled: bool,
}
fn parse_rule(input: RuleInput) -> Result<ClassificationRule> {
    let merchant = input
        .merchant_contains
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let description = input
        .description_contains
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if merchant.is_none() && description.is_none() {
        return Err("至少填写一项商户或备注条件".into());
    }
    if merchant.as_ref().is_some_and(|s| s.chars().count() > 100)
        || description
            .as_ref()
            .is_some_and(|s| s.chars().count() > 100)
        || input.priority > 10000
    {
        return Err("规则条件或优先级超出范围".into());
    }
    let id = input
        .id
        .as_deref()
        .map(|s| {
            uuid::Uuid::parse_str(s)
                .map(RuleId::from_uuid)
                .map_err(|_| "无效的规则标识")
        })
        .transpose()?
        .unwrap_or_default();
    let category_id = CategoryId::from_uuid(
        uuid::Uuid::parse_str(&input.category_id).map_err(|_| "无效的分类标识")?,
    );
    Ok(ClassificationRule {
        id,
        priority: input.priority,
        merchant_contains: merchant,
        description_contains: description,
        category_id,
        enabled: input.enabled,
    })
}
impl Ledger {
    pub fn classification_state(&self) -> Result<ClassificationState> {
        let repo = self.repo()?;
        Ok(ClassificationState {
            categories: repo
                .list_categories()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|c| CategoryView {
                    id: c.id.as_uuid().to_string(),
                    name: c.name,
                    enabled: c.enabled,
                })
                .collect(),
            rules: repo
                .list_rules()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| RuleView {
                    id: r.id.as_uuid().to_string(),
                    priority: r.priority,
                    merchant_contains: r.merchant_contains,
                    description_contains: r.description_contains,
                    category_id: r.category_id.as_uuid().to_string(),
                    enabled: r.enabled,
                })
                .collect(),
            aliases: repo
                .list_merchant_aliases()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|(alias, canonical)| AliasView { alias, canonical })
                .collect(),
        })
    }
    pub fn preview_rule(&self, input: RuleInput) -> Result<PreviewView> {
        let repo = self.repo()?;
        let rule = parse_rule(input)?;
        let preview = repo.preview_rule(&rule).map_err(|e| e.to_string())?;
        let pending_samples = preview
            .pending_ids
            .iter()
            .map(|id| {
                let transaction = repo
                    .get_transaction(*id)
                    .map_err(|e| e.to_string())?
                    .ok_or("候选不存在")?;
                Ok(PendingSampleView {
                    id: id.as_uuid().to_string(),
                    merchant: transaction.merchant,
                    description: transaction.description,
                    amount: transaction
                        .report_amount_cny
                        .map(|amount| amount.decimal_string()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(PreviewView {
            match_count: preview.match_count,
            pending_count: preview.pending_count,
            pending_ids: preview
                .pending_ids
                .into_iter()
                .map(|id| id.as_uuid().to_string())
                .collect(),
            pending_samples,
        })
    }
    pub fn write_rule(&self, input: RuleInput) -> Result<String> {
        let repo = self.repo()?;
        let creating = input.id.is_none();
        let rule = parse_rule(input)?;
        repo.preview_rule(&rule).map_err(|e| e.to_string())?;
        if creating {
            repo.insert_rule(rule.clone())
        } else {
            repo.update_rule(rule.clone())
        }
        .map_err(|e| e.to_string())?;
        Ok(rule.id.as_uuid().to_string())
    }
    pub fn write_category(
        &self,
        id: Option<String>,
        name: String,
        enabled: bool,
    ) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("分类名称需为 1 至 60 字".into());
        }
        let repo = self.repo()?;
        let creating = id.is_none();
        let id = id
            .as_deref()
            .map(|v| {
                uuid::Uuid::parse_str(v)
                    .map(CategoryId::from_uuid)
                    .map_err(|_| "无效的分类标识")
            })
            .transpose()?
            .unwrap_or_default();
        let category = Category {
            id,
            name: name.into(),
            enabled,
        };
        if id.as_uuid().is_nil() {
            return Err("无效的分类标识".into());
        }
        if creating {
            repo.insert_category(category)
        } else {
            repo.update_category(category)
        }
        .map_err(|e| e.to_string())?;
        Ok(id.as_uuid().to_string())
    }
    pub fn save_alias(&self, alias: &str, canonical: &str) -> Result<()> {
        self.repo()?
            .save_merchant_alias(alias, canonical)
            .map_err(|e| e.to_string())
    }
    pub fn delete_alias(&self, alias: &str) -> Result<()> {
        self.repo()?
            .delete_merchant_alias(alias)
            .map_err(|e| e.to_string())
    }
}
#[tauri::command]
pub fn classification_state(state: State<'_, LedgerState>) -> Result<ClassificationState> {
    access(state, |l| l.classification_state())
}
#[tauri::command]
pub fn preview_classification_rule(
    state: State<'_, LedgerState>,
    request: RuleInput,
) -> Result<PreviewView> {
    access(state, |l| l.preview_rule(request))
}
#[tauri::command]
pub fn write_classification_rule(
    state: State<'_, LedgerState>,
    request: RuleInput,
) -> Result<String> {
    access(state, |l| l.write_rule(request))
}
#[tauri::command]
pub fn write_category(
    state: State<'_, LedgerState>,
    id: Option<String>,
    name: String,
    enabled: bool,
) -> Result<String> {
    access(state, |l| l.write_category(id, name, enabled))
}
#[tauri::command]
pub fn save_merchant_alias(
    state: State<'_, LedgerState>,
    alias: String,
    canonical: String,
) -> Result<()> {
    access(state, |l| l.save_alias(&alias, &canonical))
}
#[tauri::command]
pub fn delete_merchant_alias(state: State<'_, LedgerState>, alias: String) -> Result<()> {
    access(state, |l| l.delete_alias(&alias))
}
