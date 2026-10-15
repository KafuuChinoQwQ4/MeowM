//! Shared confidence decisions; every parsed result still requires user confirmation.
use crate::{NotificationEvent, NotificationSettings};
use ledger_domain::TransactionKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCandidate {
    pub kind: TransactionKind,
    pub amount_minor: Option<i64>,
    pub merchant: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    Candidate(NotificationCandidate),
    NeedsConfirmation(NotificationCandidate),
    Ignored,
    Unsupported,
}
pub fn parse_notification(
    event: &NotificationEvent,
    settings: &NotificationSettings,
) -> ParseOutcome {
    if !ledger_app::notifications::NOTIFICATION_PACKAGES.contains(&event.package_name.as_str()) {
        return ParseOutcome::Unsupported;
    }
    if !settings.enabled || settings.package_name != event.package_name || !event.valid() {
        return ParseOutcome::Ignored;
    }
    let text = format!("{}\n{}", event.title, event.body);
    let Some(kind) = crate::providers::direction(&text) else {
        return ParseOutcome::Ignored;
    };
    let candidate = NotificationCandidate {
        kind,
        amount_minor: crate::amount::parse(&text),
        merchant: crate::providers::merchant(&text),
    };
    if candidate.amount_minor.is_some()
        && candidate.merchant.is_some()
        && !matches!(kind, TransactionKind::Transfer | TransactionKind::Refund)
    {
        ParseOutcome::Candidate(candidate)
    } else {
        ParseOutcome::NeedsConfirmation(candidate)
    }
}
