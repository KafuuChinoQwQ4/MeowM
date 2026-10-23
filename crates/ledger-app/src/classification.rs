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

/// Common merchant hints used for first-pass classification. User rules are
/// checked first, so this catalog only fills the gap before a user has taught
/// the ledger about a merchant.
const BUILTIN_MERCHANTS: &[(&str, &str)] = &[
    ("蜜雪冰城", "饮品"),
    ("喜茶", "饮品"),
    ("奈雪", "饮品"),
    ("茶百道", "饮品"),
    ("霸王茶姬", "饮品"),
    ("古茗", "饮品"),
    ("书亦烧仙草", "饮品"),
    ("一点点", "饮品"),
    ("coco", "饮品"),
    ("沪上阿姨", "饮品"),
    ("星巴克", "饮品"),
    ("瑞幸", "饮品"),
    ("麦当劳", "餐饮"),
    ("肯德基", "餐饮"),
    ("汉堡王", "餐饮"),
    ("必胜客", "餐饮"),
    ("海底捞", "餐饮"),
    ("美团外卖", "餐饮"),
    ("饿了么", "餐饮"),
    ("老乡鸡", "餐饮"),
    ("西贝", "餐饮"),
    ("达美乐", "餐饮"),
    ("杨国福", "餐饮"),
    ("张亮麻辣烫", "餐饮"),
    ("真功夫", "餐饮"),
    ("大米先生", "餐饮"),
    ("呷哺呷哺", "餐饮"),
    ("沃尔玛", "超市"),
    ("盒马", "超市"),
    ("永辉", "超市"),
    ("物美", "超市"),
    ("朴朴", "超市"),
    ("山姆", "超市"),
    ("costco", "超市"),
    ("大润发", "超市"),
    ("华润万家", "超市"),
    ("世纪联华", "超市"),
    ("全家", "便利店"),
    ("罗森", "便利店"),
    ("便利蜂", "便利店"),
    ("7-eleven", "便利店"),
    ("淘宝", "网购"),
    ("天猫", "网购"),
    ("京东", "网购"),
    ("京东到家", "网购"),
    ("拼多多", "网购"),
    ("唯品会", "网购"),
    ("得物", "网购"),
    ("抖音商城", "网购"),
    ("小红书", "网购"),
    ("滴滴", "交通"),
    ("高德打车", "交通"),
    ("曹操出行", "交通"),
    ("哈啰", "交通"),
    ("t3出行", "交通"),
    ("12306", "交通"),
    ("高铁", "交通"),
    ("地铁", "交通"),
    ("公交", "交通"),
    ("中国移动", "通讯"),
    ("中国联通", "通讯"),
    ("中国电信", "通讯"),
    ("爱奇艺", "娱乐"),
    ("腾讯视频", "娱乐"),
    ("哔哩哔哩", "娱乐"),
    ("网易云音乐", "娱乐"),
    ("qq音乐", "娱乐"),
    ("酷狗音乐", "娱乐"),
    ("猫眼电影", "娱乐"),
    ("淘票票", "娱乐"),
    ("抖音", "娱乐"),
    ("携程", "旅行"),
    ("飞猪", "旅行"),
    ("美团买药", "医疗"),
    ("京东健康", "医疗"),
    ("阿里健康", "医疗"),
    ("水费", "居住"),
    ("电费", "居住"),
    ("燃气", "居住"),
    ("优衣库", "服饰"),
    ("无印良品", "服饰"),
    ("新东方", "教育"),
    ("学而思", "教育"),
    ("工资", "工资"),
    ("薪资", "工资"),
];

/// Categories created for a fresh local ledger. Existing user categories are
/// left untouched; these names are only used when the ledger has no matching
/// category yet.
pub const DEFAULT_CATEGORY_NAMES: &[&str] = &[
    "餐饮",
    "饮品",
    "超市",
    "便利店",
    "网购",
    "交通",
    "通讯",
    "居住",
    "医疗",
    "娱乐",
    "旅行",
    "服饰",
    "教育",
    "工资",
    "其他",
];

fn builtin_category_name(
    transaction: &Transaction,
    canonical_merchant: Option<&str>,
) -> Option<&'static str> {
    let mut values = Vec::with_capacity(3);
    if let Some(value) = canonical_merchant {
        values.push(normalize_merchant(value));
    }
    if let Some(value) = transaction.merchant.as_deref() {
        values.push(normalize_merchant(value));
    }
    if let Some(value) = transaction.description.as_deref() {
        values.push(normalize_merchant(value));
    }
    BUILTIN_MERCHANTS
        .iter()
        .find(|(keyword, _)| {
            values
                .iter()
                .any(|value| value.contains(&normalize_merchant(keyword)))
        })
        .map(|(_, category)| *category)
}

/// Resolves a transaction category for confirmed writes and pending
/// confirmations. The explicit user rule wins, then the built-in catalog, and
/// finally the enabled “其他” category catches unknown merchants.
pub fn auto_category(
    categories: &[Category],
    rules: &[ClassificationRule],
    transaction: &Transaction,
    canonical_merchant: Option<&str>,
) -> Option<CategoryId> {
    if let Some(suggestion) = select_suggestion(categories, rules, transaction, canonical_merchant)
    {
        return Some(suggestion.category_id);
    }
    let target = builtin_category_name(transaction, canonical_merchant);
    target
        .and_then(|name| {
            categories
                .iter()
                .find(|category| category.enabled && category.name == name)
        })
        .or_else(|| {
            categories
                .iter()
                .find(|category| category.enabled && category.name == "其他")
        })
        .map(|category| category.id)
}

/// Category CRUD use cases. Historical transaction references remain valid when
/// a category is disabled.
pub struct CategoryService<R> {
    repository: R,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ledger_domain::{
        AccountId, CurrencyCode, Money, Posting, TransactionKind, TransactionStatus,
    };

    fn category(name: &str) -> Category {
        Category {
            id: CategoryId::new(),
            name: name.into(),
            enabled: true,
        }
    }

    fn transaction(merchant: Option<&str>, description: Option<&str>) -> Transaction {
        let currency = CurrencyCode::new("CNY", 2).unwrap();
        Transaction {
            id: TransactionId::new(),
            occurred_at: Utc::now(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: AccountId::new(),
                delta: Money::new(-100, currency.clone()),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(Money::new(100, currency)),
            merchant: merchant.map(str::to_owned),
            description: description.map(str::to_owned),
        }
    }

    #[test]
    fn auto_category_prefers_user_rules_then_builtin_brands_and_other() {
        let dining = category("餐饮");
        let drinks = category("饮品");
        let other = category("其他");
        let categories = vec![dining.clone(), drinks.clone(), other.clone()];
        let user_rule = ClassificationRule {
            id: RuleId::new(),
            priority: 0,
            merchant_contains: Some("蜜雪冰城".into()),
            description_contains: None,
            category_id: dining.id,
            enabled: true,
        };

        assert_eq!(
            auto_category(
                &categories,
                &[user_rule],
                &transaction(Some("蜜雪冰城"), None),
                None
            ),
            Some(dining.id)
        );
        assert_eq!(
            auto_category(&categories, &[], &transaction(Some("喜茶门店"), None), None),
            Some(drinks.id)
        );
        assert_eq!(
            auto_category(&categories, &[], &transaction(Some("小众商家"), None), None),
            Some(other.id)
        );
    }
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
