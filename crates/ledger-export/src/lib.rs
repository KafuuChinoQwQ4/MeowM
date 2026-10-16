//! Stable, read-only exports; only explicitly requested transaction fields leave the ledger.
mod csv;
use ledger_app::{FeatureRepository, LedgerRepository, RepositoryError};
use ledger_domain::{CurrencyCode, Money, PendingQuery, TransactionKind, TransactionStatus};
use ledger_storage::sqlite::SqliteLedgerRepository;
use serde::Serialize;
#[derive(Clone, Copy)]
pub enum ExportFormat {
    Csv { bom: bool },
    Json,
}
#[derive(Serialize)]
pub struct ExportDocument {
    pub format_version: u32,
    pub exported_at: String,
    pub transactions: Vec<ExportRow>,
}
#[derive(Serialize)]
pub struct ExportRow {
    pub id: String,
    pub occurred_at: String,
    pub kind: String,
    pub status: String,
    pub amount_cny: Option<String>,
    pub income_cny: String,
    pub expense_cny: String,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
    pub reversal_of: Option<String>,
    pub postings: Vec<ExportPosting>,
    pub sources: Vec<String>,
}
#[derive(Serialize)]
pub struct ExportPosting {
    pub account_id: String,
    pub account_name: String,
    pub amount: String,
    pub currency: String,
}
pub struct ExportService;
impl ExportService {
    pub fn export(
        repo: &SqliteLedgerRepository,
        mut query: PendingQuery,
        include_unconfirmed: bool,
        format: ExportFormat,
    ) -> Result<(Vec<u8>, usize), RepositoryError> {
        if !include_unconfirmed {
            query.status = Some(TransactionStatus::Confirmed);
        }
        // A frozen copy keeps all pages, postings, and sources in one consistent revision.
        let snapshot = repo.snapshot()?;
        let frozen = SqliteLedgerRepository::from_snapshot(&snapshot)?;
        let accounts = frozen.list_accounts()?;
        query.offset = 0;
        query.limit = 200;
        let mut rows = Vec::new();
        loop {
            let page = frozen.query_transactions(query.clone())?;
            for item in page.items {
                let t = item.transaction;
                let sources = frozen
                    .source_evidence(t.id)?
                    .iter()
                    .map(|e| format!("{:?}", e.source_kind).to_lowercase())
                    .collect();
                let effect = item
                    .metadata
                    .report_override
                    .or_else(|| ledger_domain::report_effect(&t));
                let decimal = |v| {
                    Money::new(v, CurrencyCode::new("CNY", 2).expect("constant currency"))
                        .decimal_string()
                };
                let postings = t
                    .postings
                    .iter()
                    .map(|p| ExportPosting {
                        account_id: p.account_id.as_uuid().to_string(),
                        account_name: accounts
                            .iter()
                            .find(|a| a.id == p.account_id)
                            .map(|a| a.display_name.clone())
                            .unwrap_or_default(),
                        amount: p.delta.decimal_string(),
                        currency: p.delta.currency().code().into(),
                    })
                    .collect();
                rows.push(ExportRow {
                    id: t.id.as_uuid().to_string(),
                    occurred_at: t.occurred_at.to_rfc3339(),
                    kind: match t.kind {
                        TransactionKind::Income => "income",
                        TransactionKind::Expense => "expense",
                        TransactionKind::Transfer => "transfer",
                        TransactionKind::Refund => "refund",
                        TransactionKind::BalanceAdjustment => "balance_adjustment",
                    }
                    .into(),
                    status: match t.status {
                        TransactionStatus::Confirmed => "confirmed",
                        TransactionStatus::Pending => "pending",
                        TransactionStatus::Ignored => "ignored",
                    }
                    .into(),
                    amount_cny: t.report_amount_cny.map(|v| v.decimal_string()),
                    income_cny: decimal(effect.map_or(0, |e| e.income_cny_minor)),
                    expense_cny: decimal(effect.map_or(0, |e| e.expense_cny_minor)),
                    merchant: t.merchant,
                    description: t.description,
                    category_id: t.category_id.map(|v| v.as_uuid().to_string()),
                    original_transaction_id: t
                        .original_transaction_id
                        .map(|v| v.as_uuid().to_string()),
                    reversal_of: item.metadata.reversal_of.map(|v| v.as_uuid().to_string()),
                    postings,
                    sources,
                });
            }
            query.offset += query.limit;
            if u64::from(query.offset) >= page.total {
                break;
            }
        }
        let count = rows.len();
        let bytes = match format {
            ExportFormat::Csv { bom } => csv::encode(&rows, bom)?,
            ExportFormat::Json => serde_json::to_vec_pretty(&ExportDocument {
                format_version: 1,
                exported_at: chrono::Utc::now().to_rfc3339(),
                transactions: rows,
            })
            .map_err(|_| RepositoryError::Storage("export encoding failed".into()))?,
        };
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(RepositoryError::InvalidData(
                "export exceeds 32 MB; narrow the filters".into(),
            ));
        }
        Ok((bytes, count))
    }
}
