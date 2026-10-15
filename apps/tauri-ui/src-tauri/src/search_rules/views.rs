use serde::Serialize;
#[derive(Serialize)]
pub struct CategoryView {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}
#[derive(Serialize)]
pub struct RuleView {
    pub id: String,
    pub priority: u32,
    pub merchant_contains: Option<String>,
    pub description_contains: Option<String>,
    pub category_id: String,
    pub enabled: bool,
}
#[derive(Serialize)]
pub struct AliasView {
    pub alias: String,
    pub canonical: String,
}
#[derive(Serialize)]
pub struct ClassificationState {
    pub categories: Vec<CategoryView>,
    pub rules: Vec<RuleView>,
    pub aliases: Vec<AliasView>,
}
#[derive(Serialize)]
pub struct PreviewView {
    pub match_count: u64,
    pub pending_count: u64,
    pub pending_ids: Vec<String>,
    pub pending_samples: Vec<PendingSampleView>,
}
#[derive(Serialize)]
pub struct PendingSampleView {
    pub id: String,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub amount: Option<String>,
}
