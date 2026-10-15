use super::*;
#[derive(Deserialize)]
pub struct ReportInput {
    pub from_date: String,
    pub until_date: String,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub merchant: Option<String>,
    pub merchant_offset: u32,
    pub merchant_limit: u32,
}
#[derive(Serialize)]
pub struct ReportView {
    pub income: String,
    pub expense: String,
    pub net: String,
    pub transaction_count: u64,
    pub categories: Vec<CategoryView>,
    pub merchants: Vec<MerchantView>,
    pub merchant_total: usize,
    pub trend: Vec<TrendView>,
    pub balances: Vec<BalanceView>,
}
#[derive(Serialize)]
pub struct CategoryView {
    pub id: Option<String>,
    pub name: String,
    pub expense: String,
    pub share_percent: u32,
}
#[derive(Serialize)]
pub struct MerchantView {
    pub name: String,
    pub income: String,
    pub expense: String,
}
#[derive(Serialize)]
pub struct TrendView {
    pub month: String,
    pub income: String,
    pub expense: String,
}
#[derive(Serialize)]
pub struct BalanceView {
    pub account_id: String,
    pub name: String,
    pub amount: Option<String>,
}
impl Ledger {
    pub fn report(&self, input: ReportInput) -> Result<ReportView> {
        let range = CalendarRange::new(date(&input.from_date)?, date(&input.until_date)?, Local)
            .map_err(|e| e.to_string())?;
        let report = ReportService::new(self.repo()?)
            .summarize_scoped(ScopedReportQuery {
                range: range.query,
                account_id: input.account_id.as_deref().map(account_id).transpose()?,
                category_id: category_id(input.category_id)?,
                merchant: input
                    .merchant
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty()),
                merchant_offset: input.merchant_offset,
                merchant_limit: input.merchant_limit,
            })
            .map_err(|e| e.to_string())?;
        let trend = report
            .summary
            .income_by_month
            .iter()
            .map(|(label, income)| TrendView {
                month: label.clone(),
                income: money(*income).decimal_string(),
                expense: money(*report.summary.expense_by_month.get(label).unwrap_or(&0))
                    .decimal_string(),
            })
            .collect();
        Ok(ReportView {
            income: money(report.summary.income_cny_minor).decimal_string(),
            expense: money(report.summary.expense_cny_minor).decimal_string(),
            net: money(report.summary.net_cny_minor).decimal_string(),
            transaction_count: report.transaction_count,
            categories: report
                .categories
                .into_iter()
                .map(|c| CategoryView {
                    id: c.id.map(|id| id.as_uuid().to_string()),
                    name: c.name,
                    expense: money(c.expense_cny_minor).decimal_string(),
                    share_percent: c.share_percent,
                })
                .collect(),
            merchants: report
                .merchants
                .into_iter()
                .map(|m| MerchantView {
                    name: m.name,
                    income: money(m.income_cny_minor).decimal_string(),
                    expense: money(m.expense_cny_minor).decimal_string(),
                })
                .collect(),
            merchant_total: report.merchant_total,
            trend,
            balances: report
                .balances
                .into_iter()
                .map(|b| BalanceView {
                    account_id: b.account_id.as_uuid().to_string(),
                    name: b.name,
                    amount: b.amount_cny_minor.map(|v| money(v).decimal_string()),
                })
                .collect(),
        })
    }
}
