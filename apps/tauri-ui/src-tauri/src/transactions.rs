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
mod tests {
    use super::*;

    fn request(account: &str, kind: &str, amount: &str) -> WriteTransaction {
        WriteTransaction {
            edit: PendingEdit {
                id: String::new(),
                expected_version: 0,
                account_id: account.into(),
                date: "2026-10-14".into(),
                amount: amount.into(),
                kind: kind.into(),
                merchant: "测试商户".into(),
                description: "阶段二测试".into(),
                category_id: None,
                original_transaction_id: None,
            },
            destination_id: None,
        }
    }

    #[test]
    fn manual_transfer_and_refund_keep_links_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let ledger = Ledger::open(&path).unwrap();
        let accounts = ledger.accounts().unwrap();
        let source = &accounts[0].id;
        let destination = &accounts[1].id;
        let expense = ledger
            .write_transaction(request(source, "expense", "25.00"))
            .unwrap();
        let mut refund = request(source, "refund", "5.00");
        refund.edit.original_transaction_id = Some(expense.clone());
        let refund_id = ledger.write_transaction(refund).unwrap();
        let mut transfer = request(source, "transfer", "10.00");
        transfer.destination_id = Some(destination.clone());
        let transfer_id = ledger.write_transaction(transfer).unwrap();
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        assert_eq!(
            ledger
                .transaction_detail(&refund_id)
                .unwrap()
                .original_transaction_id
                .as_deref(),
            Some(expense.as_str())
        );
        assert!(
            ledger
                .transaction_detail(&expense)
                .unwrap()
                .linked_ids
                .contains(&refund_id)
        );
        assert_eq!(
            ledger
                .transaction_detail(&transfer_id)
                .unwrap()
                .destination_id
                .as_deref(),
            Some(destination.as_str())
        );
        let transfer = ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&transfer_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(transfer.postings.len(), 2);
        assert_eq!(
            transfer.postings[0].delta.minor_units() + transfer.postings[1].delta.minor_units(),
            0
        );
    }

    #[test]
    fn confirmed_transaction_kind_and_refund_parent_are_immutable() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
        let account = ledger.accounts().unwrap().remove(0).id;
        let expense = ledger
            .write_transaction(request(&account, "expense", "25.00"))
            .unwrap();
        let mut edit = request(&account, "income", "25.00");
        edit.edit.id = expense.clone();
        assert!(ledger.write_transaction(edit).is_err());
        let mut refund = request(&account, "refund", "5.00");
        refund.edit.original_transaction_id = Some(expense.clone());
        let refund_id = ledger.write_transaction(refund).unwrap();
        let mut edit = request(&account, "refund", "5.00");
        edit.edit.id = refund_id.clone();
        edit.edit.original_transaction_id = Some(expense);
        assert!(ledger.write_transaction(edit).is_ok());
        let mut change_parent = request(&account, "refund", "5.00");
        change_parent.edit.id = refund_id;
        assert!(ledger.write_transaction(change_parent).is_err());
    }

    #[test]
    fn editing_income_updates_totals_and_rejects_stale_versions() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
        let account = ledger.accounts().unwrap().remove(0).id;
        let id = ledger
            .write_transaction(request(&account, "income", "25.00"))
            .unwrap();
        let mut edit = request(&account, "income", "30.00");
        edit.edit.id = id.clone();
        ledger.write_transaction(edit).unwrap();
        assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
        let mut stale = request(&account, "income", "40.00");
        stale.edit.id = id;
        assert!(ledger.write_transaction(stale).is_err());
        assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
    }

    #[test]
    fn reversal_updates_day_totals_and_rejects_new_refunds() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
        let account = ledger.accounts().unwrap().remove(0).id;
        let today = chrono::Local::now().date_naive().to_string();
        let mut expense = request(&account, "expense", "25.00");
        expense.edit.date = today.clone();
        let id = ledger.write_transaction(expense).unwrap();
        ledger.reverse_transaction(&id, "重复记账").unwrap();
        assert_eq!(ledger.day(&account, &today).unwrap().expense, "0.00");
        let mut refund = request(&account, "refund", "5.00");
        refund.edit.original_transaction_id = Some(id);
        assert!(ledger.write_transaction(refund).is_err());
    }
}
