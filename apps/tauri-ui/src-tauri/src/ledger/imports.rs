use super::*;

#[derive(Serialize)]
pub struct BillPreview {
    pub preview_id: String,
    pub rows: Vec<BillRow>,
    pub errors: Vec<BillIssue>,
    pub new_count: usize,
    pub exact_duplicates: usize,
    pub suspected_duplicates: usize,
    pub pending_count: usize,
}
#[derive(Serialize)]
pub struct BillIssue {
    pub row: usize,
    pub message: String,
}
#[derive(Serialize)]
pub struct BillRow {
    pub id: String,
    pub row: usize,
    pub date: String,
    pub amount: String,
    pub kind: String,
    pub transfer_direction: Option<String>,
    pub merchant: String,
    pub description: String,
    pub decision: String,
    pub duplicate_id: Option<String>,
    pub warnings: Vec<String>,
    pub transaction_number: Option<String>,
    pub merchant_order_id: Option<String>,
    pub account_hint: Option<String>,
}
pub(super) struct PreviewSession {
    pub(super) id: uuid::Uuid,
    parsed: ledger_import::ParsedImport,
    pub(super) rows: Vec<(
        String,
        ledger_import::NormalizedImportRow,
        ledger_import::PreviewDecision,
    )>,
}
impl Ledger {
    pub fn bill_preview(&self, provider: &str, bytes: Vec<u8>) -> Result<BillPreview> {
        self.bill_preview_for_account(provider, bytes, None)
    }
    pub fn bill_preview_for_account(
        &self,
        provider: &str,
        bytes: Vec<u8>,
        account: Option<&str>,
    ) -> Result<BillPreview> {
        self.bill_preview_with_password(provider, bytes, account, None)
    }

    pub fn bill_preview_with_password(
        &self,
        provider: &str,
        bytes: Vec<u8>,
        account: Option<&str>,
        password: Option<&str>,
    ) -> Result<BillPreview> {
        if bytes.len() > 5 * 1024 * 1024 {
            return Err("账单文件不能超过 5 MiB".into());
        }
        let provider = match provider {
            "wechat" => ledger_import::Provider::Wechat,
            "alipay" => ledger_import::Provider::Alipay,
            "bank" => ledger_import::Provider::Bank,
            _ => return Err("请选择账单来源".into()),
        };
        let parsed = ledger_import::parse_bill_with_password(provider, &bytes, password)
            .map_err(|error| error.to_string())?;
        self.preview_parsed(parsed, account)
    }

    pub fn repreview_bill(&self, preview_id: &str, account: Option<&str>) -> Result<BillPreview> {
        let parsed = self
            .preview
            .borrow()
            .as_ref()
            .filter(|session| session.id.to_string() == preview_id)
            .map(|session| session.parsed.clone())
            .ok_or("导入预览已失效，请重新选择账单文件")?;
        self.preview_parsed(parsed, account)
    }

