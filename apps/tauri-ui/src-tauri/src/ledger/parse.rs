use super::*;
pub(super) fn cny() -> CurrencyCode {
    CurrencyCode::new("CNY", 2).expect("constant currency")
}
pub(crate) fn money(value: i64) -> Money {
    Money::new(value, cny())
}
pub(crate) fn account_id(value: &str) -> Result<AccountId> {
    uuid::Uuid::parse_str(value)
        .map(AccountId::from_uuid)
        .map_err(|_| "无效的平台标识".into())
}
pub(crate) fn date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "请选择有效日期".into())
}
pub(super) fn parse_source(value: &str) -> Result<ledger_domain::SourceKind> {
    match value {
        "wechat" => Ok(ledger_domain::SourceKind::WeChat),
        "alipay" => Ok(ledger_domain::SourceKind::Alipay),
        "bank" => Ok(ledger_domain::SourceKind::Bank),
        "notification" => Ok(ledger_domain::SourceKind::Notification),
        "manual" => Ok(ledger_domain::SourceKind::Manual),
        "recurring" => Ok(ledger_domain::SourceKind::Recurring),
        _ => Err("无效的来源".into()),
    }
}
pub(crate) fn time(day: NaiveDate, closing: bool) -> Result<chrono::DateTime<Local>> {
    let naive = if closing {
        day.and_hms_micro_opt(23, 59, 59, 999_999)
    } else {
        day.and_hms_opt(12, 0, 0)
    }
    .ok_or("日期超出范围")?;
    Local
        .from_local_datetime(&naive)
        .single()
        .ok_or("该日期本地时间不可用".into())
}
pub(crate) fn parse_amount(value: &str, signed: bool) -> Result<i64> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let digits = if negative { &value[1..] } else { value };
    let parts: Vec<_> = digits.split('.').collect();
    if parts.len() > 2
        || parts[0].is_empty()
        || parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit()))
        || parts.get(1).is_some_and(|p| p.is_empty() || p.len() > 2)
        || (negative && !signed)
    {
        return Err("金额最多两位小数，收支金额必须为正数".into());
    }
    let whole: i64 = parts[0].parse().map_err(|_| "金额过大")?;
    let cents = match parts.get(1) {
        Some(p) => p.parse::<i64>().map_err(|_| "金额无效")? * if p.len() == 1 { 10 } else { 1 },
        None => 0,
    };
    let amount = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(cents))
        .ok_or("金额过大")?;
    if amount == 0 && !signed {
        return Err("收支金额必须大于零".into());
    }
    Ok(if negative { -amount } else { amount })
}
pub(crate) fn transaction_id(value: &str) -> Result<TransactionId> {
    uuid::Uuid::parse_str(value)
        .map(TransactionId::from_uuid)
        .map_err(|_| "无效的流水标识".into())
}
pub(super) fn source_label(source: ledger_domain::SourceKind) -> &'static str {
    match source {
        ledger_domain::SourceKind::WeChat => "wechat",
        ledger_domain::SourceKind::Alipay => "alipay",
        ledger_domain::SourceKind::Bank => "bank",
        ledger_domain::SourceKind::Notification => "notification",
        ledger_domain::SourceKind::Manual => "manual",
        ledger_domain::SourceKind::Recurring => "recurring",
    }
}
pub(crate) fn kind_label(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Refund => "refund",
        TransactionKind::Transfer => "transfer",
        TransactionKind::BalanceAdjustment => "adjustment",
    }
}
