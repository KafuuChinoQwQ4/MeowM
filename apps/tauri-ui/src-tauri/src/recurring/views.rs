use super::*;
#[derive(Serialize)]
pub struct TemplateView {
    pub id: String,
    pub version: u32,
    pub account_id: String,
    pub kind: String,
    pub amount: String,
    pub category_id: Option<String>,
    pub category_name: String,
    pub merchant: String,
    pub description: String,
    pub cadence: String,
    pub interval: u32,
    pub next_due_date: String,
    pub end_date: Option<String>,
    pub enabled: bool,
    pub ended: bool,
}
impl TemplateView {
    pub(super) fn new(t: RecurringTemplate, names: &[Category]) -> Self {
        let ended = t.ended();
        Self {
            id: t.id.as_uuid().to_string(),
            version: t.version,
            account_id: t.account_id.as_uuid().to_string(),
            kind: kind_label(t.kind).into(),
            amount: money(t.amount_cny_minor).decimal_string(),
            category_id: t.category_id.map(|id| id.as_uuid().to_string()),
            category_name: t
                .category_id
                .and_then(|id| names.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                .unwrap_or_else(|| "未分类".into()),
            merchant: t.merchant.unwrap_or_default(),
            description: t.description.unwrap_or_default(),
            cadence: match t.cadence {
                RecurringCadence::Weekly => "weekly",
                RecurringCadence::Monthly => "monthly",
                RecurringCadence::Yearly => "yearly",
            }
            .into(),
            interval: t.interval,
            next_due_date: t.next_due_date.to_string(),
            end_date: t.end_date.map(|d| d.to_string()),
            enabled: t.enabled,
            ended,
        }
    }
}
#[derive(Serialize)]
pub struct ReminderView {
    pub template_id: String,
    pub due_date: String,
    pub transaction_id: Option<String>,
    pub account_id: String,
    pub amount: String,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub overdue: bool,
}
impl From<RecurringReminder> for ReminderView {
    fn from(r: RecurringReminder) -> Self {
        Self {
            template_id: r.template_id.as_uuid().to_string(),
            due_date: r.due_date.to_string(),
            transaction_id: r.transaction_id.map(|id| id.as_uuid().to_string()),
            account_id: r.account_id.as_uuid().to_string(),
            amount: money(r.amount_cny_minor).decimal_string(),
            merchant: r.merchant,
            description: r.description,
            overdue: r.overdue,
        }
    }
}
#[derive(Serialize)]
pub struct RecurringState {
    pub templates: Vec<TemplateView>,
    pub reminders: Vec<ReminderView>,
    pub generated_count: usize,
    pub has_more: bool,
}
#[derive(Serialize)]
pub struct InstanceView {
    pub due_date: String,
    pub transaction_id: Option<String>,
    pub status: String,
    pub amount: Option<String>,
}
impl From<RecurringInstance> for InstanceView {
    fn from(i: RecurringInstance) -> Self {
        Self {
            due_date: i.due_date.to_string(),
            transaction_id: i.transaction_id.map(|id| id.as_uuid().to_string()),
            status: match i.status {
                RecurringInstanceStatus::Pending => "pending",
                RecurringInstanceStatus::Confirmed => "confirmed",
                RecurringInstanceStatus::Ignored => "ignored",
                RecurringInstanceStatus::Skipped => "skipped",
            }
            .into(),
            amount: i.amount_cny_minor.map(|n| money(n).decimal_string()),
        }
    }
}
