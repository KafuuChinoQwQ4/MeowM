use chrono::{DateTime, Utc};
use ledger_app::ports::{CredentialError, CredentialStore, ProviderId, SecretToken};
use ledger_app::{BackupRepository, LedgerRepository, MobileLedger, MobileSettings};
use ledger_domain::{AccountKind, CurrencyCode, Money, TransactionStatus};
use ledger_import::providers::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
struct MemoryCredentials(HashMap<ProviderId, Vec<u8>>);
impl CredentialStore for MemoryCredentials {
    fn load(&mut self, p: ProviderId) -> Result<Option<SecretToken>, CredentialError> {
        Ok(self.0.get(&p).cloned().map(SecretToken::new))
    }
    fn save(&mut self, p: ProviderId, t: &SecretToken) -> Result<(), CredentialError> {
        self.0.insert(p, t.expose().to_vec());
        Ok(())
    }
    fn delete(&mut self, p: ProviderId) -> Result<(), CredentialError> {
        self.0.remove(&p);
        Ok(())
    }
}
struct FixtureHttp {
    replies: VecDeque<Result<HttpResponse, ProviderError>>,
    calls: Vec<ProviderRequest>,
}
impl HttpTransport for FixtureHttp {
    fn execute(
        &mut self,
        r: &ProviderRequest,
        _: &SecretToken,
    ) -> Result<HttpResponse, ProviderError> {
        self.calls.push(r.clone());
        self.replies.pop_front().expect("unexpected HTTP call")
    }
}
fn http(bodies: &[&str]) -> FixtureHttp {
    FixtureHttp {
        replies: bodies
            .iter()
            .map(|b| {
                Ok(HttpResponse {
                    status: 200,
                    retry_after_seconds: None,
                    body: b.as_bytes().to_vec(),
                })
            })
            .collect(),
        calls: vec![],
    }
}
fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn range() -> TimeRange {
    TimeRange::new(at("2026-10-01T00:00:00Z"), at("2026-10-02T00:00:00Z")).unwrap()
}
fn setup() -> (
    MobileLedger<SqliteLedgerRepository>,
    ledger_domain::AccountId,
    MemoryCredentials,
) {
    let app = MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap());
    let account = app
        .create_account(
            "Fixture wallet",
            AccountKind::Asset,
            Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
            at("2026-01-01T00:00:00Z"),
        )
        .unwrap();
    app.save_settings(MobileSettings {
        api_enabled: true,
        api_wechat: true,
        api_alipay: true,
        api_bank: true,
        ..Default::default()
    })
    .unwrap();
    let mut store = MemoryCredentials::default();
    for p in [ProviderId::WeChat, ProviderId::Alipay, ProviderId::Bank] {
        store
            .save(p, &SecretToken::new(b"fixture-token".to_vec()))
            .unwrap();
    }
    (app, account, store)
}
#[test]
fn independent_adapters_share_pending_preview_and_explicit_idempotent_confirmation() {
    for (adapter, body) in [
        (
            &wechat::WeChatAdapter as &dyn ProviderAdapter,
            include_str!("fixtures/providers/wechat.json"),
        ),
        (
            &alipay::AlipayAdapter,
            include_str!("fixtures/providers/alipay.json"),
        ),
        (
            &bank::BankAdapter,
            include_str!("fixtures/providers/bank.json"),
        ),
    ] {
        let (app, account, mut store) = setup();
        let mut transport = http(&[body, body]);
        let preview = sync_preview(
            adapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range(),
        )
        .unwrap();
        assert_eq!(preview.counts.new, 1);
        assert_eq!(
            preview.candidates[0].transaction.status,
            TransactionStatus::Pending
        );
        assert!(app.repository().list_transactions().unwrap().is_empty());
        assert_eq!(preview.clone().confirm(&app).unwrap().inserted, 1);
        assert_eq!(preview.confirm(&app).unwrap().inserted, 0);
        let retry = sync_preview(
            adapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range(),
        )
        .unwrap();
        assert_eq!(retry.counts.duplicate, 1);
        assert_eq!(
            app.repository()
                .balance_as_of(account, range().until())
                .unwrap()
                .minor_units(),
            8750
        );
        assert_eq!(
            transport.calls[0].operation,
            Operation::Page {
                range: range(),
                cursor: None
            }
        );
    }
}
#[test]
fn disabled_or_missing_credentials_never_calls_transport() {
    let (app, account, mut store) = setup();
    let mut transport = http(&[]);
    app.save_settings(MobileSettings::default()).unwrap();
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::Disabled
    );
    app.save_settings(MobileSettings {
        api_enabled: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::Disabled
    );
    app.save_settings(MobileSettings {
        api_enabled: true,
        api_wechat: true,
        ..Default::default()
    })
    .unwrap();
    store.delete(ProviderId::WeChat).unwrap();
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::AuthorizationRequired
    );
    assert!(transport.calls.is_empty());
}
#[test]
fn pagination_errors_are_recoverable_without_partial_writes_or_cursor_loss() {
    let first = include_str!("fixtures/providers/wechat.json").replace("null", "\"page-2\"");
    for err in [
        ProviderError::Network,
        ProviderError::RateLimited {
            retry_after_seconds: Some(30),
        },
    ] {
        let (app, account, mut store) = setup();
        let mut transport = http(&[&first]);
        transport.replies.push_back(Err(err.clone()));
        assert_eq!(
            sync_preview(
                &wechat::WeChatAdapter,
                &mut transport,
                &mut store,
                app.repository(),
                account,
                range()
            )
            .unwrap_err(),
            err
        );
        assert!(app.repository().list_transactions().unwrap().is_empty());
        let mut retry = http(&[&first, include_str!("fixtures/providers/wechat.json")]);
        let p = sync_preview(
            &wechat::WeChatAdapter,
            &mut retry,
            &mut store,
            app.repository(),
            account,
            range(),
        )
        .unwrap();
        assert_eq!((p.counts.new, p.counts.duplicate), (1, 1));
        assert_eq!(p.confirm(&app).unwrap().inserted, 1);
        assert_eq!(
            retry.calls[1].operation,
            Operation::Page {
                range: range(),
                cursor: Some("page-2".into())
            }
        );
    }
    let (app, account, mut store) = setup();
    let mut cycle = http(&[&first, &first]);
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut cycle,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::PaginationLoop
    );
    assert!(app.repository().list_transactions().unwrap().is_empty());
}
#[test]
fn authorization_revocation_and_sanitized_http_failures() {
    let (app, account, mut store) = setup();
    let mut auth = http(&["{}"]);
    authorize(
        &alipay::AlipayAdapter,
        &mut auth,
        &mut store,
        &app.settings().unwrap(),
        SecretToken::new(b"new-fixture-token".to_vec()),
    )
    .unwrap();
    assert_eq!(auth.calls[0].operation, Operation::Authorize);
    let mut revoked = http(&[]);
    revoked.replies.push_back(Ok(HttpResponse {
        status: 401,
        retry_after_seconds: None,
        body: b"DO NOT ECHO server secret".to_vec(),
    }));
    assert_eq!(
        sync_preview(
            &alipay::AlipayAdapter,
            &mut revoked,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::AuthorizationRequired
    );
    assert!(store.load(ProviderId::Alipay).unwrap().is_none());
    let mut network = http(&[]);
    network.replies.push_back(Err(ProviderError::Network));
    assert_eq!(
        revoke(&wechat::WeChatAdapter, &mut network, &mut store).unwrap(),
        Revocation::LocalOnly
    );
    assert!(store.load(ProviderId::WeChat).unwrap().is_none());
    assert!(!format!("{:?}", SecretToken::new(b"NEVER-ECHO".to_vec())).contains("NEVER-ECHO"));
}
#[test]
fn provider_grants_persist_and_backup_restore_keeps_current_local_consent() {
    let (app, _, _) = setup();
    let archive = app.repository().backup_snapshot().unwrap();
    assert!(!archive.tables.contains_key("provider_settings"));
    app.save_settings(MobileSettings {
        api_enabled: false,
        api_alipay: true,
        ..Default::default()
    })
    .unwrap();
    app.repository().restore_snapshot(archive).unwrap();
    let settings = app.settings().unwrap();
    assert!(!settings.api_enabled);
    assert!(!settings.api_wechat);
    assert!(settings.api_alipay);
    assert!(!settings.api_bank);
}

#[test]
fn status_mapping_malformed_amount_range_and_unavailable_are_sanitized() {
    let body = include_str!("fixtures/providers/wechat.json");
    for (status, error) in [
        (
            429,
            ProviderError::RateLimited {
                retry_after_seconds: Some(60),
            },
        ),
        (503, ProviderError::Network),
        (400, ProviderError::InvalidResponse),
    ] {
        let (app, account, mut store) = setup();
        let mut transport = http(&[]);
        transport.replies.push_back(Ok(HttpResponse {
            status,
            retry_after_seconds: Some(60),
            body: b"sensitive untrusted body".to_vec(),
        }));
        let actual = sync_preview(
            &wechat::WeChatAdapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range(),
        )
        .unwrap_err();
        assert_eq!(actual, error);
        assert!(!actual.to_string().contains("sensitive"));
        assert!(app.repository().list_transactions().unwrap().is_empty());
    }
    for malformed in [
        body.replace("12.50", "NaN"),
        body.replace("12.50", "0"),
        body.replace("12.50", "1.234"),
        body.replace("2026-10-01", "2026-10-05"),
        body.replace("CNY", "USD"),
        body.replace("expense", "transfer"),
    ] {
        let (app, account, mut store) = setup();
        let mut transport = http(&[&malformed]);
        assert_eq!(
            sync_preview(
                &wechat::WeChatAdapter,
                &mut transport,
                &mut store,
                app.repository(),
                account,
                range()
            )
            .unwrap_err(),
            ProviderError::InvalidResponse
        );
        assert!(app.repository().list_transactions().unwrap().is_empty());
    }
    let (app, account, mut store) = setup();
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut UnavailableTransport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::Unavailable
    );
    assert_eq!(
        TimeRange::new(range().until(), range().from()).unwrap_err(),
        ProviderError::InvalidRange
    );
}