    fn preview_parsed(
        &self,
        parsed: ledger_import::ParsedImport,
        account: Option<&str>,
    ) -> Result<BillPreview> {
        use ledger_app::FeatureRepository;
        let target = account
            .filter(|id| !id.is_empty())
            .map(account_id)
            .transpose()?;
        let repo = self.repo()?;
        let mut existing = Vec::new();
        let mut seen_transactions = std::collections::HashSet::new();
        let mut queried_days = std::collections::HashSet::new();
        for row in &parsed.rows {
            // Load each local day and account scope once, including all pages.
            // Stable source IDs are looked up globally, independently of dates.
            let day = row.occurred_at.with_timezone(&Local).date_naive();
            let scope = if row.kind == TransactionKind::Transfer {
                None
            } else {
                target
            };
            if queried_days.insert((day, scope)) {
                let midnight = |date: NaiveDate| {
                    Local
                        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                        .earliest()
                        .map(|at| at.with_timezone(&Utc))
                        .ok_or("日期本地边界不可用")
                };
                let from = midnight(day)?;
                let until = midnight(day.succ_opt().ok_or("日期超出范围")?)?;
                let mut offset = 0;
                loop {
                    let page = repo
                        .query_transactions(ledger_domain::PendingQuery {
                            from_utc_inclusive: Some(from),
                            until_utc_exclusive: Some(until),
                            offset,
                            limit: 200,
                            account_id: scope,
                            ..Default::default()
                        })
                        .map_err(|error| error.to_string())?;
                    let count = page.items.len() as u32;
                    for candidate in page.items {
                        let tx = candidate.transaction;
                        if !seen_transactions.insert(tx.id) {
                            continue;
                        }
                        if candidate.metadata.reversal_of.is_some()
                            || repo
                                .related_transactions(tx.id)
                                .map_err(|e| e.to_string())?
                                .iter()
                                .any(|(_, reversal)| *reversal)
                        {
                            continue;
                        }
                        if let Some(amount) = tx
                            .report_amount_cny
                            .as_ref()
                            .or_else(|| tx.postings.first().map(|p| &p.delta))
                        {
                            let evidence = repo
                                .source_evidence(tx.id)
                                .map_err(|error| error.to_string())?;
                            existing.push(ledger_import::ExistingImport {
                                transaction_id: Some(tx.id),
                                source_kind: ledger_domain::SourceKind::Manual,
                                external_transaction_id: None,
                                occurred_at: tx.occurred_at,
                                kind: tx.kind,
                                amount_minor: amount.minor_units().abs(),
                                merchant: tx.merchant.clone(),

                                merchant_order_id: None,
                                source_fields_digest: None,
                                description: tx.description.clone(),
                                date_only: evidence.is_empty()
                                    && tx.occurred_at.with_timezone(&Local).time()
                                        == chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
                                account_ids: tx.postings.iter().map(|p| p.account_id).collect(),
                            });
                            for evidence in evidence {
                                existing.push(ledger_import::ExistingImport {
                                    transaction_id: Some(tx.id),
                                    source_kind: evidence.source_kind,
                                    external_transaction_id: evidence.external_transaction_id,
                                    occurred_at: tx.occurred_at,
                                    kind: tx.kind,
                                    amount_minor: amount.minor_units().abs(),
                                    merchant: tx.merchant.clone(),

                                    merchant_order_id: evidence.merchant_order_id,
                                    source_fields_digest: Some(evidence.source_fields_digest),
                                    description: tx.description.clone(),
                                    date_only: false,
                                    account_ids: tx.postings.iter().map(|p| p.account_id).collect(),
                                });
                            }
                        }
                    }
                    offset = offset.checked_add(count).ok_or("候选流水数量超出范围")?;
                    if count == 0 || u64::from(offset) >= page.total {
                        break;
                    }
                }
            }
            if let Some(external_id) = &row.external_transaction_id {
                use ledger_app::ImportRepository;
                if let Some(id) = repo
                    .find_source_transaction(row.source_kind, external_id)
                    .map_err(|error| error.to_string())?
                    && !existing.iter().any(|item| {
                        item.source_kind == row.source_kind
                            && item.external_transaction_id.as_deref() == Some(external_id)
                    })
                    && let Some(tx) = repo
                        .get_transaction(id)
                        .map_err(|error| error.to_string())?
                {
                    existing.push(ledger_import::ExistingImport {
                        transaction_id: Some(id),
                        source_kind: row.source_kind,
                        external_transaction_id: Some(external_id.clone()),
                        occurred_at: tx.occurred_at,
                        kind: tx.kind,
                        amount_minor: tx
                            .report_amount_cny
                            .as_ref()
                            .map(Money::minor_units)
                            .unwrap_or_default(),
                        merchant: tx.merchant,

                        merchant_order_id: None,
                        source_fields_digest: None,
                        description: tx.description,
                        date_only: false,
                        account_ids: tx.postings.iter().map(|p| p.account_id).collect(),
                    });
                }
            }
        }
        let preview =
            ledger_import::ImportPreview::build_for_account(&parsed.rows, &existing, target);
        let preview_id = uuid::Uuid::new_v4();
        let rows = preview
            .rows
            .iter()
            .map(|item| {
                let id = uuid::Uuid::new_v4().to_string();
                (id, item.row.clone(), item.decision.clone())
            })
            .collect::<Vec<_>>();
        let result = BillPreview {
            preview_id: preview_id.to_string(),
            rows: rows
                .iter()
                .map(|(id, row, decision)| {
                    let (status, duplicate_id) = match decision {
                        ledger_import::PreviewDecision::New => ("new", None),
                        ledger_import::PreviewDecision::ExactDuplicate { transaction_id } => (
                            "exact_duplicate",
                            transaction_id.map(|id| id.as_uuid().to_string()),
                        ),
                        ledger_import::PreviewDecision::SuspectedDuplicate { transaction_id } => (
                            "suspected_duplicate",
                            transaction_id.map(|id| id.as_uuid().to_string()),
                        ),
                    };
                    BillRow {
                        id: id.clone(),
                        row: row.row_number,
                        date: row
                            .occurred_at
                            .with_timezone(&Local)
                            .date_naive()
                            .to_string(),
                        amount: Money::new(row.amount_minor, row.currency.clone()).decimal_string(),
                        kind: kind_label(row.kind).into(),
                        transfer_direction: row.transfer_direction.map(
                            |direction| match direction {
                                ledger_import::TransferDirection::Outgoing => "outgoing".into(),
                                ledger_import::TransferDirection::Incoming => "incoming".into(),
                            },
                        ),
                        merchant: row.merchant.clone().unwrap_or_default(),
                        description: row.description.clone().unwrap_or_default(),
                        decision: status.into(),
                        transaction_number: row.external_transaction_id.clone(),
                        merchant_order_id: row.merchant_order_id.clone(),
                        account_hint: row.account_hint.clone(),
                        duplicate_id,
                        warnings: row
                            .warnings
                            .iter()
                            .map(|warning| warning.message.clone())
                            .collect(),
                    }
                })
                .collect(),
            errors: parsed
                .errors
                .iter()
                .chain(parsed.warnings.iter())
                .map(|issue| BillIssue {
                    row: issue.row,
                    message: issue.message.clone(),
                })
                .collect(),
            new_count: preview.counts.new,
            exact_duplicates: preview.counts.exact_duplicates,
            suspected_duplicates: preview.counts.suspected_duplicates,
            pending_count: preview.counts.pending,
        };
        self.preview.replace(Some(PreviewSession {
            id: preview_id,
            parsed,
            rows,
        }));
        Ok(result)
    }
}
