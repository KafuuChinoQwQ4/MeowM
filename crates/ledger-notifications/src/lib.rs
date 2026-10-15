//! Local-only payment notification parsing and pending capture.
mod amount;
pub mod event;
pub mod parse;
pub mod providers;
pub use event::NotificationEvent;
pub use ledger_app::notifications::NotificationSettings;
pub use parse::{NotificationCandidate, ParseOutcome, parse_notification};
mod capture;
pub use capture::capture_notification;
