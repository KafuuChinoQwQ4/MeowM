//! Offline bill-file decoding, preview, normalization, and reconciliation.

pub mod alipay;
mod csv;
mod dedupe;
mod normalize;
pub mod preview;
pub mod wechat;

use chrono::{DateTime, Utc};
use ledger_domain::SourceKind;
use std::collections::HashSet;

pub use alipay::parse_alipay_csv;
pub use dedupe::{DuplicateMatch, ExistingImport, classify_duplicate};
pub use normalize::{ImportError, ImportWarning, NormalizedImportRow};
pub use preview::{ImportPreview, PreviewCounts, PreviewDecision, PreviewRow};
pub use wechat::parse_wechat_csv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Wechat,
    Alipay,
    Bank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedImport {
    pub source_kind: SourceKind,
    pub rows: Vec<NormalizedImportRow>,
    pub warnings: Vec<ImportWarning>,
    /// Row validation errors are displayed in preview and cannot be selected.
    pub errors: Vec<ImportWarning>,
}

impl ParsedImport {
    pub fn pending_count(&self) -> usize {
        self.rows.len()
    }
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
            + self
                .rows
                .iter()
                .map(|row| row.warnings.len())
                .sum::<usize>()
    }
}

pub fn parse_bank_csv(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    parse_provider_csv(bytes, SourceKind::Bank, Provider::Bank)
}

pub(crate) fn parse_provider_csv(
    bytes: &[u8],
    source_kind: SourceKind,
    provider: Provider,
) -> Result<ParsedImport, ImportError> {
    let rows = csv::decode_csv(bytes)?;
    let header_index = rows
        .iter()
        .position(|row| {
            let map = csv::header_map(row);
            ["交易时间", "日期", "交易日期", "记账日期", "付款时间"]
                .iter()
                .any(|key| map.contains_key(&csv::normalize_header(key)))
        })
        .unwrap_or(0);
    let headers = csv::header_map(&rows[header_index]);
    let date_aliases = match provider {
        Provider::Wechat | Provider::Alipay => vec!["交易时间", "时间", "日期", "付款时间"],
        Provider::Bank => vec!["交易日期", "日期", "记账日期", "交易时间"],
    };
    let amount_aliases = match provider {
        Provider::Wechat | Provider::Alipay => vec!["金额", "交易金额", "金额(元)"],
        Provider::Bank => vec!["交易金额", "金额", "发生额", "借方发生额", "贷方发生额"],
    };
    let has_date = date_aliases
        .iter()
        .any(|alias| headers.contains_key(&csv::normalize_header(alias)));
    let has_amount = amount_aliases
        .iter()
        .any(|alias| headers.contains_key(&csv::normalize_header(alias)));
    if !has_date || !has_amount {
        let mut missing = Vec::new();
        if !has_date {
            missing.push("日期".to_owned());
        }
        if !has_amount {
            missing.push("金额".to_owned());
        }
        return Err(ImportError::MissingHeaders(missing));
    }
    let mut parsed = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut seen_ids = HashSet::new();
    for (index, row) in rows.iter().enumerate().skip(header_index + 1) {
        let row_number = index + 1;
        if row.len() < rows[header_index].len() {
            warnings.push(ImportWarning {
                row: row_number,
                code: "short_row".into(),
                message: "列数不足，空列按空值处理".into(),
            });
        }
        let result = match provider {
            Provider::Wechat => wechat::normalize_row(row, &headers, row_number, &mut seen_ids),
            Provider::Alipay => alipay::normalize_row(row, &headers, row_number, &mut seen_ids),
            Provider::Bank => bank_row(row, &headers, row_number, &mut seen_ids),
        };
        match result {
            Ok(row) => parsed.push(row),
            Err(error) => {
                let code = match &error {
                    ImportError::InvalidDate { .. } => "invalid_date",
                    ImportError::MissingAmount { .. } => "missing_amount",
                    ImportError::InvalidAmount { .. } => "invalid_amount",
                    ImportError::InvalidDirection { .. } => "invalid_direction",
                    _ => "invalid_row",
                };
                errors.push(ImportWarning {
                    row: row_number,
                    code: code.into(),
                    message: error.to_string(),
                });
            }
        }
    }
    if parsed.is_empty() && errors.is_empty() {
        return Err(ImportError::EmptyInput);
    }
    Ok(ParsedImport {
        source_kind,
        rows: parsed,
        warnings,
        errors,
    })
}

fn bank_row(
    row: &[String],
    headers: &std::collections::HashMap<String, usize>,
    row_number: usize,
    seen_ids: &mut HashSet<String>,
) -> Result<NormalizedImportRow, ImportError> {
    let date = csv::value(row, headers, &["交易日期", "日期", "记账日期", "交易时间"])
        .ok_or_else(|| ImportError::MissingHeaders(vec!["日期".into()]))?;
    let occurred_at = normalize::parse_date(date, row_number)?;
    let amount_aliases = ["交易金额", "金额", "发生额"];
    let amount_text = csv::value(row, headers, &amount_aliases)
        .ok_or(ImportError::MissingAmount { row: row_number })?;
    let (parsed, negative) = normalize::parse_minor(amount_text, 2, row_number)?;
    let direction =
        csv::value(row, headers, &["借贷标记", "收支", "借方贷方", "交易方向"]).unwrap_or("");
    let kind = normalize::direction_kind("", direction, negative, true, row_number)?;
    let amount_minor =
        parsed
            .unsigned_abs()
            .try_into()
            .map_err(|_| ImportError::InvalidAmount {
                row: row_number,
                value: amount_text.into(),
            })?;
    let id = csv::value(
        row,
        headers,
        &["流水号", "交易流水号", "参考号", "交易单号"],
    )
    .and_then(|v| normalize::normalize_text(Some(v)));
    let mut warnings = Vec::new();
    if let Some(id) = &id
        && !seen_ids.insert(id.clone())
    {
        warnings.push(ImportWarning {
            row: row_number,
            code: "duplicate_external_transaction_id".into(),
            message: format!("外部交易号 {id} 在文件中重复"),
        });
    }
    let merchant =
        normalize::normalize_text(csv::value(row, headers, &["交易对方", "对方户名", "对方"]));
    let description =
        normalize::normalize_text(csv::value(row, headers, &["摘要", "附言", "备注"]));
    let currency = normalize::cny();
    let digest = NormalizedImportRow::digest_for(
        SourceKind::Bank,
        id.as_deref(),
        occurred_at,
        kind,
        amount_minor,
        &currency,
        merchant.as_deref(),
        description.as_deref(),
    );
    Ok(NormalizedImportRow {
        row_number,
        source_kind: SourceKind::Bank,
        external_transaction_id: id,
        occurred_at,
        kind,
        amount_minor,
        currency,
        merchant,
        description,
        source_fields_digest: digest,
        warnings,
    })
}

/// Convenience for callers that need a UTC date without importing chrono in UI code.
pub fn row_time(row: &NormalizedImportRow) -> DateTime<Utc> {
    row.occurred_at
}
