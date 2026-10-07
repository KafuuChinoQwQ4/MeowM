//! Optional provider boundary. Only synthetic HTTP contracts exist today; no live
//! endpoints, transport implementation, OAuth claim, or background sync is supplied.
pub mod alipay;
pub mod bank;
pub mod wechat;
use crate::{ImportCandidate, ImportPreview, ImportSource, NormalizedImportRow, RawImportRow};
use chrono::{DateTime, Utc};
use ledger_app::ports::{CredentialStore, ProviderId, SecretToken};
use ledger_app::{MobileRepository, MobileSettings};
use ledger_domain::{AccountId, AccountKind, Money, TransactionKind, TransactionStatus};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Disabled,
    AuthorizationRequired,
    Network,
    RateLimited { retry_after_seconds: Option<u64> },
    Unavailable,
    InvalidResponse,
    InvalidRange,
    PaginationLoop,
    LimitExceeded,
    CredentialStore,
    Repository,
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Disabled => "Provider is disabled",
            Self::AuthorizationRequired => "Authorization is required",
            Self::Network => "Network failed; retry the same time range",
            Self::RateLimited { .. } => "Rate limited; retry after the indicated delay",
            Self::Unavailable => "Live provider is not connected",
            Self::InvalidResponse => "Provider response is invalid",
            Self::InvalidRange => "Time range is invalid",
            Self::PaginationLoop => "Repeated cursor; restart the same time range",
            Self::LimitExceeded => "Response limit exceeded; select a smaller time range",
            Self::CredentialStore => "Secure credential storage is unavailable",
            Self::Repository => "Ledger operation failed",
        })
    }
}
impl std::error::Error for ProviderError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    from: DateTime<Utc>,
    until: DateTime<Utc>,
}
impl TimeRange {
    pub fn new(from: DateTime<Utc>, until: DateTime<Utc>) -> Result<Self, ProviderError> {
        if from >= until {
            Err(ProviderError::InvalidRange)
        } else {
            Ok(Self { from, until })
        }
    }
    pub fn from(&self) -> DateTime<Utc> {
        self.from
    }
    pub fn until(&self) -> DateTime<Utc> {
        self.until
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Authorize,
    Page {
        range: TimeRange,
        cursor: Option<String>,
    },
    Revoke,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub provider: ProviderId,
    pub operation: Operation,
}
/// Transient HTTP data: intentionally no Debug or error-body propagation.
pub struct HttpResponse {
    pub status: u16,
    pub retry_after_seconds: Option<u64>,
    pub body: Vec<u8>,
}
/// Credentials are a separate argument so requests/cursors cannot log headers.
pub trait HttpTransport {
    fn execute(
        &mut self,
        request: &ProviderRequest,
        token: &SecretToken,
    ) -> Result<HttpResponse, ProviderError>;
}
pub struct UnavailableTransport;
impl HttpTransport for UnavailableTransport {
    fn execute(
        &mut self,
        _: &ProviderRequest,
        _: &SecretToken,
    ) -> Result<HttpResponse, ProviderError> {
        Err(ProviderError::Unavailable)
    }
}
pub struct ProviderPage {
    pub rows: Vec<NormalizedImportRow>,
    pub next_cursor: Option<String>,
}
pub trait ProviderAdapter {
    fn id(&self) -> ProviderId;
    fn source(&self) -> ImportSource;
    /// Synthetic contract decoder. Must be replaced after a specific provider is evaluated.
    fn decode_page(&self, body: &[u8]) -> Result<ProviderPage, ProviderError>;
    fn authorize(&self) -> ProviderRequest {
        ProviderRequest {
            provider: self.id(),
            operation: Operation::Authorize,
        }
    }
    fn page(&self, range: TimeRange, cursor: Option<String>) -> ProviderRequest {
        ProviderRequest {
            provider: self.id(),
            operation: Operation::Page { range, cursor },
        }
    }
    fn revoke(&self) -> ProviderRequest {
        ProviderRequest {
            provider: self.id(),
            operation: Operation::Revoke,
        }
    }
}
fn enabled(s: &MobileSettings, p: ProviderId) -> bool {
    s.api_enabled
        && match p {
            ProviderId::WeChat => s.api_wechat,
            ProviderId::Alipay => s.api_alipay,
            ProviderId::Bank => s.api_bank,
        }
}
fn response(r: HttpResponse) -> Result<Vec<u8>, ProviderError> {
    match r.status {
        401 | 403 => Err(ProviderError::AuthorizationRequired),
        429 => Err(ProviderError::RateLimited {
            retry_after_seconds: r.retry_after_seconds,
        }),
        500..=599 => Err(ProviderError::Network),
        200..=299 if r.body.len() <= 1_048_576 => Ok(r.body),
        200..=299 => Err(ProviderError::LimitExceeded),
        _ => Err(ProviderError::InvalidResponse),
    }
}
pub fn authorize(
    adapter: &dyn ProviderAdapter,
    http: &mut dyn HttpTransport,
    store: &mut dyn CredentialStore,
    settings: &MobileSettings,
    token: SecretToken,
) -> Result<(), ProviderError> {
    if !enabled(settings, adapter.id()) {
        return Err(ProviderError::Disabled);
    }
    if token.expose().is_empty() || token.expose().len() > 16_384 {
        return Err(ProviderError::AuthorizationRequired);
    }
    let result = http
        .execute(&adapter.authorize(), &token)
        .and_then(response);
    if result == Err(ProviderError::AuthorizationRequired) {
        store
            .delete(adapter.id())
            .map_err(|_| ProviderError::CredentialStore)?;
    }
    result?;
    store
        .save(adapter.id(), &token)
        .map_err(|_| ProviderError::CredentialStore)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revocation {
    RemoteAndLocal,
    LocalOnly,
}
/// Always erase local credentials first, including when remote revocation fails.
/// LocalOnly tells the future UI to direct users to the provider's own grant page.
pub fn revoke(
    adapter: &dyn ProviderAdapter,
    http: &mut dyn HttpTransport,
    store: &mut dyn CredentialStore,
) -> Result<Revocation, ProviderError> {
    let token = store.load(adapter.id());
    store
        .delete(adapter.id())
        .map_err(|_| ProviderError::CredentialStore)?;
    match token {
        Ok(Some(token)) => match http.execute(&adapter.revoke(), &token).and_then(response) {
            Ok(_) => Ok(Revocation::RemoteAndLocal),
            Err(_) => Ok(Revocation::LocalOnly),
        },
        _ => Ok(Revocation::LocalOnly),
    }
}
/// Fetch all pages before producing the shared Pending preview. Failures discard
/// transient pages; retrying the same range deduplicates against persisted evidence.
/// No ledger writes or persistent cursor advancement happen here.
pub fn sync_preview(
    adapter: &dyn ProviderAdapter,
    http: &mut dyn HttpTransport,
    store: &mut dyn CredentialStore,
    repo: &impl MobileRepository,
    account_id: AccountId,
    range: TimeRange,
) -> Result<ImportPreview, ProviderError> {
    if !enabled(
        &repo.settings().map_err(|_| ProviderError::Repository)?,
        adapter.id(),
    ) {
        return Err(ProviderError::Disabled);
    }
    let token = store
        .load(adapter.id())
        .map_err(|_| ProviderError::CredentialStore)?
        .ok_or(ProviderError::AuthorizationRequired)?;
    if token.expose().is_empty() || token.expose().len() > 16_384 {
        return Err(ProviderError::AuthorizationRequired);
    }
    let account = repo
        .get_account(account_id)
        .map_err(|_| ProviderError::Repository)?
        .ok_or(ProviderError::Repository)?;
    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for page_index in 0..128 {
        // A concurrent settings change takes effect at the next request boundary.
        if !enabled(
            &repo.settings().map_err(|_| ProviderError::Repository)?,
            adapter.id(),
        ) {
            return Err(ProviderError::Disabled);
        }
        let bytes = match http
            .execute(&adapter.page(range, cursor.clone()), &token)
            .and_then(response)
        {
            Err(ProviderError::AuthorizationRequired) => {
                store
                    .delete(adapter.id())
                    .map_err(|_| ProviderError::CredentialStore)?;
                return Err(ProviderError::AuthorizationRequired);
            }
            other => other?,
        };
        let page = adapter.decode_page(&bytes)?;
        if page.rows.iter().any(|r| {
            r.occurred_at < range.from
                || r.occurred_at >= range.until
                || r.amount.currency() != &account.currency
        }) {
            return Err(ProviderError::InvalidResponse);
        }
        rows.extend(page.rows);
        if rows.len() > 10_000 {
            return Err(ProviderError::LimitExceeded);
        }
        cursor = page.next_cursor;
        match cursor.as_ref() {
            None => break,
            Some(c) if c.is_empty() || c.len() > 512 => return Err(ProviderError::InvalidResponse),
            Some(c) if !seen.insert(c.clone()) => return Err(ProviderError::PaginationLoop),
            _ => {}
        }
        if page_index == 127 {
            return Err(ProviderError::LimitExceeded);
        }
    }
    let raw: Vec<_> = rows
        .iter_mut()
        .enumerate()
        .map(|(i, r)| {
            r.raw.row_number = i + 1;
            r.raw.clone()
        })
        .collect();
    let batch = ImportCandidate::batch_id(adapter.source(), &raw);
    let rules = repo.list_rules().map_err(|_| ProviderError::Repository)?;
    let categories = repo
        .list_categories()
        .map_err(|_| ProviderError::Repository)?;
    let candidates = rows
        .into_iter()
        .map(|row| {
            let mut c = ImportCandidate::from_normalized(adapter.source(), row, account_id, batch);
            if account.kind == AccountKind::Liability {
                for p in &mut c.transaction.postings {
                    p.delta = Money::new(-p.delta.minor_units(), p.delta.currency().clone());
                }
            }
            c.transaction.category_id =
                ledger_app::classification::suggest_category(&c.transaction, &rules, &categories);
            c
        })
        .collect();
    ImportPreview::build(
        candidates,
        repo.list_import_records()
            .map_err(|_| ProviderError::Repository)?,
    )
    .map_err(|_| ProviderError::Repository)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureRow {
    id: String,
    occurred_at: String,
    amount: String,
    currency: String,
    direction: String,
    merchant: String,
    #[serde(default)]
    original_id: Option<String>,
}
fn decode(body: &[u8], field: &str) -> Result<ProviderPage, ProviderError> {
    let mut object: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(body).map_err(|_| ProviderError::InvalidResponse)?;
    let input: Vec<FixtureRow> =
        serde_json::from_value(object.remove(field).ok_or(ProviderError::InvalidResponse)?)
            .map_err(|_| ProviderError::InvalidResponse)?;
    let cursor = object
        .remove("next_cursor")
        .ok_or(ProviderError::InvalidResponse)?;
    let next_cursor: Option<String> =
        serde_json::from_value(cursor).map_err(|_| ProviderError::InvalidResponse)?;
    if !object.is_empty() {
        return Err(ProviderError::InvalidResponse);
    }
    let mut rows = Vec::new();
    for (i, r) in input.into_iter().enumerate() {
        if r.id.is_empty() || r.id.len() > 512 || r.merchant.len() > 4096 {
            return Err(ProviderError::InvalidResponse);
        }
        let occurred_at = DateTime::parse_from_rfc3339(&r.occurred_at)
            .map_err(|_| ProviderError::InvalidResponse)?
            .with_timezone(&Utc);
        let amount = crate::normalize::amount(&r.amount, &r.currency)
            .map_err(|_| ProviderError::InvalidResponse)?;
        if amount.minor_units() <= 0 {
            return Err(ProviderError::InvalidResponse);
        }
        let kind = match r.direction.as_str() {
            "expense" => TransactionKind::Expense,
            "income" => TransactionKind::Income,
            "refund" => TransactionKind::Refund,
            _ => return Err(ProviderError::InvalidResponse),
        };
        let digest = crate::digest(&[
            r.id.clone(),
            r.occurred_at.clone(),
            r.amount.clone(),
            r.currency.clone(),
            r.direction.clone(),
            r.merchant.clone(),
            r.original_id.clone().unwrap_or_default(),
        ]);
        let raw = RawImportRow {
            row_number: i + 1,
            time_text: r.occurred_at,
            amount_text: r.amount,
            currency_text: r.currency,
            direction_text: r.direction,
            merchant: r.merchant.clone(),
            external_transaction_id: Some(r.id),
            original_external_transaction_id: r.original_id,
            description: None,
            account_hint: None,
            source_fields_digest: digest,
        };
        let mut warnings = Vec::new();
        let report_amount_cny = if amount.currency().code() == "CNY" {
            Some(amount.clone())
        } else {
            warnings.push(crate::normalize::warning(
                i + 1,
                crate::WarningCode::MissingExchangeRate,
            ));
            None
        };
        rows.push(NormalizedImportRow {
            raw,
            occurred_at,
            amount,
            report_amount_cny,
            kind,
            status: TransactionStatus::Pending,
            merchant: Some(r.merchant),
            account_hint: None,
            warnings,
        });
    }
    Ok(ProviderPage { rows, next_cursor })
}
