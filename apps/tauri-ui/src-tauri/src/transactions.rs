//! Typed transaction detail and editing boundary. All writes use application services.
use crate::ledger::{self, Ledger, PendingEdit};
use chrono::Utc;
use ledger_app::{FeatureRepository, LedgerRepository, TransactionService};
use ledger_domain::{
    AccountKind, Posting, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize)]
pub struct WriteTransaction {
    pub edit: PendingEdit,
    pub destination_id: Option<String>,
}
#[derive(Serialize)]
pub struct Detail {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub date: String,
    pub amount: String,
    pub account_id: String,
    pub destination_id: Option<String>,
    pub merchant: String,
    pub description: String,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
    pub edit_version: u32,
    pub reversal_id: Option<String>,
    pub reversal_of: Option<String>,
    pub linked_ids: Vec<String>,
    pub sources: Vec<SourceDetail>,
    pub history: Vec<(String, i64)>,
}
#[derive(Serialize)]
pub struct SourceDetail {
    pub source: String,
    pub external_id: Option<String>,
    pub digest: String,
}
impl Ledger {
    pub fn transaction_detail(&self, id: &str) -> Result<Detail> {
        let repo = self.repo()?;
        let id = ledger::transaction_id(id)?;
        let t = repo
            .get_transaction(id)
            .map_err(|e| e.to_string())?
            .ok_or("流水不存在")?;
        let m = repo.metadata(id).map_err(|e| e.to_string())?;
        let all = repo.related_transactions(id).map_err(|e| e.to_string())?;
        let mut reversal_id = None;
        let mut linked_ids = Vec::new();
        for (other_id, is_reversal) in all {
            linked_ids.push(other_id.as_uuid().to_string());
            if is_reversal {
                reversal_id = Some(other_id.as_uuid().to_string());
            }
        }
        Ok(Detail {
            id: id.as_uuid().to_string(),
            kind: ledger::kind_label(t.kind).into(),
            status: format!("{:?}", t.status).to_lowercase(),
            date: t
                .occurred_at
                .with_timezone(&chrono::Local)
                .date_naive()
                .to_string(),
            amount: t
                .report_amount_cny
                .as_ref()
                .or_else(|| t.postings.first().map(|p| &p.delta))
                .map(|m| m.decimal_string().trim_start_matches('-').to_string())
                .unwrap_or_default(),
            account_id: t
                .postings
                .first()
                .map(|p| p.account_id.as_uuid().to_string())
                .unwrap_or_default(),
            destination_id: t
                .postings
                .get(1)
                .map(|p| p.account_id.as_uuid().to_string()),
            merchant: t.merchant.unwrap_or_default(),
            description: t.description.unwrap_or_default(),
            category_id: t.category_id.map(|i| i.as_uuid().to_string()),
            original_transaction_id: t.original_transaction_id.map(|i| i.as_uuid().to_string()),
            edit_version: m.edit_version,
            reversal_of: m.reversal_of.map(|i| i.as_uuid().to_string()),
            reversal_id,
            linked_ids,
            sources: repo
                .source_evidence(id)
                .map_err(|e| e.to_string())?
                .iter()
                .map(|e| SourceDetail {
                    source: format!("{:?}", e.source_kind).to_lowercase(),
                    external_id: e.external_transaction_id.clone(),
                    digest: e
                        .source_fields_digest
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                })
                .collect(),
            history: repo.audit_history(id).map_err(|e| e.to_string())?,
        })
    }
    pub fn write_transaction(&self, request: WriteTransaction) -> Result<String> {
        let repo = self.repo()?;
        let e = request.edit;
        let existing = if e.id.is_empty() {
            None
        } else {
            Some(
                repo.get_transaction(ledger::transaction_id(&e.id)?)
                    .map_err(|e| e.to_string())?
                    .ok_or("流水不存在")?,
            )
        };
        if existing
            .as_ref()
            .is_some_and(|transaction| transaction.status != TransactionStatus::Confirmed)
        {
            return Err("请在待确认收件箱处理此记录".into());
        }
        let mut transaction = existing.clone().unwrap_or(Transaction {
            id: TransactionId::new(),
            occurred_at: Utc::now(),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            postings: Vec::new(),
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: None,
            merchant: None,
            description: None,
        });
        let transfer = e.kind == "transfer";
        if transfer {
            let mut copy = e.clone();
            copy.kind = "expense".into();
            transaction = self.apply_pending_edit(transaction, &copy)?;
            let destination = repo
                .get_account(ledger::account_id(
                    request.destination_id.as_deref().ok_or("请选择转入平台")?,
                )?)
                .map_err(|e| e.to_string())?
                .ok_or("转入平台不存在")?;
            let amount = ledger::parse_amount(&e.amount, false)?;
            transaction.kind = TransactionKind::Transfer;
            transaction.report_amount_cny = None;
            transaction.original_transaction_id = None;
            transaction.postings.push(Posting {
                account_id: destination.id,
                delta: ledger::money(if destination.kind == AccountKind::Asset {
                    amount
                } else {
                    -amount
                }),
            });
        } else {
            transaction = self.apply_pending_edit(transaction, &e)?;
        }
        let id = transaction.id.as_uuid().to_string();
        let service = TransactionService::new(repo);
        if existing.is_some() {
            service
                .edit(ledger_domain::TransactionEdit {
                    transaction,
                    expected_version: e.expected_version,
                })
                .map_err(|e| e.to_string())?;
        } else {
            service.confirm(transaction).map_err(|e| e.to_string())?;
        }
        Ok(id)
    }
    pub fn reverse_transaction(&self, id: &str, reason: &str) -> Result<String> {
        if reason.trim().is_empty() || reason.chars().count() > 200 {
            return Err("请填写撤销原因（最多200字）".into());
        }
        let repo = self.repo()?;
        let id = repo
            .reverse_with_reason(ledger::transaction_id(id)?, Utc::now(), reason.trim())
            .map_err(|e| e.to_string())?;
        Ok(id.as_uuid().to_string())
    }
}

#[cfg(test)]
#[path = "transactions/tests.rs"]
mod tests;
