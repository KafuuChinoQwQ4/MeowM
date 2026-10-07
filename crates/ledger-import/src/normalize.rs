//! Mapping provider rows into normalized ledger transactions.
use crate::{ParseWarning, WarningCode, money};
use chrono::{DateTime, NaiveDateTime, Utc};
use ledger_domain::Money;
pub(crate) fn date(s: &str) -> Result<DateTime<Utc>, WarningCode> {
    NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n - chrono::Duration::hours(8), Utc))
        .map_err(|_| WarningCode::InvalidDate)
}
pub(crate) fn currency_exponent(currency: &str) -> Option<u8> {
    match currency {
        "CNY" | "USD" | "EUR" | "GBP" | "CAD" | "AUD" | "NZD" | "CHF" | "HKD" | "SGD" | "TWD"
        | "INR" | "RUB" | "BRL" | "MXN" | "ZAR" | "TRY" | "THB" | "MYR" | "PHP" | "IDR" | "AED"
        | "SAR" => Some(2),
        "JPY" | "KRW" | "VND" => Some(0),
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" => Some(3),
        _ => None,
    }
}
pub(crate) fn amount(text: &str, currency: &str) -> Result<Money, WarningCode> {
    let exp = u32::from(currency_exponent(currency).ok_or(WarningCode::UnsupportedCurrency)?);
    let original = text.trim();
    if original.contains(',') {
        let parts = original
            .replace(['¥', '￥'], "")
            .split('.')
            .next()
            .unwrap_or("")
            .to_string();
        let whole = parts.trim_start_matches(['+', '-']);
        if !whole.split(',').enumerate().all(|(i, p)| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && (i == 0 || p.len() == 3)
        }) {
            return Err(WarningCode::InvalidAmount);
        }
    }
    let mut s = original.replace(['¥', '￥', ','], "");
    if s.is_empty() {
        return Err(WarningCode::MissingAmount);
    }
    if s.starts_with('-') || s.starts_with('+') || s.contains('e') || s.contains('E') {
        return Err(WarningCode::InvalidAmount);
    }
    if let Some((p, r)) = s.split_once(' ')
        && p.len() == 3
    {
        if p != currency {
            return Err(WarningCode::InvalidAmount);
        }
        s = r.to_string();
    }
    let mut it = s.split('.');
    let w = it.next().unwrap_or("");
    let fr = it.next();
    if it.next().is_some() || w.is_empty() || !w.chars().all(|c| c.is_ascii_digit()) {
        return Err(WarningCode::InvalidAmount);
    }
    let fr = fr.unwrap_or("");
    if fr.len() > exp as usize || !fr.chars().all(|c| c.is_ascii_digit()) {
        return Err(WarningCode::InvalidAmount);
    }
    let mut m = w
        .parse::<i64>()
        .map_err(|_| WarningCode::InvalidAmount)?
        .checked_mul(10i64.pow(exp))
        .ok_or(WarningCode::InvalidAmount)?;
    let f = if fr.is_empty() {
        0
    } else {
        fr.parse::<i64>().map_err(|_| WarningCode::InvalidAmount)?
    };
    m = m
        .checked_add(f * 10i64.pow(exp - fr.len() as u32))
        .ok_or(WarningCode::InvalidAmount)?;
    money(m, currency).map_err(|_| WarningCode::InvalidAmount)
}
pub(crate) fn is_transfer(transaction_type: &str) -> bool {
    ["转账", "转入", "转出", "还款"]
        .iter()
        .any(|label| transaction_type.contains(label))
}

pub(crate) fn warning(row: usize, code: WarningCode) -> ParseWarning {
    let message = match code {
        WarningCode::InvalidDate => "日期无法解析",
        WarningCode::InvalidAmount => "金额无法解析为整数最小单位",
        WarningCode::UnknownDirection => "无法确定收支方向",
        WarningCode::MissingAmount => "缺少金额",
        WarningCode::MissingRefundOriginal => "退款缺少原交易号",
        WarningCode::MissingExchangeRate => "缺少兑换为 CNY 的汇率",
        WarningCode::MalformedRow => "行字段数量不足",
        WarningCode::UnsettledTransaction => "交易未完成",
        WarningCode::UnsupportedCurrency => "不支持该币种",
        WarningCode::TransferNeedsAccountMapping => {
            "转账或还款需要转出和转入账户；请排除此行后手工补录"
        }
    };
    ParseWarning {
        row_number: row,
        code,
        message: message.into(),
    }
}
