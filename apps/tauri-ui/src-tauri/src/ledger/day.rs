use super::*;
use ledger_app::ClassificationRuleRepository;

#[derive(Serialize)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub amount: String,
    pub note: String,
    pub time: String,
    pub category_name: Option<String>,
}
#[derive(Serialize)]
pub struct Reference {
    pub date: String,
    pub amount: String,
}
#[derive(Serialize)]
pub struct DayView {
    pub date: String,
    pub income: String,
    pub expense: String,
    pub net: String,
    pub balance: Option<String>,
    pub reference: Option<Reference>,
    pub entries: Vec<Entry>,
}
impl Ledger {
    pub fn save_entry(
        &self,
        id: &str,
        day: &str,
        amount: &str,
        note: &str,
        income: bool,
    ) -> Result<()> {
        let account = self.account(id)?;
        let amount = parse_amount(amount, false)?;
        if note.chars().count() > 200 {
            return Err("备注最多 200 个字符".into());
        }
        let sign = if income == (account.kind == AccountKind::Asset) {
            1
        } else {
            -1
        };
        let repo = self.repo()?;
        let mut transaction = Transaction {
            id: TransactionId::new(),
            occurred_at: time(date(day)?, false)?.with_timezone(&Utc),
            kind: if income {
                TransactionKind::Income
            } else {
                TransactionKind::Expense
            },
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: account.id,
                delta: money(amount * sign),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(money(amount)),
            merchant: None,
            description: if note.trim().is_empty() {
                None
            } else {
                Some(note.trim().into())
            },
        };
        transaction.category_id = ledger_app::classification::auto_category(
            &repo.list_categories().map_err(|error| error.to_string())?,
            &repo.list_rules().map_err(|error| error.to_string())?,
            &transaction,
            None,
        );
        TransactionService::new(repo)
            .confirm(transaction)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn set_reference(&self, id: &str, day: &str, amount: &str) -> Result<()> {
        let id = self.account(id)?.id;
        let day = date(day)?;
        AccountService::new(self.repo()?)
            .set_day_closing_balance(
                id,
                money(parse_amount(amount, true)?),
                day,
                *time(day, true)?.offset(),
            )
            .map_err(|e| e.to_string())
    }
    pub fn clear_reference(&self, id: &str) -> Result<()> {
        AccountService::new(self.repo()?)
            .clear_opening_balance(self.account(id)?.id)
            .map_err(|e| e.to_string())
    }
    pub fn day(&self, id: &str, day: &str) -> Result<DayView> {
        self.day_view(Some(id), day)
    }
    pub fn day_all(&self, day: &str) -> Result<DayView> {
        self.day_view(None, day)
    }
    fn day_view(&self, id: Option<&str>, day: &str) -> Result<DayView> {
        let account = id.map(|id| self.account(id)).transpose()?;
        let day = date(day)?;
        let repo = self.repo()?;
        let balance = if let Some(account) = &account {
            repo.balance_as_of(account.id, time(day, true)?.with_timezone(&Utc))
                .map_err(|e| e.to_string())?
                .map(|m| m.decimal_string())
        } else {
            None
        };
        let mut income = 0_i64;
        let mut expense = 0_i64;
        let mut entries = Vec::new();
        let category_names = repo
            .list_categories()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|category| (category.id, category.name))
            .collect::<std::collections::HashMap<_, _>>();
        for tx in repo
            .list_transactions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .rev()
        {
            if tx.status != TransactionStatus::Confirmed
                || tx.occurred_at.with_timezone(&Local).date_naive() != day
            {
                continue;
            }
            let posting = match &account {
                Some(account) => tx.postings.iter().find(|p| p.account_id == account.id),
                None => tx.postings.first(),
            };
            let Some(posting) = posting else {
                continue;
            };
            if let Some(effect) = repo
                .report_override(tx.id)
                .map_err(|e| e.to_string())?
                .or_else(|| ledger_domain::report_effect(&tx))
            {
                income = income
                    .checked_add(effect.income_cny_minor)
                    .ok_or("收入合计过大")?;
                expense = expense
                    .checked_add(effect.expense_cny_minor)
                    .ok_or("支出合计过大")?;
            }
            let kind = match tx.kind {
                TransactionKind::Income => "income",
                TransactionKind::Expense => "expense",
                TransactionKind::Refund => "refund",
                TransactionKind::Transfer => "transfer",
                TransactionKind::BalanceAdjustment => "adjustment",
            };
            entries.push(Entry {
                id: tx.id.as_uuid().to_string(),
                kind: kind.into(),
                amount: posting.delta.decimal_string(),
                note: tx.description.unwrap_or_default(),
                time: tx
                    .occurred_at
                    .with_timezone(&Local)
                    .format("%H:%M")
                    .to_string(),
                category_name: tx
                    .category_id
                    .and_then(|id| category_names.get(&id).cloned()),
            });
        }
        let reference = account
            .and_then(|account| {
                account
                    .opening_balance
                    .zip(account.opening_balance_effective_at)
            })
            .map(|(amount, dt)| Reference {
                date: dt.with_timezone(&Local).date_naive().to_string(),
                amount: amount.decimal_string(),
            });
        Ok(DayView {
            date: day.to_string(),
            income: money(income).decimal_string(),
            expense: money(expense).decimal_string(),
            net: money(income.checked_sub(expense).ok_or("净变化过大")?).decimal_string(),
            balance,
            reference,
            entries,
        })
    }
}
