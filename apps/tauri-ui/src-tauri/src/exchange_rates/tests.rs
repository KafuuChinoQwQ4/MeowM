use super::provider::{HttpRateProvider, RateProvider, RateProviderError, parse_rates};
use super::service::sync;
use chrono::{DateTime, NaiveDate, Utc};
use ledger_storage::sqlite::{ExchangeRateObservation, SqliteLedgerRepository};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn observation() -> ExchangeRateObservation {
    parse_rates(
        br#"[{"date":"2026-10-29","base":"CNY","quote":"USD","rate":0.14012345678912345,"providers":[{"key":"ECB","date":"2026-10-29","rate":0.1402}]}]"#,
        now(),
    ).unwrap().pop().unwrap()
}

#[test]
fn decimal_json_numbers_and_real_provider_objects_remain_exact() {
    let row = observation();
    assert_eq!(row.rate, "0.14012345678912345");
    assert_eq!(row.provider_sources, vec!["ECB"]);
}

#[test]
fn parser_rejects_invalid_dates_currencies_rates_and_attribution() {
    for body in [
        r#"[{"date":"2026-02-30","base":"CNY","quote":"USD","rate":0.14}]"#,
        r#"[{"date":"2026-10-30","base":"EUR","quote":"USD","rate":0.14}]"#,
        r#"[{"date":"2026-10-30","base":"CNY","quote":"XXX","rate":0.14}]"#,
        r#"[{"date":"2026-10-30","base":"CNY","quote":"USD","rate":0}]"#,
        r#"[{"date":"2026-10-30","base":"CNY","quote":"USD","rate":-1}]"#,
        r#"[{"date":"2026-10-30","base":"CNY","quote":"USD","rate":"0.14"}]"#,
        r#"[{"date":"2026-10-30","base":"CNY","quote":"USD","rate":0.14,"providers":["ECB"]}]"#,
    ] {
        assert!(
            parse_rates(body.as_bytes(), now()).is_err(),
            "accepted {body}"
        );
    }
}

struct FixtureProvider {
    result: std::result::Result<Vec<ExchangeRateObservation>, RateProviderError>,
    requests: RefCell<Vec<(Option<NaiveDate>, NaiveDate)>>,
}
impl FixtureProvider {
    fn new(result: std::result::Result<Vec<ExchangeRateObservation>, RateProviderError>) -> Self {
        Self {
            result,
            requests: RefCell::new(Vec::new()),
        }
    }
}
impl RateProvider for FixtureProvider {
    fn fetch(
        &self,
        from: Option<NaiveDate>,
        through: NaiveDate,
    ) -> std::result::Result<Vec<ExchangeRateObservation>, RateProviderError> {
        self.requests.borrow_mut().push((from, through));
        self.result.clone()
    }
}

#[test]
fn first_sync_requests_365_days_and_daily_success_is_skipped() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let provider = FixtureProvider::new(Ok(vec![observation()]));
    let state = sync(&repo, &provider, now(), false).unwrap();
    assert_eq!(state.status, "success");
    assert_eq!(
        provider.requests.borrow().as_slice(),
        &[(Some(date("2025-10-31")), date("2026-10-30"))]
    );
    sync(&repo, &provider, now(), false).unwrap();
    assert_eq!(provider.requests.borrow().len(), 1);
    assert_eq!(
        repo.exchange_rates("USD", date("2026-10-01"), date("2026-11-01"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn failure_preserves_cache_and_allows_same_day_retry() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.upsert_exchange_rates(&[observation()]).unwrap();
    let provider = FixtureProvider::new(Err(RateProviderError::Timeout));
    let state = sync(&repo, &provider, now(), false).unwrap();
    assert_eq!(state.status, "timeout");
    assert_eq!(state.error_code.as_deref(), Some("timeout"));
    assert_eq!(
        repo.exchange_rates("USD", date("2026-10-01"), date("2026-11-01"))
            .unwrap()[0]
            .rate,
        "0.14012345678912345"
    );
    sync(&repo, &provider, now(), false).unwrap();
    assert_eq!(provider.requests.borrow().len(), 2);
}

#[test]
fn incremental_empty_response_does_not_invent_weekend_observations() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.upsert_exchange_rates(&[observation()]).unwrap();
    let provider = FixtureProvider::new(Ok(Vec::new()));
    let state = sync(&repo, &provider, now(), true).unwrap();
    assert_eq!(state.status, "no_new_data");
    assert_eq!(
        provider.requests.borrow().as_slice(),
        &[(Some(date("2026-10-29")), date("2026-10-30"))]
    );
    assert_eq!(
        repo.newest_exchange_rate_date().unwrap(),
        Some(date("2026-10-29"))
    );
}

#[test]
fn http_requests_fixed_quotes_and_merges_latest_attribution_without_expanding_history() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for body in [
            r#"[{"date":"2026-10-29","base":"CNY","quote":"USD","rate":0.14}]"#,
            r#"[{"date":"2026-10-29","base":"CNY","quote":"USD","rate":0.14,"providers":[{"key":"ECB","date":"2026-10-29","rate":0.14}]}]"#,
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    let provider = HttpRateProvider::new(ureq::Agent::new_with_defaults(), endpoint);
    let rows = provider
        .fetch(Some(date("2026-10-01")), date("2026-10-30"))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].provider_sources, vec!["ECB"]);
    let requests = server.join().unwrap();
    assert!(requests[0].contains(
        "base=CNY&quotes=USD,EUR,JPY,HKD,GBP,KRW,AUD,CAD,SGD,CHF&from=2026-10-01&to=2026-10-30"
    ));
    assert!(!requests[0].contains("expand"));
    assert!(requests[1].contains("date=2026-10-29&expand=providers"));
}

#[test]
fn history_command_validates_currency_and_interval_without_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = crate::ledger::Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    assert!(
        ledger
            .exchange_rates_history("XXX", "2026-10-01", "2026-11-01")
            .is_err()
    );
    assert!(
        ledger
            .exchange_rates_history("USD", "2026-11-01", "2026-10-01")
            .is_err()
    );
    assert!(
        ledger
            .exchange_rates_history("USD", "2026-10-01", "2026-11-01")
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "Uses the public Frankfurter API; run explicitly for integration verification"]
fn live_frankfurter_backfill_persists_real_reference_history() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let state = sync(&repo, &HttpRateProvider::default(), Utc::now(), true).unwrap();
    assert_eq!(
        state.status, "success",
        "live sync status: {:?}",
        state.error_code
    );
    assert!(repo.newest_exchange_rate_date().unwrap().is_some());
    let today = chrono::Local::now().date_naive();
    for quote in [
        "USD", "EUR", "JPY", "HKD", "GBP", "KRW", "AUD", "CAD", "SGD", "CHF",
    ] {
        assert!(
            repo.exchange_rates(
                quote,
                today - chrono::Duration::days(364),
                today.succ_opt().unwrap()
            )
            .unwrap()
            .len()
                > 100
        );
    }
}
