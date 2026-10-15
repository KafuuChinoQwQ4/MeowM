use super::*;

pub(super) struct TransactionSqlValues {
    pub(super) id: String,
    pub(super) occurred_at: i64,
    pub(super) kind: &'static str,
    pub(super) status: &'static str,
    pub(super) category_id: Option<String>,
    pub(super) original_transaction_id: Option<String>,
    pub(super) report_amount_minor: Option<i64>,
    pub(super) report_currency_code: Option<String>,
    pub(super) report_currency_exponent: Option<u8>,
    pub(super) merchant: Option<String>,
    pub(super) description: Option<String>,
}

pub(super) fn transaction_values(transaction: &Transaction) -> TransactionSqlValues {
    let report = transaction.report_amount_cny.as_ref();
    TransactionSqlValues {
        id: transaction.id.as_uuid().to_string(),
        occurred_at: transaction.occurred_at.timestamp_micros(),
        kind: transaction_kind_str(transaction.kind),
        status: transaction_status_str(transaction.status),
        category_id: transaction.category_id.map(|id| id.as_uuid().to_string()),
        original_transaction_id: transaction
            .original_transaction_id
            .map(|id| id.as_uuid().to_string()),
        report_amount_minor: report.map(Money::minor_units),
        report_currency_code: report.map(|m| m.currency().code().to_owned()),
        report_currency_exponent: report.map(|m| m.currency().minor_unit_exponent()),
        merchant: transaction.merchant.clone(),
        description: transaction.description.clone(),
    }
}

pub(super) fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let id: String = row.get(0)?;
    let kind: String = row.get(2)?;
    let code: String = row.get(3)?;
    let exponent: u8 = row.get(4)?;
    let amount: Option<i64> = row.get(5)?;
    let effective: Option<i64> = row.get(6)?;
    Ok(Account {
        id: AccountId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        display_name: row.get(1)?,
        kind: parse_account_kind(&kind).map_err(to_sql_conversion_error)?,
        currency: CurrencyCode::new(&code, exponent)
            .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        opening_balance: amount
            .map(|amount| {
                CurrencyCode::new(&code, exponent).map(|currency| Money::new(amount, currency))
            })
            .transpose()
            .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        opening_balance_effective_at: effective
            .map(parse_timestamp)
            .transpose()
            .map_err(to_sql_conversion_error)?,
    })
}

pub(super) fn category_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    let id: String = row.get(0)?;
    let enabled: i64 = row.get(2)?;
    Ok(Category {
        id: CategoryId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        name: row.get(1)?,
        enabled: match enabled {
            0 => false,
            1 => true,
            value => {
                return Err(to_sql_conversion_error(format!(
                    "invalid category enabled value {value}"
                )));
            }
        },
    })
}

pub(super) fn classification_rule_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ClassificationRule> {
    let id: String = row.get(0)?;
    let priority: i64 = row.get(1)?;
    let category_id: String = row.get(4)?;
    let enabled: i64 = row.get(5)?;
    Ok(ClassificationRule {
        id: RuleId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
        priority: u32::try_from(priority)
            .map_err(|_| to_sql_conversion_error(format!("invalid rule priority {priority}")))?,
        merchant_contains: row.get(2)?,
        description_contains: row.get(3)?,
        category_id: CategoryId::from_uuid(
            parse_uuid(&category_id).map_err(to_sql_conversion_error)?,
        ),
        enabled: match enabled {
            0 => false,
            1 => true,
            value => {
                return Err(to_sql_conversion_error(format!(
                    "invalid rule enabled value {value}"
                )));
            }
        },
    })
}

