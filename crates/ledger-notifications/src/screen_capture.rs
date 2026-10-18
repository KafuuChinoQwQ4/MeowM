//! In-memory OCR event parsing for payment success screens.
use crate::{NotificationCandidate, amount, providers};
use ledger_app::notifications::NOTIFICATION_PACKAGES;
use ledger_domain::TransactionKind;

/// OCR input from Android. The recognized text is never persisted by this crate.
#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenCaptureEvent {
    pub package_name: String,
    pub event_id: String,
    pub recognized_text: String,
    pub captured_at_ms: i64,
}

impl ScreenCaptureEvent {
    pub fn valid(&self) -> bool {
        !self.event_id.is_empty()
            && self.event_id.len() <= 128
            && self.package_name.len() <= 128
            && self.recognized_text.len() <= 16_384
            && !self.recognized_text.trim().is_empty()
            && self.captured_at_ms > 0
            && chrono::DateTime::from_timestamp_millis(self.captured_at_ms).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenCaptureParserSettings {
    pub enabled: bool,
}

impl ScreenCaptureParserSettings {
    pub const fn enabled() -> Self {
        Self { enabled: true }
    }
    pub const fn disabled() -> Self {
        Self { enabled: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenCaptureOutcome {
    Candidate(NotificationCandidate),
    NeedsConfirmation(NotificationCandidate),
    Ignored,
    Unsupported,
}

pub fn parse_screen_capture(
    event: &ScreenCaptureEvent,
    settings: &ScreenCaptureParserSettings,
) -> ScreenCaptureOutcome {
    if !NOTIFICATION_PACKAGES.contains(&event.package_name.as_str()) {
        return ScreenCaptureOutcome::Unsupported;
    }
    if !settings.enabled || !event.valid() {
        return ScreenCaptureOutcome::Ignored;
    }
    let text = event.recognized_text.as_str();
    if !["支付成功", "付款成功", "收款成功", "转账成功", "退款成功"]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return ScreenCaptureOutcome::Ignored;
    }
    if ["支付失败", "付款失败", "已取消", "待付款", "付款码"]
        .iter()
        .any(|m| text.contains(m))
    {
        return ScreenCaptureOutcome::Ignored;
    }
    let kind = if text.contains("退款成功") {
        TransactionKind::Refund
    } else if text.contains("转账成功") {
        TransactionKind::Transfer
    } else if text.contains("收款成功") {
        TransactionKind::Income
    } else {
        TransactionKind::Expense
    };
    let candidate = NotificationCandidate {
        kind,
        amount_minor: amount::parse(text),
        merchant: merchant_line(text).or_else(|| providers::merchant(text)),
    };
    if candidate.amount_minor.is_some()
        && candidate.merchant.is_some()
        && !matches!(kind, TransactionKind::Transfer | TransactionKind::Refund)
    {
        ScreenCaptureOutcome::Candidate(candidate)
    } else {
        ScreenCaptureOutcome::NeedsConfirmation(candidate)
    }
}

fn merchant_line(text: &str) -> Option<String> {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    for (i, line) in lines.iter().enumerate() {
        for label in ["商户", "商家", "收款方", "收款商户"] {
            if *line == label || *line == format!("{label}：") || *line == format!("{label}:") {
                return lines
                    .get(i + 1)
                    .filter(|s| valid_merchant(s))
                    .map(|s| s.to_string());
            }
            if let Some(name) = line
                .strip_prefix(&format!("{label}："))
                .or_else(|| line.strip_prefix(&format!("{label}:")))
            {
                return valid_merchant(name.trim()).then(|| name.trim().to_string());
            }
        }
    }
    None
}
fn valid_merchant(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 100
        && amount::parse(name).is_none()
        && ![
            "成功", "隐藏", "完成", "返回", "订单", "金额", "时间", "优惠",
        ]
        .iter()
        .any(|m| name.contains(m))
}
