//! Provider-independent bill rows and deterministic amount/date normalization.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use ledger_domain::{CurrencyCode, SourceKind, TransactionKind};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    InvalidUtf8,
    EmptyInput,
    MissingHeaders(Vec<String>),
    UnknownFormat(String),
    InvalidDate { row: usize, value: String },
    MissingAmount { row: usize },
    InvalidAmount { row: usize, value: String },
    InvalidDirection { row: usize, value: String },
    Csv(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => f.write_str("账单必须是 UTF-8 文本"),
            Self::EmptyInput => f.write_str("账单文件为空"),
            Self::MissingHeaders(headers) => write!(f, "缺少必要表头：{}", headers.join("、")),
            Self::UnknownFormat(message) => write!(f, "无法识别账单格式：{message}"),
            Self::InvalidDate { row, value } => write!(f, "第 {row} 行日期无效：{value}"),
            Self::MissingAmount { row } => write!(f, "第 {row} 行缺少金额"),
            Self::InvalidAmount { row, value } => write!(f, "第 {row} 行金额无效：{value}"),
            Self::InvalidDirection { row, value } => write!(f, "第 {row} 行收支方向无效：{value}"),
            Self::Csv(message) => write!(f, "账单 CSV 无效：{message}"),
        }
    }
}
impl std::error::Error for ImportError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    pub row: usize,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedImportRow {
    pub row_number: usize,
    pub source_kind: SourceKind,
    pub external_transaction_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub kind: TransactionKind,
    /// Absolute amount in the source currency's minor unit. Direction is in `kind`.
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub merchant: Option<String>,
    pub description: Option<String>,
    /// Digest of normalized fields. Raw CSV text is intentionally not retained.
    pub source_fields_digest: [u8; 32],
    pub warnings: Vec<ImportWarning>,
}

impl NormalizedImportRow {
    pub fn digest_for(
        source: SourceKind,
        external_id: Option<&str>,
        occurred_at: DateTime<Utc>,
        kind: TransactionKind,
        amount_minor: i64,
        currency: &CurrencyCode,
        merchant: Option<&str>,
        description: Option<&str>,
    ) -> [u8; 32] {
        let values = [
            source_name(source).to_owned(),
            external_id.unwrap_or_default().to_owned(),
            occurred_at.to_rfc3339(),
            kind_name(kind).to_owned(),
            amount_minor.to_string(),
            currency.code().to_owned(),
            currency.minor_unit_exponent().to_string(),
            merchant.unwrap_or_default().to_owned(),
            description.unwrap_or_default().to_owned(),
        ];
        let mut hasher = Sha256::new();
        for value in values {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        hasher.finalize().into()
    }
}

pub(crate) fn source_name(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Alipay => "alipay",
        SourceKind::WeChat => "wechat",
        SourceKind::Bank => "bank",
        SourceKind::Notification => "notification",
        SourceKind::Manual => "manual",
    }
}

pub(crate) fn kind_name(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Transfer => "transfer",
        TransactionKind::Refund => "refund",
        TransactionKind::BalanceAdjustment => "balance_adjustment",
    }
}

pub(crate) fn normalize_text(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && !["/", "--", "-"].contains(&value)).then(|| value.to_owned())
}

pub(crate) fn parse_date(value: &str, row: usize) -> Result<DateTime<Utc>, ImportError> {
    let raw = value.trim().trim_matches('\u{feff}');
    let raw = raw.replace('年', "-").replace('月', "-").replace('日', "");
    let raw = raw.replace('/', "-");
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(value) = NaiveDateTime::parse_from_str(&raw, format) {
            return Local
                .from_local_datetime(&value)
                .earliest()
                .map(|time| time.with_timezone(&Utc))
                .ok_or_else(|| ImportError::InvalidDate {
                    row,
                    value: raw.clone(),
                });
        }
    }
    if let Ok(value) = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
        return Local
            .from_local_datetime(&value.and_hms_opt(0, 0, 0).expect("midnight is valid"))
            .earliest()
            .map(|time| time.with_timezone(&Utc))
            .ok_or_else(|| ImportError::InvalidDate {
                row,
                value: raw.clone(),
            });
    }
    Err(ImportError::InvalidDate {
        row,
        value: value.to_owned(),
    })
}

/// Parse a decimal amount into integer minor units without binary floating point.
pub(crate) fn parse_minor(
    value: &str,
    exponent: u8,
    row: usize,
) -> Result<(i64, bool), ImportError> {
    let mut raw = value
        .trim()
        .replace(',', "")
        .replace('¥', "")
        .replace('￥', "");
    if raw.is_empty() {
        return Err(ImportError::MissingAmount { row });
    }
    let mut negative = false;
    if raw.starts_with('(') && raw.ends_with(')') {
        negative = true;
        raw = raw[1..raw.len() - 1].to_owned();
    }
    if let Some(rest) = raw.strip_prefix('+') {
        raw = rest.to_owned();
    } else if let Some(rest) = raw.strip_prefix('-') {
        negative = true;
        raw = rest.to_owned();
    }
    let mut pieces = raw.split('.');
    let whole = pieces.next().unwrap_or_default();
    let fraction = pieces.next().unwrap_or_default();
    if pieces.next().is_some()
        || whole.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
        || fraction.chars().count() > exponent as usize
    {
        return Err(ImportError::InvalidAmount {
            row,
            value: value.to_owned(),
        });
    }
    let scale =
        10_i64
            .checked_pow(u32::from(exponent))
            .ok_or_else(|| ImportError::InvalidAmount {
                row,
                value: value.to_owned(),
            })?;
    let whole = whole
        .parse::<i64>()
        .map_err(|_| ImportError::InvalidAmount {
            row,
            value: value.to_owned(),
        })?;
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| ImportError::InvalidAmount {
                row,
                value: value.to_owned(),
            })?
            * 10_i64.pow(u32::from(exponent) - fraction.chars().count() as u32)
    };
    let amount = whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction_value))
        .ok_or_else(|| ImportError::InvalidAmount {
            row,
            value: value.to_owned(),
        })?;
    Ok((
        if negative {
            amount
                .checked_neg()
                .ok_or_else(|| ImportError::InvalidAmount {
                    row,
                    value: value.to_owned(),
                })?
        } else {
            amount
        },
        negative,
    ))
}

pub(crate) fn cny() -> CurrencyCode {
    CurrencyCode::new("CNY", 2).expect("CNY is a valid currency")
}

/// Provider-independent direction handling; explicit directions outrank amount
/// signs. Bank exports may infer direction from a sign only when direction is absent.
pub(crate) fn direction_kind(
    type_text: &str,
    direction: &str,
    negative: bool,
    infer_sign: bool,
    row: usize,
) -> Result<TransactionKind, ImportError> {
    let text = format!("{type_text}{direction}");
    if text.contains("退款") || text.contains("退回") {
        return Ok(TransactionKind::Refund);
    }
    if ["支出", "付款", "转出", "借"]
        .iter()
        .any(|label| text.contains(label))
    {
        return Ok(TransactionKind::Expense);
    }
    if ["收入", "收款", "转入", "贷"]
        .iter()
        .any(|label| text.contains(label))
    {
        return Ok(TransactionKind::Income);
    }
    if direction.trim().is_empty() && (infer_sign || negative) {
        return Ok(if negative {
            TransactionKind::Expense
        } else {
            TransactionKind::Income
        });
    }
    Err(ImportError::InvalidDirection {
        row,
        value: direction.to_owned(),
    })
}