pub(super) fn transaction_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(Transaction, Result<Option<Money>, RepositoryError>)> {
    let id: String = row.get(0)?;
    let occurred: i64 = row.get(1)?;
    let kind: String = row.get(2)?;
    let status: String = row.get(3)?;
    let category: Option<String> = row.get(4)?;
    let original: Option<String> = row.get(5)?;
    let amount: Option<i64> = row.get(6)?;
    let code: Option<String> = row.get(7)?;
    let exponent: Option<u8> = row.get(8)?;
    let merchant: Option<String> = row.get(9)?;
    let description: Option<String> = row.get(10)?;
    let report = match (amount, code, exponent) {
        (None, None, None) => Ok(None),
        (Some(amount), Some(code), Some(exponent)) => CurrencyCode::new(&code, exponent)
            .map(|currency| Some(Money::new(amount, currency)))
            .map_err(|error| RepositoryError::InvalidData(error.to_string())),
        _ => Err(RepositoryError::InvalidData(
            "incomplete report amount fields".into(),
        )),
    };
    Ok((
        Transaction {
            id: TransactionId::from_uuid(parse_uuid(&id).map_err(to_sql_conversion_error)?),
            occurred_at: parse_timestamp(occurred).map_err(to_sql_conversion_error)?,
            kind: parse_transaction_kind(&kind).map_err(to_sql_conversion_error)?,
            status: parse_transaction_status(&status).map_err(to_sql_conversion_error)?,
            postings: Vec::new(),
            category_id: category
                .map(|id| parse_uuid(&id).map(CategoryId::from_uuid))
                .transpose()
                .map_err(to_sql_conversion_error)?,
            original_transaction_id: original
                .map(|id| parse_uuid(&id).map(TransactionId::from_uuid))
                .transpose()
                .map_err(to_sql_conversion_error)?,
            report_amount_cny: None,
            merchant,
            description,
        },
        report,
    ))
}

pub(super) fn posting_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Posting> {
    let account: String = row.get(0)?;
    let amount: i64 = row.get(1)?;
    let code: String = row.get(2)?;
    let exponent: u8 = row.get(3)?;
    Ok(Posting {
        account_id: AccountId::from_uuid(parse_uuid(&account).map_err(to_sql_conversion_error)?),
        delta: Money::new(
            amount,
            CurrencyCode::new(&code, exponent)
                .map_err(|e| to_sql_conversion_error(e.to_string()))?,
        ),
    })
}

pub(super) fn parse_timestamp(micros: i64) -> Result<DateTime<Utc>, String> {
    DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| "invalid Unix microsecond timestamp".into())
}

pub(super) fn parse_uuid(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| error.to_string())
}

pub(super) fn to_sql_conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error)),
    )
}

pub(super) fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

pub(super) fn map_insert_error(error: rusqlite::Error) -> RepositoryError {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            RepositoryError::Conflict
        }
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            RepositoryError::InvalidData(error.to_string())
        }
        _ => storage_error(error),
    }
}

pub(super) fn account_kind_str(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
    }
}
pub(super) fn parse_account_kind(value: &str) -> Result<AccountKind, String> {
    match value {
        "asset" => Ok(AccountKind::Asset),
        "liability" => Ok(AccountKind::Liability),
        _ => Err(format!("unknown account kind {value}")),
    }
}
pub(super) fn transaction_kind_str(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::Transfer => "transfer",
        TransactionKind::Refund => "refund",
        TransactionKind::BalanceAdjustment => "balance_adjustment",
    }
}
pub(super) fn parse_transaction_kind(value: &str) -> Result<TransactionKind, String> {
    match value {
        "income" => Ok(TransactionKind::Income),
        "expense" => Ok(TransactionKind::Expense),
        "transfer" => Ok(TransactionKind::Transfer),
        "refund" => Ok(TransactionKind::Refund),
        "balance_adjustment" => Ok(TransactionKind::BalanceAdjustment),
        _ => Err(format!("unknown transaction kind {value}")),
    }
}
pub(super) fn transaction_status_str(status: TransactionStatus) -> &'static str {
    match status {
        TransactionStatus::Pending => "pending",
        TransactionStatus::Confirmed => "confirmed",
        TransactionStatus::Ignored => "ignored",
    }
}
pub(super) fn parse_transaction_status(value: &str) -> Result<TransactionStatus, String> {
    match value {
        "pending" => Ok(TransactionStatus::Pending),
        "confirmed" => Ok(TransactionStatus::Confirmed),
        "ignored" => Ok(TransactionStatus::Ignored),
        _ => Err(format!("unknown transaction status {value}")),
    }
}
pub(super) fn source_kind_str(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Alipay => "alipay",
        SourceKind::WeChat => "wechat",
        SourceKind::Bank => "bank",
        SourceKind::Notification => "notification",
        SourceKind::Manual => "manual",
    }
}
