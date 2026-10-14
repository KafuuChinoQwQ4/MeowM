//! Alipay personal bill CSV adapter.

use crate::{Provider, csv, normalize::*};
use ledger_domain::{SourceKind, TransactionKind};
use std::collections::HashSet;

pub fn parse_alipay_csv(bytes: &[u8]) -> Result<crate::ParsedImport, ImportError> {
    super::parse_provider_csv(bytes, SourceKind::Alipay, Provider::Alipay)
}

pub(crate) fn normalize_row(
    row: &[String],
    headers: &std::collections::HashMap<String, usize>,
    row_number: usize,
    seen_ids: &mut HashSet<String>,
) -> Result<NormalizedImportRow, ImportError> {
    let date = csv::value(row, headers, &["交易时间", "付款时间", "日期"])
        .ok_or_else(|| ImportError::MissingHeaders(vec!["交易时间".into()]))?;
    let occurred_at = parse_date(date, row_number)?;
    let amount_text = csv::value(row, headers, &["交易金额", "金额", "金额(元)"])
        .ok_or(ImportError::MissingAmount { row: row_number })?;
    let (parsed, negative) = parse_minor(amount_text, 2, row_number)?;
    let direction = csv::value(row, headers, &["收/支", "收支", "资金流向"]).unwrap_or("");
    let type_text = csv::value(row, headers, &["交易类型", "类型", "分类"]).unwrap_or("");
    let kind = kind_for(type_text, direction, negative, row_number)?;
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
        &["交易订单号", "商家订单号", "交易单号", "流水号"],
    )
    .and_then(|v| normalize_text(Some(v)));
    let mut warnings = Vec::new();
    if let Some(id) = &id {
        if !seen_ids.insert(id.clone()) {
            warnings.push(ImportWarning {
                row: row_number,
                code: "duplicate_external_transaction_id".into(),
                message: format!("外部交易号 {id} 在文件中重复"),
            });
        }
    }
    let merchant = normalize_text(csv::value(row, headers, &["交易对方", "对方"]));
    let description = normalize_text(csv::value(row, headers, &["商品说明", "商品", "备注"]));
    let currency = cny();
    let digest = NormalizedImportRow::digest_for(
        SourceKind::Alipay,
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
        source_kind: SourceKind::Alipay,
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

fn kind_for(
    type_text: &str,
    direction: &str,
    negative: bool,
    row: usize,
) -> Result<TransactionKind, ImportError> {
    let text = format!("{type_text}{direction}");
    if text.contains("退款") || text.contains("退回") {
        return Ok(TransactionKind::Refund);
    }
    if text.contains("收入")
        || text.contains("收款")
        || text.contains("转入")
        || text.contains("贷")
    {
        return Ok(TransactionKind::Income);
    }
    if text.contains("支出")
        || text.contains("付款")
        || text.contains("转出")
        || text.contains("借")
        || negative
    {
        return Ok(TransactionKind::Expense);
    }
    Err(ImportError::InvalidDirection {
        row,
        value: direction.to_owned(),
    })
}
