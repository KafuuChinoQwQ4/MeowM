use super::*;
pub fn ignore_pending(ledger: &Ledger, ids: Vec<String>) -> Result<()> {
    use ledger_app::FeatureRepository;
    let repo = ledger.repo()?;
    let ids = ids
        .into_iter()
        .map(|id| {
            uuid::Uuid::parse_str(&id)
                .map(ledger_domain::TransactionId::from_uuid)
                .map_err(|_| "无效的流水标识".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    repo.ignore_bulk(&ids).map_err(|e| e.to_string())
}
pub fn restore_pending(ledger: &Ledger, id: String) -> Result<()> {
    use ledger_app::FeatureRepository;
    let id = uuid::Uuid::parse_str(&id)
        .map(ledger_domain::TransactionId::from_uuid)
        .map_err(|_| "无效的流水标识".to_owned())?;
    ledger
        .repo()?
        .restore_pending_atomic(id)
        .map_err(|e| e.to_string())
}

#[derive(Clone, serde::Deserialize)]
pub struct PendingEdit {
    pub id: String,
    pub expected_version: u32,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub kind: String,
    pub merchant: String,
    pub description: String,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
}
#[derive(Serialize)]
pub struct CategoryOption {
    pub id: String,
    pub name: String,
}

impl Ledger {
    pub(crate) fn apply_pending_edit(
        &self,
        mut tx: Transaction,
        edit: &PendingEdit,
    ) -> Result<Transaction> {
        let account = self.account(&edit.account_id)?;
        if edit.merchant.chars().count() > 100 || edit.description.chars().count() > 200 {
            return Err("商户最多 100 字，备注最多 200 字".into());
        }
        tx.kind = match edit.kind.as_str() {
            "income" => TransactionKind::Income,
            "expense" => TransactionKind::Expense,
            "refund" => TransactionKind::Refund,
            _ => return Err("无效的候选类型".into()),
        };
        let amount = parse_amount(&edit.amount, false)?;
        let positive = matches!(tx.kind, TransactionKind::Income | TransactionKind::Refund);
        let sign = if positive == (account.kind == AccountKind::Asset) {
            1
        } else {
            -1
        };
        let proposed_date = date(&edit.date)?;
        if tx.occurred_at.with_timezone(&Local).date_naive() != proposed_date {
            tx.occurred_at = time(proposed_date, false)?.with_timezone(&Utc);
        }
        tx.postings = vec![Posting {
            account_id: account.id,
            delta: money(amount * sign),
        }];
        tx.report_amount_cny = Some(money(amount));
        tx.merchant = (!edit.merchant.trim().is_empty()).then(|| edit.merchant.trim().into());
        tx.description =
            (!edit.description.trim().is_empty()).then(|| edit.description.trim().into());
        tx.original_transaction_id = edit
            .original_transaction_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(transaction_id)
            .transpose()?;
        tx.category_id = edit
            .category_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(|id| {
                uuid::Uuid::parse_str(id)
                    .map(ledger_domain::CategoryId::from_uuid)
                    .map_err(|_| "无效分类".to_owned())
            })
            .transpose()?;
        Ok(tx)
    }
    pub fn confirm_pending(&self, ids: Vec<String>, edits: Vec<PendingEdit>) -> Result<()> {
        let repo = self.repo()?;
        let mut domain_edits = Vec::new();
        for edit in edits {
            let id = transaction_id(&edit.id)?;
            let tx = repo
                .get_transaction(id)
                .map_err(|error| error.to_string())?
                .ok_or("流水不存在")?;
            domain_edits.push(ledger_domain::TransactionEdit {
                transaction: self.apply_pending_edit(tx, &edit)?,
                expected_version: edit.expected_version,
            });
        }
        ledger_app::PendingService::new(repo)
            .confirm(
                ids.iter()
                    .map(|id| transaction_id(id))
                    .collect::<Result<Vec<_>>>()?,
                domain_edits,
            )
            .map_err(|error| error.to_string())
    }
    pub fn edit_pending(&self, edit: PendingEdit) -> Result<()> {
        use ledger_app::FeatureRepository;
        let repo = self.repo()?;
        let tx = repo
            .get_transaction(transaction_id(&edit.id)?)
            .map_err(|error| error.to_string())?
            .ok_or("流水不存在")?;
        if tx.status != TransactionStatus::Pending {
            return Err("只能修改待确认记录".into());
        }
        repo.edit_atomic(ledger_domain::TransactionEdit {
            transaction: self.apply_pending_edit(tx, &edit)?,
            expected_version: edit.expected_version,
        })
        .map_err(|error| error.to_string())
    }
    pub fn categories(&self) -> Result<Vec<CategoryOption>> {
        use ledger_app::ClassificationRuleRepository;
        self.repo()?
            .list_categories()
            .map(|categories| {
                categories
                    .into_iter()
                    .filter(|category| category.enabled)
                    .map(|category| CategoryOption {
                        id: category.id.as_uuid().to_string(),
                        name: category.name,
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    }
}
