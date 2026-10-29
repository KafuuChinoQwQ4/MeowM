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
    pub reversal_id: Option<String>,
    pub account_id: Option<String>,
    pub destination_id: Option<String>,
    pub category_id: Option<String>,
    pub original_transaction_id: Option<String>,
    pub refund_matches: Vec<super::refunds::RefundMatch>,
    pub sources: Vec<String>,
    pub external_ids: Vec<ExternalId>,
    pub merchant_order_ids: Vec<ExternalId>,
    pub suggestion: Option<SuggestionView>,
}
#[derive(Serialize)]
pub struct ExternalId {
    pub source: String,
    pub id: String,
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
impl PageQuery {
    pub(crate) fn into_domain(
        self,
    ) -> std::result::Result<ledger_domain::PendingQuery, CommandError> {
        let midnight = |v: &str| -> Result<chrono::DateTime<Utc>> {
            let d = date(v)?;
            Local
                .from_local_datetime(&d.and_hms_opt(0, 0, 0).ok_or("日期超出范围")?)
                .earliest()
                .map(|t| t.with_timezone(&Utc))
                .ok_or("日期本地边界不可用".into())
        };
        let status = match self.status.as_deref() {
            None => None,
            Some("pending") => Some(TransactionStatus::Pending),
            Some("confirmed") => Some(TransactionStatus::Confirmed),
            Some("ignored") => Some(TransactionStatus::Ignored),
            _ => return Err(CommandError::invalid("无效的流水状态")),
        };
        Ok(ledger_domain::PendingQuery {
            status,
            source_kind: self
                .source
                .as_deref()
                .map(parse_source)
                .transpose()
                .map_err(CommandError::invalid)?,
            account_id: self
                .account_id
                .as_deref()
                .map(account_id)
                .transpose()
                .map_err(CommandError::invalid)?,
            category_id: self
                .category_id
                .as_deref()
                .map(|v| {
                    uuid::Uuid::parse_str(v)
                        .map(ledger_domain::CategoryId::from_uuid)
                        .map_err(|_| CommandError::invalid("无效的分类标识"))
                })
                .transpose()?,
            from_utc_inclusive: self
                .from_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            until_utc_exclusive: self
                .until_date
                .as_deref()
                .map(midnight)
                .transpose()
                .map_err(CommandError::invalid)?,
            merchant: self.merchant,
            keyword: self.keyword,
            min_amount_cny_minor: self
                .min_amount
                .as_deref()
                .map(search_amount)
                .transpose()
                .map_err(CommandError::invalid)?,
            max_amount_cny_minor: self
                .max_amount
                .as_deref()
                .map(search_amount)
                .transpose()
                .map_err(CommandError::invalid)?,
            offset: self.offset,
            limit: self.limit,
        })
    }
}
impl Ledger {
    pub fn page(&self, q: PageQuery) -> std::result::Result<EntryPage, CommandError> {
        use ledger_app::FeatureRepository;
        let query = q.into_domain()?;
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
        let refund_matcher = if page.items.iter().any(|c| {
            c.transaction.kind == TransactionKind::Refund
                && c.transaction.status == TransactionStatus::Pending
                && c.transaction.original_transaction_id.is_none()
        }) {
            Some(
                super::refunds::RefundMatcher::load(&repo, &[]).map_err(|_| CommandError {
                    code: "storage_error".into(),
                    message: "无法读取退款匹配建议，请重试".into(),
                })?,
            )
        } else {
            None
        };
        Ok(EntryPage {
            total: page.total,
            offset: page.offset,
            limit: page.limit,
            items: page
                .items
                .into_iter()
                .map(|c| {
                    let t = c.transaction;
                    let refund_matches = refund_matcher
                        .as_ref()
                        .map(|matcher| matcher.matches(&t))
                        .transpose()
                        .map_err(|_| CommandError {
                            code: "storage_error".into(),
                            message: "无法读取退款匹配建议，请重试".into(),
                        })?
                        .unwrap_or_default();
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
                    let reversal_id = if t.status == TransactionStatus::Confirmed {
                        repo.related_transactions(t.id)
                            .map_err(|_| CommandError {
                                code: "storage_error".into(),
                                message: "无法读取流水撤销状态，请重试".into(),
                            })?
                            .into_iter()
                            .find_map(|(id, is_reversal)| {
                                is_reversal.then(|| id.as_uuid().to_string())
                            })
                    } else {
                        None
                    };
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
                        amount: t
                            .report_amount_cny
                            .as_ref()
                            .or_else(|| t.postings.first().map(|p| &p.delta))
                            .map(|v| v.decimal_string().trim_start_matches('-').to_string()),
                        merchant: t.merchant,
                        description: t.description,
                        edit_version: c.metadata.edit_version,
                        reversal_of: c.metadata.reversal_of.map(|v| v.as_uuid().to_string()),
                        reversal_id,
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
                        external_ids: sources
                            .iter()
                            .filter_map(|e| {
                                e.external_transaction_id.as_ref().map(|id| ExternalId {
                                    source: source_label(e.source_kind).into(),
                                    id: id.clone(),
                                })
                            })
                            .collect(),
                        merchant_order_ids: sources
                            .iter()
                            .filter_map(|e| {
                                e.merchant_order_id.as_ref().map(|id| ExternalId {
                                    source: source_label(e.source_kind).into(),
                                    id: id.clone(),
                                })
                            })
                            .collect(),
                        suggestion,
                        refund_matches,
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
