//! Alipay personal bill CSV adapter.

use crate::{Provider, csv, normalize::*};
use ledger_domain::SourceKind;
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
    let date = csv::value(row, headers, &["交易时间", "付款时间", "记录时间", "日期"])
        .ok_or_else(|| ImportError::MissingHeaders(vec!["交易时间".into()]))?;
    let occurred_at = parse_date(date, row_number)?;
    let amount_text = csv::value(row, headers, &["交易金额", "金额", "金额(元)"])
        .ok_or(ImportError::MissingAmount { row: row_number })?;
    let (parsed, negative) = parse_minor(amount_text, 2, row_number)?;
    let direction =
        csv::value(row, headers, &["收/支", "收支", "收支类型", "资金流向"]).unwrap_or("");
    let type_text = csv::value(row, headers, &["交易类型", "类型", "分类"]).unwrap_or("");
    let kind = if direction.trim() == "不计收支" {
        if type_text.contains("退款") {
            ledger_domain::TransactionKind::Refund
        } else if ["转账", "借还款", "借款", "投资理财"].contains(&type_text.trim()) {
            ledger_domain::TransactionKind::Transfer
        } else {
            return Err(ImportError::InvalidDirection {
                row: row_number,
                value: direction.into(),
            });
        }
    } else {
        direction_kind(type_text, direction, negative, false, row_number)?
    };
    let amount_minor =
        parsed
            .unsigned_abs()
            .try_into()
            .map_err(|_| ImportError::InvalidAmount {
                row: row_number,
                value: amount_text.into(),
            })?;
    let id = csv::value(row, headers, &["交易订单号", "交易单号", "流水号"])
        .and_then(|v| normalize_text(Some(v)));
    let merchant_order_id = normalize_text(csv::value(
        row,
        headers,
        &["商家订单号", "商户单号", "商户订单号"],
    ));
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
    let original_external_transaction_id = refund_reference(row, headers, kind);
    let digest = digest_with_reference(digest, original_external_transaction_id.as_deref());
    let digest = digest_with_merchant_order(digest, merchant_order_id.as_deref());
    let account_hint = normalize_text(csv::value(row, headers, &["账户", "支付账户"]));
    let digest = digest_with_account_hint(digest, account_hint.as_deref());
    Ok(NormalizedImportRow {
        row_number,
        source_kind: SourceKind::Alipay,
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
        account_hint,
        source_fields_digest: digest,
        warnings,
    })
}
