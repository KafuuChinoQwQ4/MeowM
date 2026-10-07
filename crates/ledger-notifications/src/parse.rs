//! Shared notification parsing, normalization, confidence, and pending handling.
use crate::{NotificationEvent, providers};
use chrono::{DateTime, NaiveDateTime, Utc};
use ledger_domain::{Money, TransactionKind, TransactionStatus};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub code: WarningCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    MissingAmount,
    InvalidAmount,
    MissingDate,
    InvalidDate,
    UnknownDirection,
    MissingMerchant,
    MissingOriginalTransaction,
    TransferNeedsAccountMapping,
    SourceDisabled,
}

/// Sanitized normalized result. It contains no notification title or body.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedNotification {
    pub source: String,
    pub notification_identity: String,
    pub content_digest: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub amount: Option<Money>,
    pub currency: Option<String>,
    pub direction: Option<TransactionKind>,
    pub merchant: Option<String>,
    pub account_hint: Option<String>,
    pub external_transaction_id: Option<String>,
    pub original_external_transaction_id: Option<String>,
    pub confidence: Confidence,
    pub warnings: Vec<ParseWarning>,
    /// Notification-derived transactions are always pending until an explicit user action.
    pub status: TransactionStatus,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    Candidate(ParsedNotification),
    NeedsConfirmation(ParsedNotification),
    Ignored,
    Unsupported,
}

impl fmt::Debug for ParsedNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParsedNotification")
            .field("source", &self.source)
            .field("digest", &self.content_digest)
            .finish()
    }
}

impl fmt::Debug for ParseOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Candidate(parsed) => formatter.debug_tuple("Candidate").field(parsed).finish(),
            Self::NeedsConfirmation(parsed) => formatter
                .debug_tuple("NeedsConfirmation")
                .field(parsed)
                .finish(),
            Self::Ignored => formatter.write_str("Ignored"),
            Self::Unsupported => formatter.write_str("Unsupported"),
        }
    }
}

/// Current source preferences. Unconfigured sources are disabled by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub wechat_enabled: bool,
    pub alipay_enabled: bool,
}

pub(crate) struct NormalizedFields {
    pub occurred_at: Option<DateTime<Utc>>,
    pub amount: Option<Money>,
    pub direction: Option<TransactionKind>,
    pub merchant: Option<String>,
    pub account_hint: Option<String>,
    pub external_transaction_id: Option<String>,
    pub original_external_transaction_id: Option<String>,
    pub warnings: Vec<ParseWarning>,
    pub confidence: Confidence,
    pub must_confirm: bool,
}

pub fn parse_notification(
    event: &NotificationEvent,
    settings: &NotificationSettings,
) -> ParseOutcome {
    if !matches!(
        event.package_name.as_str(),
        providers::WECHAT_PACKAGE | providers::ALIPAY_PACKAGE
    ) {
        return ParseOutcome::Unsupported;
    }
    if !settings.enabled || !event.capture_settings.capture_enabled {
        return ParseOutcome::Ignored;
    }
    let (source_enabled, fields) = match event.package_name.as_str() {
        providers::WECHAT_PACKAGE => (settings.wechat_enabled, providers::wechat::parse(event)),
        providers::ALIPAY_PACKAGE => (settings.alipay_enabled, providers::alipay::parse(event)),
        _ => unreachable!("package was checked against explicit whitelist"),
    };
    if !source_enabled {
        return ParseOutcome::Ignored;
    }
    let content_digest = stable_digest(&[
        &event.package_name,
        &event.notification_id,
        fields
            .occurred_at
            .as_ref()
            .map(|value| value.to_rfc3339())
            .as_deref()
            .unwrap_or(""),
        fields
            .amount
            .as_ref()
            .map(Money::decimal_string)
            .as_deref()
            .unwrap_or(""),
        fields.direction.map(kind_name).unwrap_or(""),
        fields.merchant.as_deref().unwrap_or(""),
        fields.external_transaction_id.as_deref().unwrap_or(""),
        fields
            .original_external_transaction_id
            .as_deref()
            .unwrap_or(""),
        fields.account_hint.as_deref().unwrap_or(""),
    ]);
    // Android keys identify reusable notification slots, not payment events.
    // Keep provider transaction IDs stable across metadata updates; otherwise
    // include the posted instant and a digest of the complete transient content.
    let notification_identity = match fields.external_transaction_id.as_deref() {
        Some(external) => {
            stable_digest(&["notification-v2-external", &event.package_name, external])
        }
        None => stable_digest(&[
            "notification-v2-event",
            &event.package_name,
            &event.notification_id,
            &event.posted_at_epoch_ms.to_string(),
            &content_digest,
            &stable_digest(&[&event.title, &event.body]),
        ]),
    };
    let missing_required =
        fields.occurred_at.is_none() || fields.amount.is_none() || fields.direction.is_none();
    let must_confirm =
        fields.must_confirm || missing_required || fields.confidence != Confidence::High;
    let source = match event.package_name.as_str() {
        providers::WECHAT_PACKAGE => "wechat",
        providers::ALIPAY_PACKAGE => "alipay",
        _ => unreachable!("package was checked against explicit whitelist"),
    };
    let parsed = ParsedNotification {
        source: source.to_owned(),
        notification_identity,
        content_digest,
        occurred_at: fields.occurred_at,
        currency: fields
            .amount
            .as_ref()
            .map(|amount| amount.currency().code().to_owned()),
        amount: fields.amount,
        direction: fields.direction,
        merchant: fields.merchant,
        account_hint: fields.account_hint,
        external_transaction_id: fields.external_transaction_id,
        original_external_transaction_id: fields.original_external_transaction_id,
        confidence: fields.confidence,
        warnings: fields.warnings,
        status: TransactionStatus::Pending,
    };
    if must_confirm {
        ParseOutcome::NeedsConfirmation(parsed)
    } else {
        ParseOutcome::Candidate(parsed)
    }
}

