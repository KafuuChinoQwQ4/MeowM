use super::*;

#[derive(serde::Deserialize)]
pub struct PageQuery {
    pub status: Option<String>,
    pub source: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub from_date: Option<String>,
    pub until_date: Option<String>,
    pub merchant: Option<String>,
    pub keyword: Option<String>,
    pub min_amount: Option<String>,
    pub max_amount: Option<String>,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Serialize)]
pub struct PageEntry {
    pub id: String,
    pub status: String,
    pub kind: String,
    pub occurred_at: String,
    pub amount: Option<String>,
    pub merchant: Option<String>,
    pub description: Option<String>,
    pub edit_version: u32,
    pub reversal_of: Option<String>,
    pub account_id: Option<String>,
    pub destination_id: Option<String>,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
    pub sources: Vec<String>,
    pub suggestion: Option<SuggestionView>,
}
#[derive(Serialize)]
pub struct SuggestionView {
    pub category_id: String,
    pub rule_id: String,
    pub reason: String,
}
#[derive(Serialize)]
pub struct EntryPage {
    pub items: Vec<PageEntry>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}
impl CommandError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".into(),
            message: message.into(),
        }
    }
}
impl Ledger {
    pub fn page(&self, q: PageQuery) -> std::result::Result<EntryPage, CommandError> {
        use ledger_app::FeatureRepository;
        let midnight = |v: &str| -> Result<chrono::DateTime<Utc>> {
            let d = date(v)?;
            Local
                .from_local_datetime(&d.and_hms_opt(0, 0, 0).ok_or("日期超出范围")?)
                .earliest()
                .map(|t| t.with_timezone(&Utc))
                .ok_or("日期本地边界不可用".into())
        };
        let status = match q.status.as_deref() {
            None => None,
            Some("pending") => Some(TransactionStatus::Pending),
            Some("confirmed") => Some(TransactionStatus::Confirmed),
            Some("ignored") => Some(TransactionStatus::Ignored),
            _ => return Err(CommandError::invalid("无效的流水状态")),
        };
        let query = ledger_domain::PendingQuery {
            status,
            source_kind: q
                .source
                .as_deref()
                .map(parse_source)
                .transpose()
                .map_err(CommandError::invalid)?,
            account_id: q
                .account_id
                .as_deref()
                .map(account_id)
                .transpose()
                .map_err(CommandError::invalid)?,
            category_id: q
                .category_id
                .as_deref()
                .map(|v| {
                    uuid::Uuid::parse_str(v)
                        .map(ledger_domain::CategoryId::from_uuid)
                        .map_err(|_| CommandError::invalid("无效的分类标识"))
                })
                .transpose()?,
            from_utc_inclusive: q
                .from_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            until_utc_exclusive: q
                .until_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            merchant: q.merchant,
            keyword: q.keyword,
            min_amount_cny_minor: q
                .min_amount
                .as_deref()
                .map(search_amount)
                .transpose()
                .map_err(CommandError::invalid)?,
            max_amount_cny_minor: q
                .max_amount
                .as_deref()
                .map(search_amount)
                .transpose()
                .map_err(CommandError::invalid)?,
            offset: q.offset,
            limit: q.limit,
        };
        let repo = self.repo().map_err(|_| CommandError {
            code: "storage_error".into(),
            message: "无法打开本地账本".into(),
        })?;
        let page = repo.query_transactions(query).map_err(|e| CommandError {
            code: match e {
                ledger_app::RepositoryError::InvalidData(_) => "invalid_input",
                ledger_app::RepositoryError::NotFound => "not_found",
                ledger_app::RepositoryError::Conflict => "conflict",
                _ => "storage_error",
            }
            .into(),
            message: "无法查询流水，请检查筛选条件或重试".into(),
        })?;
        Ok(EntryPage {
            total: page.total,
            offset: page.offset,
            limit: page.limit,
            items: page
                .items
                .into_iter()
                .map(|c| {
                    let t = c.transaction;
                    let suggestion = if t.status == TransactionStatus::Pending {
                        repo.suggest_pending(t.id)
                            .map_err(|_| CommandError {
                                code: "storage_error".into(),
                                message: "无法读取分类建议".into(),
                            })?
                            .map(|s| SuggestionView {
                                category_id: s.category_id.as_uuid().to_string(),
                                rule_id: s.rule_id.as_uuid().to_string(),
                                reason: s.reason,
                            })
                    } else {
                        None
                    };
                    let sources = repo.source_evidence(t.id).map_err(|_| CommandError {
                        code: "storage_error".into(),
                        message: "无法读取流水来源，请重试".into(),
                    })?;
                    Ok::<PageEntry, CommandError>(PageEntry {
                        id: t.id.as_uuid().to_string(),
                        status: match t.status {
                            TransactionStatus::Pending => "pending",
                            TransactionStatus::Confirmed => "confirmed",
                            TransactionStatus::Ignored => "ignored",
                        }
                        .into(),
                        kind: kind_label(t.kind).into(),
                        occurred_at: t.occurred_at.to_rfc3339(),
                        amount: t.report_amount_cny.map(|v| v.decimal_string()),
                        merchant: t.merchant,
                        description: t.description,
                        edit_version: c.metadata.edit_version,
                        reversal_of: c.metadata.reversal_of.map(|v| v.as_uuid().to_string()),
                        account_id: t
                            .postings
                            .first()
                            .map(|p| p.account_id.as_uuid().to_string()),
                        destination_id: if t.kind == TransactionKind::Transfer {
                            t.postings
                                .get(1)
                                .map(|p| p.account_id.as_uuid().to_string())
                        } else {
                            None
                        },
                        category_id: t.category_id.map(|id| id.as_uuid().to_string()),
                        original_transaction_id: t
                            .original_transaction_id
                            .map(|id| id.as_uuid().to_string()),
                        sources: sources
                            .iter()
                            .map(|e| source_label(e.source_kind).into())
                            .collect(),
                        suggestion,
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?,
        })
    }
}

fn search_amount(value: &str) -> Result<i64> {
    if value.trim().starts_with('-') {
        return Err("金额范围不能为负".into());
    }
    parse_amount(value, true)
}
