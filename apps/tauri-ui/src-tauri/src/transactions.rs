//! Typed transaction detail and editing boundary. All writes use application services.
use crate::ledger::{self, Ledger, PendingEdit};
use chrono::{TimeZone, Utc};
use ledger_app::{FeatureRepository, ImportRepository, LedgerRepository, TransactionService};
use ledger_domain::{
    AccountKind, Posting, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, String>;
#[derive(Deserialize)]
pub struct WriteTransaction {
    pub edit: PendingEdit,
    pub destination_id: Option<String>,
    pub source: Option<String>,
    pub transaction_number: Option<String>,
    pub merchant_order_id: Option<String>,
    pub time: Option<String>,
    pub manual_fx_rate: Option<String>,
}
#[derive(Serialize)]
pub struct ForeignPreview {
    pub amount: String,
    pub currency_code: String,
    pub report_amount_cny: String,
    pub rate: String,
    pub source: String,
    pub effective_date: String,
    pub fx_snapshot: FxSnapshotView,
}
impl Ledger {
    pub fn preview_foreign_transaction(
        &self,
        amount: &str,
        quote: &str,
        date: &str,
        manual_rate: Option<&str>,
    ) -> Result<ForeignPreview> {
        let definition = ledger_domain::supported_currencies()
            .iter()
            .find(|c| c.code == quote && c.code != "CNY")
            .ok_or("不支持的外币")?;
        let currency = ledger_domain::CurrencyCode::new(quote, definition.exponent)
            .map_err(|e| e.to_string())?;
        let minor = ledger::parse_currency_amount(amount, false, definition.exponent)?;
        let (report, snapshot) =
            self.convert_amount(minor, &currency, ledger::date(date)?, manual_rate)?;
        let fx_snapshot = FxSnapshotView::from(snapshot.as_ref().ok_or("缺少汇率")?);
        Ok(ForeignPreview {
            amount: ledger_domain::Money::new(minor, currency).decimal_string(),
            currency_code: quote.into(),
            report_amount_cny: report.decimal_string(),
            rate: fx_snapshot.rate.clone(),
            source: fx_snapshot.source.clone(),
            effective_date: fx_snapshot.effective_date.clone(),
            fx_snapshot,
        })
    }
}
#[derive(Serialize)]
pub struct Detail {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub date: String,
    pub time: String,
    pub amount: String,
    pub currency_code: String,
    pub report_amount_cny: Option<String>,
    pub fx_snapshot: Option<FxSnapshotView>,
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
pub struct FxSnapshotView {
    pub rate: String,
    pub effective_date: String,
    pub source: String,
    pub provider: Option<String>,
    pub provider_sources: Vec<String>,
}
impl From<&ledger_domain::TransactionFxSnapshot> for FxSnapshotView {
    fn from(value: &ledger_domain::TransactionFxSnapshot) -> Self {
        Self {
            rate: value.rate.clone(),
            effective_date: value.effective_date.to_string(),
            source: match value.source {
                ledger_domain::FxSnapshotSource::Reference => "reference",
                ledger_domain::FxSnapshotSource::Manual => "manual",
            }
            .into(),
            provider: value.provider.clone(),
            provider_sources: value.provider_sources.clone(),
        }
    }
}
#[derive(Serialize)]
pub struct SourceDetail {
    pub source: String,
    pub external_id: Option<String>,
    pub merchant_order_id: Option<String>,
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
            time: t
                .occurred_at
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string(),
            amount: t
                .postings
                .first()
                .map(|p| p.delta.decimal_string().trim_start_matches('-').to_string())
                .unwrap_or_default(),
            currency_code: t
                .postings
                .first()
                .map(|p| p.delta.currency().code().to_string())
                .unwrap_or_else(|| "CNY".into()),
            report_amount_cny: t.report_amount_cny.as_ref().map(|m| m.decimal_string()),
            fx_snapshot: t.fx_snapshot.as_ref().map(FxSnapshotView::from),
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
                    merchant_order_id: e.merchant_order_id.clone(),
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
        if e.id.is_empty() && e.kind == "expense" && e.merchant.trim().is_empty() {
            return Err("请填写实际消费商户，便于以后与账单核对".into());
        }
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
            fx_snapshot: None,
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
            let source = self.account(&e.account_id)?;
            if source.currency != destination.currency {
                return Err("转账平台币种必须相同".into());
            }
            let amount = ledger::parse_currency_amount(
                &e.amount,
                false,
                destination.currency.minor_unit_exponent(),
            )?;
            transaction.kind = TransactionKind::Transfer;
            transaction.report_amount_cny = None;
            transaction.fx_snapshot = None;
            transaction.original_transaction_id = None;
            transaction.postings.push(Posting {
                account_id: destination.id,
                delta: ledger_domain::Money::new(
                    if destination.kind == AccountKind::Asset {
                        amount
                    } else {
                        -amount
                    },
                    destination.currency.clone(),
                ),
            });
        } else {
            transaction = self.apply_pending_edit(transaction, &e)?;
        }
        if transaction.kind != TransactionKind::Transfer {
            let account = self.account(&e.account_id)?;
            let amount = ledger::parse_currency_amount(
                &e.amount,
                false,
                account.currency.minor_unit_exponent(),
            )?;
            let unchanged_foreign = existing.as_ref().is_some_and(|prior| {
                prior.fx_snapshot.is_some()
                    && prior.report_amount_cny.is_some()
                    && prior.kind == transaction.kind
                    && prior.occurred_at.with_timezone(&chrono::Local).date_naive()
                        == transaction
                            .occurred_at
                            .with_timezone(&chrono::Local)
                            .date_naive()
                    && prior.postings.first().is_some_and(|posting| {
                        posting.delta.currency() == &account.currency
                            && posting.delta.minor_units().unsigned_abs() == amount as u64
                    })
            });
            if unchanged_foreign {
                let prior = existing.as_ref().expect("checked above");
                transaction.report_amount_cny = prior.report_amount_cny.clone();
                transaction.fx_snapshot = prior.fx_snapshot.clone();
            } else {
                let (report, snapshot) = self.convert_amount(
                    amount,
                    &account.currency,
                    ledger::date(&e.date)?,
                    request.manual_fx_rate.as_deref(),
                )?;
                transaction.report_amount_cny = Some(report);
                transaction.fx_snapshot = snapshot;
            }
        }
        if let Some(value) = request.time.as_deref().filter(|v| !v.is_empty()) {
            let clock = chrono::NaiveTime::parse_from_str(value, "%H:%M:%S")
                .or_else(|_| chrono::NaiveTime::parse_from_str(value, "%H:%M"))
                .map_err(|_| "交易时间格式无效")?;
            transaction.occurred_at = chrono::Local
                .from_local_datetime(&ledger::date(&e.date)?.and_time(clock))
                .single()
                .ok_or("该交易时间本地不可用")?
                .with_timezone(&Utc);
        }
        if transaction.kind == TransactionKind::Refund {
            if let Some(parent_id) = transaction.original_transaction_id {
                let parent = repo
                    .get_transaction(parent_id)
                    .map_err(|e| e.to_string())?
                    .ok_or("原消费不存在")?;
                let parent_currency = parent.postings.first().map(|p| p.delta.currency());
                let refund_currency = transaction.postings.first().map(|p| p.delta.currency());
                if parent_currency != refund_currency {
                    return Err("退款币种须与原消费一致".into());
                }
            }
        }
        let number = request
            .transaction_number
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_owned());
        let merchant_order = request
            .merchant_order_id
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_owned());
        if number.iter().chain(merchant_order.iter()).any(|v| {
            v.chars().count() > 200
                || v.chars().any(char::is_whitespace)
                || v.chars().any(char::is_control)
        }) {
            return Err("单号最多 200 字且不能包含空白字符".into());
        }
        let evidence = if number.is_some() || merchant_order.is_some() {
            if existing.is_some() {
                return Err("已有记录的来源单号不可改写".into());
            }
            let source = match request.source.as_deref() {
                Some("wechat") => ledger_domain::SourceKind::WeChat,
                Some("alipay") => ledger_domain::SourceKind::Alipay,
                Some("bank") => ledger_domain::SourceKind::Bank,
                _ => return Err("填写单号时请选择支付渠道".into()),
            };
            if let Some(number) = number.as_deref()
                && repo
                    .find_source_transaction(source, number)
                    .map_err(|e| e.to_string())?
                    .is_some()
            {
                return Err("该渠道交易单号已存在，请查看已有记录".into());
            }
            Some(ledger_domain::SourceEvidence {
                source_kind: source,
                source_fields_digest: ledger_import::NormalizedImportRow::digest_for(
                    // Manual provenance must never equal a statement digest:
                    // only its channel ID can establish an exact import match.
                    ledger_domain::SourceKind::Manual,
                    number.as_deref(),
                    transaction.occurred_at,
                    transaction.kind,
                    ledger::parse_currency_amount(
                        &e.amount,
                        false,
                        transaction.postings[0]
                            .delta
                            .currency()
                            .minor_unit_exponent(),
                    )?,
                    transaction.postings[0].delta.currency(),
                    transaction.merchant.as_deref(),
                    transaction.description.as_deref(),
                ),
                external_transaction_id: number,
                merchant_order_id: merchant_order,
                original_external_transaction_id: None,
                import_batch_id: uuid::Uuid::new_v4(),
            })
        } else {
            None
        };
        let id = transaction.id.as_uuid().to_string();
        let service = TransactionService::new(repo);
        if existing.is_some() {
            service
                .edit(ledger_domain::TransactionEdit {
                    transaction,
                    expected_version: e.expected_version,
                })
                .map_err(|e| e.to_string())?;
        } else if let Some(evidence) = evidence {
            service
                .confirm_with_evidence(transaction, evidence)
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
