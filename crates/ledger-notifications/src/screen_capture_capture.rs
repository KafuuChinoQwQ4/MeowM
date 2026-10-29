//! Atomic persistence for local screen OCR candidates.
use crate::{
    ScreenCaptureEvent, ScreenCaptureOutcome, ScreenCaptureParserSettings, parse_screen_capture,
};
use chrono::{DateTime, Utc};
use ledger_app::{
    RepositoryError,
    notifications::{CaptureResult, NotificationRecord, NotificationSettings},
    ports::ScreenCaptureRepository,
};
use ledger_domain::{
    CurrencyCode, Money, SourceEvidence, SourceKind, Transaction, TransactionId, TransactionStatus,
};
use sha2::{Digest, Sha256};

pub fn capture_screen(
    repo: &impl ScreenCaptureRepository,
    event: &ScreenCaptureEvent,
) -> Result<CaptureResult, RepositoryError> {
    if !event.valid() {
        return Err(RepositoryError::InvalidData(
            "invalid screen capture event".into(),
        ));
    }
    let settings = repo
        .screen_settings()?
        .into_iter()
        .find(|s| s.package_name == event.package_name)
        .unwrap_or_else(|| NotificationSettings::disabled(&event.package_name));
    if !settings.enabled {
        return Ok(CaptureResult {
            code: "ignored".into(),
            transaction_id: None,
        });
    }
    let parsed = parse_screen_capture(event, &ScreenCaptureParserSettings::enabled());
    let identity = hex(hash(&[
        "screen_capture",
        &event.package_name,
        &event.event_id,
    ]));
    let digest = hash(&[
        "screen_capture",
        &event.package_name,
        &normalize(&event.recognized_text),
        &event.captured_at_ms.to_string(),
    ]);
    let (candidate, mut code) = match parsed {
        ScreenCaptureOutcome::Candidate(c) => (Some(c), "pending"),
        ScreenCaptureOutcome::NeedsConfirmation(c) => (Some(c), "needs_confirmation"),
        ScreenCaptureOutcome::Ignored | ScreenCaptureOutcome::Unsupported => (None, "ignored"),
    };
    let candidate = candidate
        .map(|candidate| -> Result<_, RepositoryError> {
            let mut postings = Vec::new();
            if candidate.kind != ledger_domain::TransactionKind::Transfer {
                if let (Some(id), Some(amount)) = (settings.account_id, candidate.amount_minor) {
                    let account = repo.get_account(id)?.ok_or(RepositoryError::NotFound)?;
                    let positive = matches!(
                        candidate.kind,
                        ledger_domain::TransactionKind::Income
                            | ledger_domain::TransactionKind::Refund
                    );
                    let sign = if positive == (account.kind == ledger_domain::AccountKind::Asset) {
                        1
                    } else {
                        -1
                    };
                    postings.push(ledger_domain::Posting {
                        account_id: id,
                        delta: Money::new(amount * sign, account.currency),
                    });
                }
            }
            let currency = CurrencyCode::new("CNY", 2).expect("CNY is valid");
            if candidate.amount_minor.is_none()
                || candidate.merchant.is_none()
                || postings.is_empty()
            {
                code = "needs_confirmation";
            }
            let tx = Transaction {
                id: TransactionId::new(),
                occurred_at: DateTime::<Utc>::from_timestamp_millis(event.captured_at_ms)
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
                        "截图候选：请核对后确认"
                    } else {
                        "截图候选：请补全金额、平台或关联信息后确认"
                    }
                    .into(),
                ),
            };
            let evidence = SourceEvidence {
                source_kind: SourceKind::ScreenCapture,
                external_transaction_id: Some(identity.clone()),
                original_external_transaction_id: None,
                merchant_order_id: None,
                import_batch_id: uuid::Uuid::new_v4(),
                source_fields_digest: digest,
            };
            Ok((tx, evidence))
        })
        .transpose()?;
    repo.record_screen_capture(NotificationRecord {
        package_name: event.package_name.clone(),
        identity,
        digest,
        candidate,
        code: code.into(),
        settings,
        source_kind: SourceKind::ScreenCapture,
    })
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hash(fields: &[&str]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().into()
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
