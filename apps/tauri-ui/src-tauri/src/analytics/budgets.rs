use super::*;
#[derive(Clone, Deserialize)]
pub struct BudgetInput {
    pub id: Option<String>,
    pub from_date: String,
    pub until_date: String,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub amount: String,
    pub enabled: bool,
    pub custom_period: bool,
}
#[derive(Serialize)]
pub struct BudgetView {
    pub id: String,
    pub from_date: String,
    pub until_date: String,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub category_name: String,
    pub amount: String,
    pub enabled: bool,
    pub custom_period: bool,
    pub spent: String,
    pub remaining: String,
    pub percent: String,
    pub exceeded: bool,
}
#[derive(Serialize)]
pub struct ReminderView {
    pub id: String,
    pub budget_id: String,
    pub level: String,
    pub spent: String,
    pub amount: String,
}
impl Ledger {
    pub fn write_budget(&self, input: BudgetInput) -> Result<String> {
        let create = input.id.is_none();
        let id = match input.id {
            Some(id) => Uuid::parse_str(&id)
                .map(BudgetId::from_uuid)
                .map_err(|_| "无效的预算标识")?,
            None => BudgetId::new(),
        };
        let plan = BudgetPlan {
            id,
            from_date: date(&input.from_date)?,
            until_date: date(&input.until_date)?,
            account_id: input.account_id.as_deref().map(account_id).transpose()?,
            category_id: category_id(input.category_id)?,
            amount_cny_minor: parse_amount(&input.amount, true)?,
            enabled: input.enabled,
            custom_period: input.custom_period,
        };
        let service = BudgetService::new(self.repo()?);
        if create {
            service.create(plan)
        } else {
            service.update(plan)
        }
        .map_err(|e| match e {
            ledger_app::AppError::Repository(ledger_app::RepositoryError::Conflict) => {
                "同一周期及范围已存在预算，请编辑原预算".into()
            }
            other => other.to_string(),
        })?;
        Ok(id.as_uuid().to_string())
    }
    pub fn budget_views(&self) -> Result<Vec<BudgetView>> {
        let service = BudgetService::new(self.repo()?);
        let names = service
            .repository()
            .list_categories()
            .map_err(|e| e.to_string())?;
        service
            .list()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|b| {
                let p = service.progress(b.id, Local).map_err(|e| e.to_string())?;
                Ok(BudgetView {
                    id: b.id.as_uuid().to_string(),
                    from_date: b.from_date.to_string(),
                    until_date: b.until_date.to_string(),
                    account_id: b.account_id.map(|id| id.as_uuid().to_string()),
                    category_id: b.category_id.map(|id| id.as_uuid().to_string()),
                    category_name: b
                        .category_id
                        .and_then(|id| names.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                        .unwrap_or_else(|| "全部分类".into()),
                    amount: money(b.amount_cny_minor).decimal_string(),
                    enabled: b.enabled,
                    custom_period: b.custom_period,
                    spent: money(p.spent_cny_minor).decimal_string(),
                    remaining: money(p.remaining_cny_minor).decimal_string(),
                    percent: p.percent.to_string(),
                    exceeded: p.exceeded,
                })
            })
            .collect()
    }
    pub fn archive_budget(&self, id: &str) -> Result<()> {
        let id = Uuid::parse_str(id)
            .map(BudgetId::from_uuid)
            .map_err(|_| "无效的预算标识")?;
        BudgetService::new(self.repo()?)
            .archive(id)
            .map_err(|e| e.to_string())
    }
    pub fn budget_reminders(&self) -> Result<Vec<ReminderView>> {
        let repo = self.repo()?;
        ReminderService::new(&repo)
            .due(Local::now().date_naive(), Local)
            .map_err(|e| e.to_string())
            .map(|events| {
                events
                    .into_iter()
                    .map(|e| ReminderView {
                        id: e.id,
                        budget_id: e.budget_id.as_uuid().to_string(),
                        level: e.level.into(),
                        spent: money(e.spent_cny_minor).decimal_string(),
                        amount: money(e.amount_cny_minor).decimal_string(),
                    })
                    .collect()
            })
    }
}
