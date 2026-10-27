use super::*;
use ledger_app::ClassificationRuleRepository;
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
    pub destination_id: Option<String>,
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

fn validate_refund_link(repo: &SqliteLedgerRepository, tx: &Transaction) -> Result<()> {
    if tx.status != TransactionStatus::Pending || tx.kind != TransactionKind::Refund {
        return Ok(());
    }
    let Some(original) = tx.original_transaction_id else {
        return Ok(());
    };
    if repo
        .related_transactions(original)
        .map_err(|error| error.to_string())?
        .iter()
        .any(|(_, is_reversal)| *is_reversal)
    {
        return Err("关联的原消费已撤销，请重新选择原消费后再保存或确认退款。".into());
    }
    Ok(())
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
            "transfer" => TransactionKind::Transfer,
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
        if tx.kind == TransactionKind::Transfer {
            let destination =
                self.account(edit.destination_id.as_deref().ok_or("请选择转入平台")?)?;
            if destination.id == account.id {
                return Err("转出和转入平台必须不同".into());
            }
            if destination.currency.code() != "CNY" {
                return Err("请选择人民币平台".into());
            }
            tx.postings[0].delta = money(if account.kind == AccountKind::Asset {
                -amount
            } else {
                amount
            });
            tx.postings.push(Posting {
                account_id: destination.id,
                delta: money(if destination.kind == AccountKind::Asset {
                    amount
                } else {
                    -amount
                }),
            });
        }
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
        if tx.category_id.is_none()
            && matches!(
                tx.kind,
                TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund
            )
        {
            let repo = self.repo()?;
            let canonical = tx
                .merchant
                .as_deref()
                .map(|merchant| repo.canonical_merchant(merchant))
                .transpose()
                .map_err(|error| error.to_string())?;
            tx.category_id = ledger_app::classification::auto_category(
                &repo.list_categories().map_err(|error| error.to_string())?,
                &repo.list_rules().map_err(|error| error.to_string())?,
                &tx,
                canonical.as_deref(),
            );
        }
        Ok(tx)
    }
    pub fn confirm_pending(&self, ids: Vec<String>, edits: Vec<PendingEdit>) -> Result<()> {
        if ids.len() > 200 {
            return Err("请选择有效的待确认记录".into());
        }
        let ids = ids
            .iter()
            .map(|id| transaction_id(id))
            .collect::<Result<Vec<_>>>()?;
        self.confirm_pending_ids(self.repo()?, ids, edits)
    }

    pub fn confirm_all_pending(&self, mut query: PageQuery) -> Result<u64> {
        use ledger_app::FeatureRepository;
        if query
            .status
            .as_deref()
            .is_some_and(|status| status != "pending")
        {
            return Err("只能批量确认待确认记录".into());
        }
        query.status = Some("pending".into());
        query.offset = 0;
        query.limit = 200;
        let mut query = query.into_domain().map_err(|error| error.message)?;
        let repo = self.repo()?;
        let mut ids = Vec::new();
        // Collect the entire filtered selection before writing. Mutating each
        // page would shift offsets and could leave an import half confirmed.
        loop {
            let page = repo
                .query_transactions(query.clone())
                .map_err(|e| e.to_string())?;
            if page.items.is_empty() {
                break;
            }
            ids.extend(page.items.into_iter().map(|item| item.transaction.id));
            if ids.len() as u64 >= page.total {
                break;
            }
            query.offset = u32::try_from(ids.len()).map_err(|_| "待确认记录过多")?;
        }
        let count = ids.len() as u64;
        self.confirm_pending_ids(repo, ids, vec![])?;
        Ok(count)
    }

    fn confirm_pending_ids(
        &self,
        repo: SqliteLedgerRepository,
        mut ids: Vec<TransactionId>,
        edits: Vec<PendingEdit>,
    ) -> Result<()> {
        let edited_ids = edits
            .iter()
            .map(|edit| transaction_id(&edit.id))
            .collect::<Result<std::collections::HashSet<_>>>()?;
        if edits
            .iter()
            .any(|edit| !ids.iter().any(|id| id.as_uuid().to_string() == edit.id))
            || edited_ids.len() != edits.len()
        {
            return Err("请选择有效的待确认记录".into());
        }
        let mut proposed = std::collections::HashMap::new();
        let mut domain_edits = Vec::new();
        for edit in edits {
            let id = transaction_id(&edit.id)?;
            let current = repo
                .get_transaction(id)
                .map_err(|e| e.to_string())?
                .ok_or("流水不存在")?;
            let transaction = self.apply_pending_edit(current, &edit)?;
            proposed.insert(id, transaction.clone());
            domain_edits.push(ledger_domain::TransactionEdit {
                transaction,
                expected_version: edit.expected_version,
            });
        }
        for id in &ids {
            if !proposed.contains_key(id) {
                proposed.insert(
                    *id,
                    repo.get_transaction(*id)
                        .map_err(|e| e.to_string())?
                        .ok_or("流水不存在")?,
                );
            }
        }
        // Parent expenses must be written first in the same atomic confirmation.
        ids.sort_by_key(|id| proposed[id].kind == TransactionKind::Refund);
        let mut seen = std::collections::HashSet::new();
        ids.retain(|id| seen.insert(*id));
        let needs_matching = proposed.values().any(|tx| {
            tx.kind == TransactionKind::Refund
                && tx.status == TransactionStatus::Pending
                && tx.original_transaction_id.is_none()
        });
        let mut matcher = if needs_matching {
            Some(super::refunds::RefundMatcher::load(
                &repo,
                &proposed.values().cloned().collect::<Vec<_>>(),
            )?)
        } else {
            None
        };
        let categories = repo.list_categories().map_err(|e| e.to_string())?;
        let rules = repo.list_rules().map_err(|e| e.to_string())?;
        for id in &ids {
            let mut tx = proposed[id].clone();
            if tx.status == TransactionStatus::Pending {
                let before = tx.clone();
                if !edited_ids.contains(id)
                    && tx.kind == TransactionKind::Refund
                    && tx.original_transaction_id.is_none()
                    && let Some(matcher) = &matcher
                {
                    if let Some(original) = matcher
                        .matches(&tx)?
                        .into_iter()
                        .find(|m| m.confidence == "exact" && m.status == "confirmed")
                    {
                        tx.original_transaction_id = Some(transaction_id(&original.id)?);
                    }
                }
                validate_refund_link(&repo, &tx)?;
                if tx.category_id.is_none()
                    && matches!(
                        tx.kind,
                        TransactionKind::Income
                            | TransactionKind::Expense
                            | TransactionKind::Refund
                    )
                {
                    let canonical = tx
                        .merchant
                        .as_deref()
                        .map(|merchant| repo.canonical_merchant(merchant))
                        .transpose()
                        .map_err(|e| e.to_string())?;
                    tx.category_id = ledger_app::classification::auto_category(
                        &categories,
                        &rules,
                        &tx,
                        canonical.as_deref(),
                    );
                }
                if let Some(edit) = domain_edits
                    .iter_mut()
                    .find(|edit| edit.transaction.id == *id)
                {
                    edit.transaction = tx.clone();
                } else if tx != before {
                    domain_edits.push(ledger_domain::TransactionEdit {
                        transaction: tx.clone(),
                        expected_version: repo
                            .metadata(*id)
                            .map_err(|e| e.to_string())?
                            .edit_version,
                    });
                }
                if let Some(matcher) = &mut matcher {
                    matcher.record_confirmed(tx);
                }
            }
        }
        ledger_app::PendingService::new(repo)
            .confirm(ids, domain_edits)
            .map_err(|e| e.to_string())
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
        let transaction = self.apply_pending_edit(tx, &edit)?;
        validate_refund_link(&repo, &transaction)?;
        repo.edit_atomic(ledger_domain::TransactionEdit {
            transaction,
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