pub(crate) fn parse_amount(text: &str) -> Result<Money, WarningCode> {
    let mut value = text.trim().replace(['¥', '￥'], "");
    for prefix in ["人民币", "CNY", "RMB"] {
        value = value.replace(prefix, "");
    }
    value = value
        .replace("元", "")
        .replace(['圆', ',', '，'], "")
        .trim()
        .to_owned();
    if value.is_empty() {
        return Err(WarningCode::MissingAmount);
    }
    if value.starts_with('-') || value.starts_with('+') || value.contains(['e', 'E']) {
        return Err(WarningCode::InvalidAmount);
    }
    let Some((whole, fraction)) = split_amount(&value) else {
        return Err(WarningCode::InvalidAmount);
    };
    if whole.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(WarningCode::InvalidAmount);
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| WarningCode::InvalidAmount)?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| WarningCode::InvalidAmount)?
            * if fraction.len() == 1 { 10 } else { 1 }
    };
    let minor = whole
        .checked_mul(100)
        .and_then(|amount| amount.checked_add(fractional))
        .ok_or(WarningCode::InvalidAmount)?;
    Ok(Money::new(
        minor,
        ledger_domain::CurrencyCode::new("CNY", 2).map_err(|_| WarningCode::InvalidAmount)?,
    ))
}

fn split_amount(value: &str) -> Option<(&str, &str)> {
    match value.split_once('.') {
        Some((whole, fraction)) if !fraction.contains('.') => Some((whole, fraction)),
        Some(_) => None,
        None => Some((value, "")),
    }
}

pub(crate) fn parse_date(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
    ] {
        if let Ok(value) = NaiveDateTime::parse_from_str(text, format) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(
                value - chrono::Duration::hours(8),
                Utc,
            ));
        }
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

pub(crate) fn direction(text: &str) -> Option<TransactionKind> {
    if text.contains("退款") || text.contains("退还") {
        Some(TransactionKind::Refund)
    } else if text.contains("转账") || text.contains("转出") || text.contains("转入") {
        Some(TransactionKind::Transfer)
    } else if text.contains("收款") || text.contains("到账") || text.contains("收入") {
        Some(TransactionKind::Income)
    } else if text.contains("支付")
        || text.contains("付款")
        || text.contains("消费")
        || text.contains("支出")
    {
        Some(TransactionKind::Expense)
    } else {
        None
    }
}

pub(crate) fn confidence(
    occurred_at: Option<&DateTime<Utc>>,
    amount: Option<&Money>,
    direction: Option<TransactionKind>,
    merchant: Option<&str>,
) -> Confidence {
    if occurred_at.is_some() && amount.is_some() && direction.is_some() && merchant.is_some() {
        Confidence::High
    } else if amount.is_some() && direction.is_some() {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

pub(crate) fn warning(code: WarningCode) -> ParseWarning {
    let message = match code {
        WarningCode::MissingAmount => "通知中缺少金额",
        WarningCode::InvalidAmount => "通知金额格式无法安全解析",
        WarningCode::MissingDate => "通知中缺少交易时间",
        WarningCode::InvalidDate => "通知交易时间格式无法解析",
        WarningCode::UnknownDirection => "无法确定通知中的收支方向",
        WarningCode::MissingMerchant => "通知中缺少商户或对方信息",
        WarningCode::MissingOriginalTransaction => "退款缺少原交易号",
        WarningCode::TransferNeedsAccountMapping => "转账需要用户确认转出和转入账户",
        WarningCode::SourceDisabled => "通知来源已关闭",
    };
    ParseWarning {
        code,
        message: message.into(),
    }
}

/// Match an actual transaction-ID label, never its "original transaction" suffix.
pub(crate) fn external_transaction_id(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        ["交易单号", "交易订单号", "交易号"]
            .iter()
            .find_map(|label| {
                let value = line.trim().strip_prefix(label)?;
                let value = value.trim_start().strip_prefix(['：', ':'])?.trim();
                (!value.is_empty() && value != "/" && value != "-").then(|| value.to_owned())
            })
    })
}
fn stable_digest(fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    hex(&hash.finalize())
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    result
}
fn kind_name(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Transfer => "transfer",
        TransactionKind::Refund => "refund",
        TransactionKind::BalanceAdjustment => "adjustment",
    }
}
