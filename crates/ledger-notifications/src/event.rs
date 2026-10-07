//! Primitive, transient notification input from the Android adapter.
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fmt;

/// Collection state captured by the adapter when it creates an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSettings {
    pub capture_enabled: bool,
}

/// A notification value object. `body` is transient parser input and must never
/// be stored as source evidence or emitted to logs.
#[derive(Clone, PartialEq, Eq)]
pub struct NotificationEvent {
    pub package_name: String,
    pub title: String,
    pub body: String,
    pub notification_id: String,
    /// Unix timestamp in milliseconds supplied by Android.
    pub posted_at_epoch_ms: i64,
    pub capture_settings: CaptureSettings,
}

impl fmt::Debug for NotificationEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source = match self.package_name.as_str() {
            "com.tencent.mm" => "wechat",
            "com.eg.android.AlipayGphone" => "alipay",
            _ => "unsupported",
        };
        let mut digest = Sha256::new();
        digest.update(self.package_name.as_bytes());
        digest.update([0]);
        digest.update(self.notification_id.as_bytes());
        digest.update([0]);
        digest.update(self.title.as_bytes());
        digest.update([0]);
        digest.update(self.body.as_bytes());
        formatter
            .debug_struct("NotificationEvent")
            .field("source", &source)
            .field("summary_digest", &Hex(&digest.finalize()))
            .field(
                "posted_at",
                &DateTime::<Utc>::from_timestamp_millis(self.posted_at_epoch_ms),
            )
            .field("capture_enabled", &self.capture_settings.capture_enabled)
            .finish()
    }
}

struct Hex<'a>(&'a [u8]);
impl fmt::Debug for Hex<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
