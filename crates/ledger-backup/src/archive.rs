//! Portable allowlisted archive schema; v3 upgrades to v4 with default settings.
pub use ledger_app::{ArchiveCell, LedgerArchive};
pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
