//! WeChat personal bill CSV adapter.

use crate::{Provider, csv, normalize::*};
use ledger_domain::{SourceKind, TransactionKind};
use std::collections::HashSet;

pub fn parse_wechat_csv(bytes: &[u8]) -> Result<crate::ParsedImport, ImportError> {
    super::parse_provider_csv(bytes, SourceKind::WeChat, Provider::Wechat)
}

pub(crate) fn normalize_row(
    row: &[String],
    headers: &std::collections::HashMap<String, usize>,
    row_number: usize,
    seen_ids: &mut HashSet<String>,
) -> Result<NormalizedImportRow, ImportError> {
    let date = csv::value(row, headers, &["交易时间", "时间", "日期"])
        .ok_or_else(|| ImportError::MissingHeaders(vec!["交易时间".into()]))?;
    let occurred_at = parse_date(date, row_number)?;
    let amount_text = csv::value(row, headers, &["金额(元)", "金额", "交易金额"])
        .ok_or(ImportError::MissingAmount { row: row_number })?;
    let (parsed, negative) = parse_minor(amount_text, 2, row_number)?;
    let direction = csv::value(row, headers, &["收/支", "收支", "借贷标记"]).unwrap_or("");
    let type_text = csv::value(row, headers, &["交易类型", "类型"]).unwrap_or("");
    let status = csv::value(row, headers, &["当前状态", "交易状态", "状态"]).unwrap_or("");
    let kind = wechat_kind(type_text, status, direction, negative, row_number)?;
    let amount_minor =
        parsed
            .unsigned_abs()
            .try_into()
            .map_err(|_| ImportError::InvalidAmount {
                row: row_number,
                value: amount_text.into(),
            })?;
    let id = csv::value(row, headers, &["交易单号", "订单号", "流水号"])
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
    let merchant_order_id = normalize_text(csv::value(row, headers, &["商户单号", "商户订单号"]));
    let description = normalize_text(csv::value(row, headers, &["商品", "商品说明", "备注"]));
    let currency = cny();
    let digest = NormalizedImportRow::digest_for(
        SourceKind::WeChat,
        id.as_deref(),
        occurred_at,
        kind,
        amount_minor,
        &currency,
        merchant.as_deref(),
        description.as_deref(),
    );
    let original_external_transaction_id = refund_reference(row, headers, kind);
    let digest = digest_with_reference(digest, original_external_transaction_id.as_deref());
    let digest = digest_with_merchant_order(digest, merchant_order_id.as_deref());
    Ok(NormalizedImportRow {
        row_number,
        source_kind: SourceKind::WeChat,
        external_transaction_id: id,
        merchant_order_id,
        original_external_transaction_id,
        occurred_at,
        kind,
        transfer_direction: None,
        amount_minor,
        currency,
        merchant,
        description,
        account_hint: None,
        source_fields_digest: digest,
        warnings,
    })
}

fn wechat_kind(
    type_text: &str,
    status: &str,
    direction: &str,
    negative: bool,
    row: usize,
) -> Result<TransactionKind, ImportError> {
    if direction.trim() == "/" {
        let transaction_type = type_text.trim();
        let status = status.trim();
        match transaction_type {
            "零钱充值" if status.contains("充值完成") => {
                return Ok(TransactionKind::Income);
            }
            "零钱提现" if status.contains("提现已到账") => {
                return Ok(TransactionKind::Expense);
            }
            "其他" if status.contains("已存入零钱") => return Ok(TransactionKind::Income),
            _ => {}
        }
    }
    direction_kind(type_text, direction, negative, false, row)
}
