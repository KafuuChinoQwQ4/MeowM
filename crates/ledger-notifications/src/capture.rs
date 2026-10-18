//! Converts in-memory parser output into an atomic, restart-safe pending receipt.
use crate::{NotificationEvent, ParseOutcome, parse_notification};
use chrono::{DateTime, Utc};
use ledger_app::{
    RepositoryError,
    notifications::{CaptureResult, NotificationRecord, NotificationSettings},
    ports::NotificationRepository,
};
use ledger_domain::{
    AccountKind, CurrencyCode, Money, Posting, SourceEvidence, SourceKind, Transaction,
    TransactionId, TransactionKind, TransactionStatus,
};
use sha2::{Digest, Sha256};

pub fn capture_notification(
    repo: &impl NotificationRepository,
    event: &NotificationEvent,
) -> Result<CaptureResult, RepositoryError> {
    if !event.valid() {
        return Err(RepositoryError::InvalidData(
            "invalid notification event".into(),
        ));
    }
    let settings = repo
        .notification_settings()?
        .into_iter()
        .find(|s| s.package_name == event.package_name)
        .unwrap_or_else(|| NotificationSettings::disabled(&event.package_name));
    if !settings.enabled {
        return Ok(CaptureResult {
            code: "ignored".into(),
            transaction_id: None,
        });
    }
    let parsed = parse_notification(event, &settings);
    let identity = hash(&[
        &event.package_name,
        &event.notification_key,
        &event.posted_at_ms.to_string(),
    ]);
    let digest = hash(&[
        &event.package_name,
        &event.title.split_whitespace().collect::<Vec<_>>().join(" "),
        &event.body.split_whitespace().collect::<Vec<_>>().join(" "),
        &event.posted_at_ms.to_string(),
    ]);
    let identity = identity
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let (candidate, mut code) = match parsed {
        ParseOutcome::Candidate(c) => (Some(c), "pending"),
        ParseOutcome::NeedsConfirmation(c) => (Some(c), "needs_confirmation"),
        ParseOutcome::Ignored | ParseOutcome::Unsupported => (None, "ignored"),
    };
    let candidate = candidate
        .map(|candidate| {
            let currency = CurrencyCode::new("CNY", 2).expect("CNY is valid");
            let mut postings = Vec::new();
            if candidate.kind != TransactionKind::Transfer
                && let (Some(id), Some(amount)) = (settings.account_id, candidate.amount_minor)
            {
                let account = repo.get_account(id)?.ok_or(RepositoryError::NotFound)?;
                let positive = matches!(
                    candidate.kind,
                    TransactionKind::Income | TransactionKind::Refund
                );
                let sign = if positive == (account.kind == AccountKind::Asset) {
                    1
                } else {
                    -1
                };
                postings.push(Posting {
                    account_id: id,
                    delta: Money::new(amount * sign, currency.clone()),
                });
            }
            if postings.is_empty() {
                code = "needs_confirmation";
            }
            let tx = Transaction {
                id: TransactionId::new(),
                occurred_at: DateTime::<Utc>::from_timestamp_millis(event.posted_at_ms)
                    .expect("validated timestamp"),
                kind: candidate.kind,
                status: TransactionStatus::Pending,
                postings,
                category_id: None,
                original_transaction_id: None,
                report_amount_cny: candidate.amount_minor.map(|n| Money::new(n, currency)),
                merchant: candidate.merchant,
                description: Some(
                    if code == "pending" {
                        "通知候选：请核对后确认"
                    } else {
                        "通知候选：请补全金额、平台或关联信息后确认"
                    }
                    .into(),
                ),
            };
            let evidence = SourceEvidence {
                source_kind: SourceKind::Notification,
                external_transaction_id: Some(identity.clone()),
                import_batch_id: uuid::Uuid::new_v4(),
                source_fields_digest: digest,
            };
            Ok::<_, RepositoryError>((tx, evidence))
        })
        .transpose()?;
    repo.record_notification(NotificationRecord {
        package_name: event.package_name.clone(),
        identity,
        digest,
        candidate,
        code: code.into(),
        settings,
        source_kind: ledger_domain::SourceKind::Notification,
    })
}
fn hash(fields: &[&str]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    hash.finalize().into()
}
