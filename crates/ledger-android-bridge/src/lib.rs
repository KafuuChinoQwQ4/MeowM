//! Rust-owned decision and persistence boundary for Android notification capture.

pub mod credentials;

use chrono::{DateTime, Utc};
use ledger_app::{ImportRecord, MobileLedger, MobileRepository, PendingDetails, RepositoryError};
use ledger_domain::{
    Money, Posting, SourceEvidence, SourceKind, Transaction, TransactionId, TransactionKind,
    TransactionStatus,
};
use ledger_notifications::{
    NotificationEvent, NotificationSettings, ParseOutcome, parse_notification,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CaptureResult {
    Ignored = 0,
    Pending = 1,
    Confirmed = 2,
    Error = 3,
}

pub struct NotificationProcessor<R> {
    repository: R,
}
impl<R: MobileRepository> NotificationProcessor<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn repository(&self) -> &R {
        &self.repository
    }

    pub fn capture(
        &self,
        event: NotificationEvent,
        settings: NotificationSettings,
    ) -> Result<CaptureResult, RepositoryError> {
        let parsed = match parse_notification(&event, &settings) {
            ParseOutcome::Candidate(value) | ParseOutcome::NeedsConfirmation(value) => value,
            ParseOutcome::Ignored | ParseOutcome::Unsupported => return Ok(CaptureResult::Ignored),
        };
        let notification_identity = parsed.notification_identity.clone();
        if let Some(existing) = self
            .repository
            .list_import_records()?
            .into_iter()
            .find(|record| {
                record.evidence.source_kind == SourceKind::Notification
                    && record.evidence.external_transaction_id.as_deref()
                        == Some(notification_identity.as_str())
            })
        {
            return Ok(result_for_status(existing.transaction.status));
        }

        let posted = DateTime::<Utc>::from_timestamp_millis(event.posted_at_epoch_ms)
            .ok_or_else(|| RepositoryError::InvalidData("invalid notification timestamp".into()))?;
        let occurred_at = parsed.occurred_at.unwrap_or(posted);
        let kind = parsed.direction.unwrap_or(TransactionKind::Expense);
        let amount = parsed.amount.clone();
        let report_amount_cny: Option<Money> = parsed
            .amount
            .filter(|amount| amount.currency().code() == "CNY");
        let digest = decode_digest(&parsed.content_digest)?;
        let batch_id = notification_batch_id(&notification_identity);
        let record = ImportRecord {
            transaction: Transaction {
                id: TransactionId::new(),
                occurred_at,
                kind,
                status: TransactionStatus::Pending,
                postings: Vec::<Posting>::new(),
                category_id: None,
                original_transaction_id: None,
                report_amount_cny,
                merchant: parsed.merchant,
                description: None,
            },
            evidence: SourceEvidence {
                source_kind: SourceKind::Notification,
                external_transaction_id: Some(notification_identity.clone()),
                import_batch_id: batch_id,
                source_fields_digest: digest,
            },
        };
        if self.repository.capture_pending(
            record,
            PendingDetails {
                amount,
                source: parsed.source,
                account_hint: parsed.account_hint,
                original_external_id: parsed.original_external_transaction_id,
            },
        )? == 1
        {
            Ok(CaptureResult::Pending)
        } else {
            let status = self
                .repository
                .list_import_records()?
                .into_iter()
                .find(|record| {
                    record.evidence.source_kind == SourceKind::Notification
                        && record.evidence.external_transaction_id.as_deref()
                            == Some(notification_identity.as_str())
                })
                .map(|record| record.transaction.status);
            Ok(status
                .map(result_for_status)
                .unwrap_or(CaptureResult::Ignored))
        }
    }
}

fn result_for_status(status: TransactionStatus) -> CaptureResult {
    match status {
        TransactionStatus::Pending => CaptureResult::Pending,
        TransactionStatus::Confirmed => CaptureResult::Confirmed,
        TransactionStatus::Ignored => CaptureResult::Ignored,
    }
}

