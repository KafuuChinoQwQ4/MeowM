//! Local parsing of Android payment notification events.

pub mod event;
pub mod parse;
pub mod providers;

pub use event::{CaptureSettings, NotificationEvent};
pub use parse::{
    Confidence, NotificationSettings, ParseOutcome, ParseWarning, ParsedNotification, WarningCode,
    parse_notification,
};
