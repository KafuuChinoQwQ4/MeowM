use super::*;
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub struct RecurringInput {
    pub id: Option<String>,
    pub expected_version: Option<u32>,
    pub account_id: String,
    pub kind: String,
    pub amount: String,
    pub category_id: Option<String>,
    pub merchant: String,
    pub description: String,
    pub cadence: String,
    pub interval: u32,
    pub next_due_date: String,
    pub end_date: Option<String>,
    pub enabled: bool,
}
impl Ledger {
    pub fn write_recurring(&self, input: RecurringInput) -> Result<String> {
        let repo = self.repo()?;
        let service = RecurringService::new(&repo);
        let id = input
            .id
            .as_deref()
            .map(template_id)
            .transpose()?
            .unwrap_or_default();
        let old = if input.id.is_some() {
            Some(
                service
                    .list()
                    .map_err(recurring_error)?
                    .into_iter()
                    .find(|t| t.id == id)
                    .ok_or("周期账单不存在")?,
            )
        } else {
            None
        };
        if old.is_some() != input.expected_version.is_some() {
            return Err("编辑周期账单需要有效版本，请刷新后重试".into());
        }
        let next = date(&input.next_due_date)?;
        let cadence = match input.cadence.as_str() {
            "weekly" => RecurringCadence::Weekly,
            "monthly" => RecurringCadence::Monthly,
            "yearly" => RecurringCadence::Yearly,
            _ => return Err("请选择每周、每月或每年".into()),
        };
        let anchor = old
            .as_ref()
            .filter(|t| {
                t.next_due_date == next && t.cadence == cadence && t.interval == input.interval
            })
            .map(|t| t.anchor_date)
            .unwrap_or(next);
        let t = RecurringTemplate {
            id,
            account_id: account_id(&input.account_id)?,
            kind: match input.kind.as_str() {
                "income" => TransactionKind::Income,
                "expense" => TransactionKind::Expense,
                _ => return Err("周期账单仅支持收入或支出".into()),
            },
            amount_cny_minor: parse_amount(&input.amount, false)?,
            category_id: input
                .category_id
                .map(|s| {
                    uuid::Uuid::parse_str(&s)
                        .map(CategoryId::from_uuid)
                        .map_err(|_| "无效的分类标识")
                })
                .transpose()?,
            merchant: (!input.merchant.trim().is_empty()).then(|| input.merchant.trim().to_owned()),
            description: (!input.description.trim().is_empty())
                .then(|| input.description.trim().to_owned()),
            cadence,
            interval: input.interval,
            anchor_date: anchor,
            next_due_date: next,
            end_date: input.end_date.as_deref().map(date).transpose()?,
            enabled: input.enabled,
            version: input.expected_version.unwrap_or(0),
        };
        if let Some(version) = input.expected_version {
            service.update(t, version)
        } else {
            service.create(t)
        }
        .map_err(recurring_error)?;
        Ok(id.as_uuid().to_string())
    }
}