#[test]
fn classification_liability_sign_and_atomic_refund_rejection_use_shared_ledger_rules() {
    use ledger_app::{ClassificationRule, ClassificationRuleRepository};
    use ledger_domain::RuleId;
    let (app, _, mut store) = setup();
    let account = app
        .create_account(
            "Card",
            AccountKind::Liability,
            Money::new(0, CurrencyCode::new("CNY", 2).unwrap()),
            range().from(),
        )
        .unwrap();
    let category = app.create_category("Food").unwrap();
    app.repository()
        .insert_rule(ClassificationRule {
            id: RuleId::new(),
            priority: 1,
            merchant_contains: Some("cafe".into()),
            description_contains: None,
            category_id: category,
            enabled: true,
        })
        .unwrap();
    let body = include_str!("fixtures/providers/wechat.json");
    let mut transport = http(&[body]);
    let p = sync_preview(
        &wechat::WeChatAdapter,
        &mut transport,
        &mut store,
        app.repository(),
        account,
        range(),
    )
    .unwrap();
    assert_eq!(p.candidates[0].transaction.category_id, Some(category));
    assert_eq!(
        p.candidates[0].transaction.postings[0].delta.minor_units(),
        1250
    );
    let mut broken = p.clone();
    let mut refund = broken.candidates[0].clone();
    refund.transaction.id = ledger_domain::TransactionId::new();
    refunded(&mut refund);
    broken.candidates.push(refund);
    assert!(broken.confirm(&app).is_err());
    assert!(app.repository().list_transactions().unwrap().is_empty());
    assert_eq!(p.confirm(&app).unwrap().inserted, 1);
}
fn refunded(c: &mut ledger_import::ImportCandidate) {
    c.transaction.kind = ledger_domain::TransactionKind::Refund;
    c.transaction.postings.clear();
    c.transaction.original_transaction_id = None;
    c.evidence.external_transaction_id = Some("fixture-unmapped-refund".into());
}

#[test]
fn unauthorized_revoke_does_not_claim_platform_grant_was_removed() {
    let (_, _, mut store) = setup();
    let mut transport = http(&[]);
    transport.replies.push_back(Ok(HttpResponse {
        status: 403,
        retry_after_seconds: None,
        body: vec![],
    }));
    assert_eq!(
        revoke(&wechat::WeChatAdapter, &mut transport, &mut store).unwrap(),
        Revocation::LocalOnly
    );
    assert!(store.load(ProviderId::WeChat).unwrap().is_none());
}
#[test]
fn empty_stored_token_never_reaches_http_transport() {
    let (app, account, mut store) = setup();
    store
        .save(ProviderId::WeChat, &SecretToken::new(vec![]))
        .unwrap();
    let mut transport = http(&[]);
    // Using unavailable transport means an unexpected request returns a distinct error.
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut UnavailableTransport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::AuthorizationRequired
    );
    assert!(transport.calls.is_empty());
    // Retain a concrete no-call transport check too.
    assert_eq!(
        sync_preview(
            &wechat::WeChatAdapter,
            &mut transport,
            &mut store,
            app.repository(),
            account,
            range()
        )
        .unwrap_err(),
        ProviderError::AuthorizationRequired
    );
}
