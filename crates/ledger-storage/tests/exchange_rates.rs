use chrono::{DateTime, NaiveDate, Utc};
use ledger_app::{LedgerRepository, Timestamp};
use ledger_domain::{
    Account, AccountKind, CurrencyCode, Money, Posting, Transaction, TransactionId,
    TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::{
    ExchangeRateObservation, ExchangeRateSyncState, SqliteLedgerRepository,
};
use rusqlite::Connection;

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn utc(value: &str) -> Timestamp {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn observation(day: &str, quote: &str, rate: &str) -> ExchangeRateObservation {
    ExchangeRateObservation {
        effective_date: date(day),
        base_currency: "CNY".into(),
        quote_currency: quote.into(),
        rate: rate.into(),
        provider: "frankfurter".into(),
        provider_sources: vec!["ecb".into()],
        fetched_at_utc: utc("2026-10-30T17:00:00Z"),
    }
}

#[test]
fn observations_are_idempotent_exact_and_filtered_by_half_open_dates() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(repo.newest_exchange_rate_date().unwrap(), None);
    let friday = observation("2026-10-30", "USD", "7.12345678912345");
    let monday = observation("2026-11-02", "USD", "7.13");
    assert_eq!(
        repo.upsert_exchange_rates(&[friday.clone(), monday])
            .unwrap(),
        2
    );
    assert_eq!(repo.upsert_exchange_rates(&[friday]).unwrap(), 0);
    assert_eq!(
        repo.newest_exchange_rate_date().unwrap(),
        Some(date("2026-11-02"))
    );

    let rows = repo
        .exchange_rates("USD", date("2026-10-30"), date("2026-11-02"))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].rate, "7.12345678912345");
}

#[test]
fn a_later_provider_revision_corrects_the_latest_daily_observation() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let original = observation("2026-10-30", "USD", "7.12");
    repo.upsert_exchange_rates(&[original.clone()]).unwrap();
    let mut corrected = original;
    corrected.rate = "7.13".into();
    corrected.fetched_at_utc = utc("2026-10-30T18:00:00Z");

    assert_eq!(repo.upsert_exchange_rates(&[corrected]).unwrap(), 1);
    let rows = repo
        .exchange_rates("USD", date("2026-10-30"), date("2026-10-31"))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].rate, "7.13");
    assert_eq!(rows[0].fetched_at_utc, utc("2026-10-30T18:00:00Z"));
}

#[test]
fn invalid_currency_and_rate_values_are_rejected_before_writing() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert!(
        repo.upsert_exchange_rates(&[observation("2026-10-30", "XXX", "7.1")])
            .is_err()
    );

    for rate in ["0", "-1", "1e2", "NaN", "1.", ".5"] {
        assert!(
            repo.upsert_exchange_rates(&[observation("2026-10-30", "USD", rate)])
                .is_err(),
            "accepted invalid rate {rate}"
        );
    }
    assert!(
        repo.exchange_rates("XXX", date("2026-10-01"), date("2026-11-01"))
            .is_err()
    );
}

#[test]
fn sync_state_and_rate_history_survive_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exchange-rates.sqlite3");
    {
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        repo.upsert_exchange_rates(&[observation("2026-10-30", "USD", "7.12")])
            .unwrap();
        let state = ExchangeRateSyncState {
            last_attempt_utc: Some(utc("2026-10-30T17:00:00Z")),
            last_success_utc: Some(utc("2026-10-30T17:00:00Z")),
            status: "success".into(),
            error_code: None,
        };
        repo.record_exchange_rate_sync(&state).unwrap();
    }

    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(
        repo.exchange_rates("USD", date("2026-10-30"), date("2026-10-31"))
            .unwrap()[0]
            .rate,
        "7.12"
    );
    assert_eq!(
        repo.exchange_rate_sync_state().unwrap().unwrap().status,
        "success"
    );
}

#[test]
fn schema_twelve_upgrade_preserves_accounts_and_transactions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema-twelve.sqlite3");
    let account = Account::new(
        "旧账本账户",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    let transaction = Transaction {
        id: TransactionId::new(),
        occurred_at: utc("2026-10-29T12:00:00Z"),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Confirmed,
        postings: vec![Posting {
            account_id: account.id,
            delta: Money::new(-1250, account.currency.clone()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(1250, account.currency.clone())),
        fx_snapshot: None,
        merchant: Some("旧商户".into()),
        description: None,
    };

    {
        let repo = SqliteLedgerRepository::open(&path).unwrap();
        repo.insert_account(account.clone()).unwrap();
        repo.insert_transaction(transaction.clone()).unwrap();
    }
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE transaction_fx_snapshots;
             DROP TABLE exchange_rate_sync;
             DROP TABLE exchange_rates;
             DROP TABLE crypto_records; DROP TABLE crypto_watchlist; DELETE FROM schema_version WHERE version IN (13,14,15);",
        )
        .unwrap();
    drop(connection);

    let upgraded = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), 15);
    assert_eq!(upgraded.list_accounts().unwrap(), vec![account]);
    assert_eq!(
        upgraded.get_transaction(transaction.id).unwrap(),
        Some(transaction)
    );
}

#[test]
fn snapshot_restore_rejects_malformed_exchange_rate_text() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    repo.upsert_exchange_rates(&[observation("2026-10-30", "USD", "7.12")])
        .unwrap();
    let mut snapshot = serde_json::to_value(repo.snapshot().unwrap()).unwrap();
    let table = snapshot["tables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|table| table["name"] == "exchange_rates")
        .unwrap();
    let rate_column = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|column| column == "rate")
        .unwrap();
    table["rows"][0][rate_column] = serde_json::json!({ "Text": "NaN" });

    let snapshot = serde_json::from_value(snapshot).unwrap();
    assert!(SqliteLedgerRepository::from_snapshot(&snapshot).is_err());
}
