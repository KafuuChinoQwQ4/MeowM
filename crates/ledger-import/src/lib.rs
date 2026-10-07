//! Offline bill-file decoding, preview, normalization, and reconciliation.
pub mod alipay;
pub mod csv;
pub mod dedupe;
pub mod normalize;
pub mod preview;
pub mod providers;
pub mod wechat;
use chrono::{DateTime, Utc};
pub use dedupe::DuplicateMatch;
use ledger_app::RepositoryError;
use ledger_domain::{
    AccountId, CurrencyCode, Money, Posting, SourceEvidence, Transaction, TransactionId,
    TransactionKind, TransactionStatus,
};
use std::fmt;
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    WeChat,
    Alipay,
    BankCsv,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    InvalidDate,
    InvalidAmount,
    UnknownDirection,
    MissingAmount,
    MissingRefundOriginal,
    MissingExchangeRate,
    MalformedRow,
    UnsettledTransaction,
    UnsupportedCurrency,
    TransferNeedsAccountMapping,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub row_number: usize,
    pub code: WarningCode,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImportRow {
    pub row_number: usize,
    pub time_text: String,
    pub amount_text: String,
    pub currency_text: String,
    pub direction_text: String,
    pub merchant: String,
    pub external_transaction_id: Option<String>,
    pub original_external_transaction_id: Option<String>,
    pub description: Option<String>,
    pub account_hint: Option<String>,
    pub source_fields_digest: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedImportRow {
    pub raw: RawImportRow,
    pub occurred_at: DateTime<Utc>,
    pub amount: Money,
    pub report_amount_cny: Option<Money>,
    pub kind: TransactionKind,
    pub status: TransactionStatus,
    pub merchant: Option<String>,
    pub account_hint: Option<String>,
    pub warnings: Vec<ParseWarning>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedImport {
    pub source: ImportSource,
    pub raw_rows: Vec<RawImportRow>,
    pub rows: Vec<NormalizedImportRow>,
    pub warnings: Vec<ParseWarning>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    InvalidEncoding(String),
    UnknownHeader(String),
    Csv(String),
    Repository(RepositoryError),
}
impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEncoding(s) | Self::UnknownHeader(s) | Self::Csv(s) => f.write_str(s),
            Self::Repository(error) => write!(f, "import storage failed: {error}"),
        }
    }
}
impl std::error::Error for ImportError {}
impl From<RepositoryError> for ImportError {
    fn from(value: RepositoryError) -> Self {
        Self::Repository(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    pub source: ImportSource,
    pub account_id: AccountId,
    pub transaction: Transaction,
    pub evidence: SourceEvidence,
    pub warnings: Vec<ParseWarning>,
    pub match_status: dedupe::DuplicateMatch,
}

impl ImportCandidate {
    pub fn batch_id(source: ImportSource, rows: &[RawImportRow]) -> Uuid {
        let mut digest = sha2::Sha256::new();
        use sha2::Digest;
        digest.update(format!("{:?}", source));
        for row in rows {
            digest.update(row.source_fields_digest);
        }
        Uuid::new_v5(&Uuid::NAMESPACE_OID, &digest.finalize())
    }

    pub fn from_normalized(
        source: ImportSource,
        row: NormalizedImportRow,
        account_id: AccountId,
        batch_id: Uuid,
    ) -> Self {
        Self::from_normalized_with_refund_parent(source, row, account_id, batch_id, None)
    }

    /// Creates a candidate with an explicitly mapped, confirmed refund parent.
    /// Without this mapping refunds remain posting-free and pending.
    pub fn from_normalized_with_refund_parent(
        source: ImportSource,
        row: NormalizedImportRow,
        account_id: AccountId,
        batch_id: Uuid,
        refund_parent: Option<TransactionId>,
    ) -> Self {
        let id = Uuid::new_v5(
            &batch_id,
            format!("{}:{:x?}", row.raw.row_number, row.raw.source_fields_digest).as_bytes(),
        );
        let minor = row.amount.minor_units();
        let posting = if matches!(row.kind, TransactionKind::Income | TransactionKind::Refund) {
            minor
        } else {
            -minor
        };
        let has_mapped_parent = row.kind != TransactionKind::Refund || refund_parent.is_some();
        let postings = if !has_mapped_parent || row.kind == TransactionKind::Transfer {
            Vec::new()
        } else {
            vec![Posting {
                account_id,
                delta: Money::new(posting, row.amount.currency().clone()),
            }]
        };
        let mut warnings = row.warnings;
        if row.kind == TransactionKind::Refund && refund_parent.is_none() {
            warnings.push(crate::normalize::warning(
                row.raw.row_number,
                WarningCode::MissingRefundOriginal,
            ));
        }
        if row.kind == TransactionKind::Transfer
            && !warnings
                .iter()
                .any(|warning| warning.code == WarningCode::TransferNeedsAccountMapping)
        {
            warnings.push(crate::normalize::warning(
                row.raw.row_number,
                WarningCode::TransferNeedsAccountMapping,
            ));
        }
        let transaction = Transaction {
            id: TransactionId::from_uuid(id),
            occurred_at: row.occurred_at,
            kind: row.kind,
            status: TransactionStatus::Pending,
            postings,
            category_id: None,
            original_transaction_id: refund_parent,
            report_amount_cny: if row.kind == TransactionKind::Transfer {
                None
            } else {
                row.report_amount_cny
            },
            merchant: row.merchant,
            description: row.raw.description.clone(),
        };
        let evidence = SourceEvidence {
            source_kind: dedupe::source_kind(source),
            external_transaction_id: row.raw.external_transaction_id.clone(),
            import_batch_id: batch_id,
            source_fields_digest: row.raw.source_fields_digest,
        };
        Self {
            source,
            account_id,
            transaction,
            evidence,
            warnings,
            match_status: dedupe::DuplicateMatch::NoMatch,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub new: usize,
    pub duplicate: usize,
    pub suspected: usize,
    pub warnings: usize,
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    pub candidates: Vec<ImportCandidate>,
    pub warnings: Vec<ParseWarning>,
    pub counts: ImportCounts,
    pub batch_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReceipt {
    pub batch_id: Uuid,
    pub inserted: usize,
    pub skipped: usize,
}

impl ImportPreview {
    pub fn build(
        rows: Vec<ImportCandidate>,
        existing: Vec<ledger_app::ImportRecord>,
    ) -> Result<Self, ImportError> {
        Self::build_with_warnings(rows, existing, Vec::new())
    }

    /// Includes file-level parser warnings for rows that could not form candidates.
    pub fn build_with_warnings(
        mut rows: Vec<ImportCandidate>,
        mut existing: Vec<ledger_app::ImportRecord>,
        parse_warnings: Vec<ParseWarning>,
    ) -> Result<Self, ImportError> {
        let batch_id = rows
            .first()
            .map(|row| row.evidence.import_batch_id)
            .unwrap_or_else(Uuid::new_v4);
        if rows
            .iter()
            .any(|row| row.evidence.import_batch_id != batch_id)
        {
            return Err(ImportError::Csv(
                "all rows in a preview must use the same import batch".into(),
            ));
        }
        let mut warnings = parse_warnings;
        warnings.extend(rows.iter().flat_map(|row| row.warnings.iter().cloned()));
        let mut counts = ImportCounts {
            warnings: warnings.len(),
            ..ImportCounts::default()
        };
        for row in &mut rows {
            row.match_status = dedupe::match_transaction(
                row.source,
                row.evidence.external_transaction_id.as_deref(),
                &row.transaction,
                row.account_id,
                &existing,
            );
            match row.match_status {
                dedupe::DuplicateMatch::ExactExternalId => counts.duplicate += 1,
                dedupe::DuplicateMatch::StrongCandidate => counts.suspected += 1,
                dedupe::DuplicateMatch::NoMatch => counts.new += 1,
            }
            counts.pending += usize::from(row.transaction.status == TransactionStatus::Pending);
            if row.match_status == dedupe::DuplicateMatch::NoMatch {
                existing.push(ledger_app::ImportRecord {
                    transaction: row.transaction.clone(),
                    evidence: row.evidence.clone(),
                });
            }
        }
        Ok(Self {
            candidates: rows,
            warnings,
            counts,
            batch_id,
        })
    }

    pub fn confirm(
        self,
        repository: &impl ledger_app::ImportRepository,
    ) -> Result<ImportReceipt, ImportError> {
        if self.candidates.iter().any(|item| {
            item.match_status == dedupe::DuplicateMatch::NoMatch
                && item.transaction.kind == TransactionKind::Transfer
        }) {
            return Err(ImportError::Csv(
                "转账或还款需要转出和转入账户；请排除此行后手工补录".into(),
            ));
        }
        let records: Vec<_> = self
            .candidates
            .into_iter()
            .filter(|item| item.match_status == dedupe::DuplicateMatch::NoMatch)
            .map(|mut item| {
                item.transaction.status = TransactionStatus::Confirmed;
                ledger_app::ImportRecord {
                    transaction: item.transaction,
                    evidence: item.evidence,
                }
            })
            .collect();
        let total = records.len();
        let inserted = repository.confirm_import_batch(self.batch_id, records)?;
        Ok(ImportReceipt {
            batch_id: self.batch_id,
            inserted,
            skipped: total - inserted,
        })
    }
}
pub fn parse_wechat_csv(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    wechat::parse(bytes)
}
pub fn parse_alipay_csv(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    alipay::parse(bytes)
}
pub(crate) fn digest(fields: &[String]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    hash.finalize().into()
}

pub(crate) fn money(minor: i64, code: &str) -> Result<Money, ImportError> {
    let exp = match code {
        "JPY" | "KRW" | "VND" => 0,
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" => 3,
        _ => 2,
    };
    CurrencyCode::new(code, exp)
        .map(|c| Money::new(minor, c))
        .map_err(|e| ImportError::Csv(e.to_string()))
}