fn notification_batch_id(identity: &str) -> Uuid {
    let mut bytes: [u8; 16] = Sha256::digest(identity.as_bytes())[..16]
        .try_into()
        .expect("digest prefix has fixed length");
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn decode_digest(value: &str) -> Result<[u8; 32], RepositoryError> {
    if value.len() != 64 {
        return Err(RepositoryError::InvalidData(
            "invalid notification digest".into(),
        ));
    }
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| RepositoryError::InvalidData("invalid notification digest".into()))?;
    }
    Ok(result)
}

#[derive(Default)]
struct BridgeState {
    database_path: Option<String>,
    wechat_enabled: bool,
    alipay_enabled: bool,
}
static STATE: OnceLock<Mutex<BridgeState>> = OnceLock::new();
fn state() -> &'static Mutex<BridgeState> {
    STATE.get_or_init(|| Mutex::new(BridgeState::default()))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_initializeNative(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    database_path: jni::objects::JString<'_>,
) -> jni::sys::jboolean {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Ok(path) = env.get_string(&database_path) else {
            return false;
        };
        let path: String = path.into();
        if path.is_empty() {
            return false;
        }
        let Ok(_repo) = SqliteLedgerRepository::open(Path::new(&path)) else {
            return false;
        };
        let Ok(mut state) = state().lock() else {
            return false;
        };
        state.database_path = Some(path);
        true
    }))
    .unwrap_or(false) as jni::sys::jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_setSourceEnabledNative(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    package_name: jni::objects::JString<'_>,
    enabled: jni::sys::jboolean,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Ok(package_name) = env.get_string(&package_name) else {
            return;
        };
        let package_name: String = package_name.into();
        let Ok(mut state) = state().lock() else {
            return;
        };
        match package_name.as_str() {
            "com.tencent.mm" => state.wechat_enabled = enabled != 0,
            "com.eg.android.AlipayGphone" => state.alipay_enabled = enabled != 0,
            _ => return,
        }
        if let Some(path) = state.database_path.as_deref()
            && let Ok(repository) = SqliteLedgerRepository::open(Path::new(path))
        {
            let app = MobileLedger::new(repository);
            if let Ok(mut settings) = app.settings() {
                settings.notification_wechat = state.wechat_enabled;
                settings.notification_alipay = state.alipay_enabled;
                let _ = app.save_settings(settings);
            }
        }
    }));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_localledger_RustBridge_onPaymentNotificationNative(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    package_name: jni::objects::JString<'_>,
    title: jni::objects::JString<'_>,
    body: jni::objects::JString<'_>,
    notification_id: jni::objects::JString<'_>,
    posted_at: jni::sys::jlong,
) -> jni::sys::jint {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (Ok(package_name), Ok(title), Ok(body), Ok(notification_id)) = (
            env.get_string(&package_name),
            env.get_string(&title),
            env.get_string(&body),
            env.get_string(&notification_id),
        ) else {
            return CaptureResult::Error as i32;
        };
        let (package_name, title, body, notification_id): (String, String, String, String) = (
            package_name.into(),
            title.into(),
            body.into(),
            notification_id.into(),
        );
        let Ok(state) = state().lock() else {
            return CaptureResult::Error as i32;
        };
        let Some(path) = state.database_path.as_deref() else {
            return CaptureResult::Error as i32;
        };
        let settings = NotificationSettings {
            enabled: true,
            wechat_enabled: state.wechat_enabled,
            alipay_enabled: state.alipay_enabled,
        };
        let event = NotificationEvent {
            package_name,
            title,
            body,
            notification_id,
            posted_at_epoch_ms: posted_at,
            capture_settings: ledger_notifications::CaptureSettings {
                capture_enabled: true,
            },
        };
        let Ok(repository) = SqliteLedgerRepository::open(Path::new(path)) else {
            return CaptureResult::Error as i32;
        };
        NotificationProcessor::new(repository)
            .capture(event, settings)
            .map(|result| result as i32)
            .unwrap_or(CaptureResult::Error as i32)
    }))
    .unwrap_or(CaptureResult::Error as i32)
}
