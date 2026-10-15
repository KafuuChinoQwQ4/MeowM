//! Narrow in-memory Android event. Deliberately does not implement Debug.
#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationEvent {
    pub package_name: String,
    pub title: String,
    pub body: String,
    pub notification_key: String,
    pub posted_at_ms: i64,
}
impl NotificationEvent {
    pub fn valid(&self) -> bool {
        self.package_name.len() <= 128
            && !self.notification_key.trim().is_empty()
            && self.notification_key.len() <= 1024
            && self.title.len() <= 2048
            && self.body.len() <= 16384
            && self.posted_at_ms > 0
            && chrono::DateTime::from_timestamp_millis(self.posted_at_ms).is_some()
    }
}
