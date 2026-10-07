//! WeChat notification format extraction; shared value normalization lives in `parse`.
use crate::{
    NotificationEvent,
    parse::{self, NormalizedFields, WarningCode},
};
use ledger_domain::TransactionKind;

pub(crate) fn parse(event: &NotificationEvent) -> NormalizedFields {
    let text = format!("{}\n{}", event.title, event.body);
    let amount_text = value_after(&text, &["支付金额", "收款金额", "退款金额", "金额"]);
    let date_text = value_after(
        &text,
        &["支付时间", "收款时间", "退款时间", "转账时间", "交易时间"],
    );
    let merchant = value_after(&text, &["商户", "收款方", "付款方", "交易对方"]);
    let account = value_after(&text, &["支付方式", "付款方式", "收款方式"]);
    let external = parse::external_transaction_id(&text);
    let original = value_after(&text, &["原交易单号", "原交易号", "原交易订单号"]);
    let occurred_at = date_text.as_deref().and_then(parse::parse_date);
    let amount = amount_text
        .as_deref()
        .and_then(|value| parse::parse_amount(value).ok());
    let direction = parse::direction(&text);
    let mut warnings = Vec::new();
    if amount.is_none() {
        warnings.push(parse::warning(if amount_text.is_some() {
            WarningCode::InvalidAmount
        } else {
            WarningCode::MissingAmount
        }));
    }
    if occurred_at.is_none() {
        warnings.push(parse::warning(if date_text.is_some() {
            WarningCode::InvalidDate
        } else {
            WarningCode::MissingDate
        }));
    }
    if direction.is_none() {
        warnings.push(parse::warning(WarningCode::UnknownDirection));
    }
    if direction == Some(TransactionKind::Refund) && original.is_none() {
        warnings.push(parse::warning(WarningCode::MissingOriginalTransaction));
    }
    if merchant.is_none() {
        warnings.push(parse::warning(WarningCode::MissingMerchant));
    }
    let transfer = direction == Some(TransactionKind::Transfer);
    if transfer {
        warnings.push(parse::warning(WarningCode::TransferNeedsAccountMapping));
    }
    let confidence = parse::confidence(
        occurred_at.as_ref(),
        amount.as_ref(),
        direction,
        merchant.as_deref(),
    );
    NormalizedFields {
        occurred_at,
        amount,
        direction,
        merchant,
        account_hint: account,
        external_transaction_id: external,
        original_external_transaction_id: original,
        warnings,
        confidence,
        must_confirm: transfer || direction == Some(TransactionKind::Refund),
    }
}

fn value_after(text: &str, labels: &[&str]) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        for label in labels {
            if let Some((_, value)) = line.split_once(label) {
                let value = value.trim().trim_start_matches(['：', ':', ' ']).trim();
                if !value.is_empty() {
                    return Some(value.to_owned());
                }
            }
        }
    }
    None
}
