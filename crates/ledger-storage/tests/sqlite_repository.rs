use chrono::{DateTime, Utc};
use ledger_app::{
    ClassificationRule, ClassificationRuleRepository, LedgerRepository, RepositoryError, Timestamp,
    TransactionService,
};
use ledger_domain::{
    Account, AccountId, AccountKind, Category, CategoryId, CurrencyCode, Money, Posting, RuleId,
    SourceEvidence, SourceKind, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use rusqlite::Connection;
use uuid::Uuid;

fn utc(value: &str) -> Timestamp {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn account(currency: &str, minor_units: i64) -> Account {
    let code = CurrencyCode::new(currency, if currency == "JPY" { 0 } else { 2 }).unwrap();
    Account {
        id: AccountId::new(),
        display_name: "Primary wallet".into(),
        kind: AccountKind::Asset,
        currency: code.clone(),
        opening_balance: Some(Money::new(minor_units, code)),
        opening_balance_effective_at: Some(utc("2026-01-01T00:00:00.123456Z")),
    }
}

fn expense(account_id: AccountId, id: TransactionId, status: TransactionStatus) -> Transaction {
    let cny = CurrencyCode::new("CNY", 2).unwrap();
    Transaction {
        id,
        occurred_at: utc("2026-01-02T03:04:05.123456Z"),
        kind: TransactionKind::Expense,
        status,
        postings: vec![Posting {
            account_id,
            delta: Money::new(-1250, cny.clone()),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(1250, cny)),
        merchant: None,
        description: None,
    }
}

#[test]
fn fresh_database_migrates_once_and_round_trips_account_and_transaction() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(repo.schema_version().unwrap(), 3);
    assert_eq!(repo.schema_version().unwrap(), 3);

    let account = account("CNY", 20_000);
    let account_id = account.id;
    repo.insert_account(account.clone()).unwrap();
    assert_eq!(repo.get_account(account_id).unwrap(), Some(account));
    assert_eq!(repo.list_accounts().unwrap().len(), 1);

    let transaction = expense(
        account_id,
        TransactionId::new(),
        TransactionStatus::Confirmed,
    );
    repo.insert_transaction(transaction.clone()).unwrap();
    assert_eq!(
        repo.get_transaction(transaction.id).unwrap(),
        Some(transaction.clone())
    );
    assert_eq!(repo.list_transactions().unwrap(), vec![transaction]);
}

#[test]
fn opening_existing_file_keeps_schema_at_version_three() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    assert_eq!(
        SqliteLedgerRepository::open(&path)
            .unwrap()
            .schema_version()
            .unwrap(),
        3
    );
    assert_eq!(
        SqliteLedgerRepository::open(&path)
            .unwrap()
            .schema_version()
            .unwrap(),
        3
    );
}

#[test]
fn opening_database_with_empty_schema_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty-version.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY NOT NULL, applied_at_utc_micros INTEGER NOT NULL);",
        )
        .unwrap();
    drop(connection);

    assert!(SqliteLedgerRepository::open(&path).is_err());
}

#[test]
fn opening_database_with_newer_schema_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future-version.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY NOT NULL, applied_at_utc_micros INTEGER NOT NULL); INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (99, 0);",
        )
        .unwrap();
    drop(connection);

    assert!(SqliteLedgerRepository::open(&path).is_err());
}

#[test]
fn opening_database_with_invalid_schema_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid-version.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY NOT NULL, applied_at_utc_micros INTEGER NOT NULL); INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (0, 0);",
        )
        .unwrap();
    drop(connection);

    assert!(SqliteLedgerRepository::open(&path).is_err());
}

#[test]
fn opening_version_one_database_upgrades_to_version_three() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../migrations/0001_initial.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_version(version, applied_at_utc_micros) VALUES (1, 0)",
            [],
        )
        .unwrap();
    drop(connection);

    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.schema_version().unwrap(), 3);
    let connection = Connection::open(&path).unwrap();
    let has_rules: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'classification_rules'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(has_rules, 1);
}

#[test]
fn categories_and_rules_round_trip_and_disabled_categories_reject_rules() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let category = Category {
        id: CategoryId::new(),
        name: "Food".into(),
        enabled: true,
    };
    repo.insert_category(category.clone()).unwrap();
    assert_eq!(
        repo.get_category(category.id).unwrap(),
        Some(category.clone())
    );
    let rule = ClassificationRule {
        id: RuleId::new(),
        priority: 3,
        merchant_contains: Some("咖啡".into()),
        description_contains: Some("早餐".into()),
        category_id: category.id,
        enabled: true,
    };
    repo.insert_rule(rule.clone()).unwrap();
    assert_eq!(repo.list_rules().unwrap(), vec![rule.clone()]);
    let mut disabled = category;
    disabled.enabled = false;
    repo.update_category(disabled).unwrap();
    assert!(matches!(
        repo.insert_rule(rule),
        Err(RepositoryError::InvalidData(_))
    ));
}

#[test]
fn existing_rule_can_be_disabled_after_its_category_is_disabled() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let category = Category {
        id: CategoryId::new(),
        name: "Food".into(),
        enabled: true,
    };
    let other_category = Category {
        id: CategoryId::new(),
        name: "Transport".into(),
        enabled: false,
    };
    repo.insert_category(category.clone()).unwrap();
    repo.insert_category(other_category.clone()).unwrap();
    let mut rule = ClassificationRule {
        id: RuleId::new(),
        priority: 3,
        merchant_contains: Some("咖啡".into()),
        description_contains: None,
        category_id: category.id,
        enabled: true,
    };
    repo.insert_rule(rule.clone()).unwrap();

    let mut disabled_category = category.clone();
    disabled_category.enabled = false;
    repo.update_category(disabled_category).unwrap();

    assert!(matches!(
        repo.update_rule(rule.clone()),
        Err(RepositoryError::InvalidData(_))
    ));

    rule.enabled = false;
    rule.category_id = other_category.id;
    assert!(matches!(
        repo.update_rule(rule.clone()),
        Err(RepositoryError::InvalidData(_))
    ));

    rule.category_id = category.id;
    repo.update_rule(rule.clone()).unwrap();

    assert_eq!(repo.list_rules().unwrap(), vec![rule]);
}

#[test]
fn duplicate_source_external_id_is_rejected_without_duplicate_evidence() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = account("CNY", 0);
    repo.insert_account(account.clone()).unwrap();
    let first = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    let second = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    repo.insert_transaction(first.clone()).unwrap();
    repo.insert_transaction(second.clone()).unwrap();
    let batch = Uuid::new_v4();
    repo.create_import_batch(batch).unwrap();
    let evidence = SourceEvidence {
        source_kind: SourceKind::WeChat,
        external_transaction_id: Some("wx-transaction-42".into()),
        import_batch_id: batch,
        source_fields_digest: [7; 32],
    };
    repo.insert_source_evidence(first.id, evidence.clone())
        .unwrap();
    assert_eq!(
        repo.insert_source_evidence(second.id, evidence),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(repo.source_evidence_count().unwrap(), 1);
}

#[test]
fn evidence_conflict_rolls_back_transaction_postings_and_import_batch() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = account("CNY", 0);
    repo.insert_account(account.clone()).unwrap();
    let existing = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    let external_id = Some("wx-transaction-42".to_owned());
    repo.insert_transaction_with_evidence(
        existing,
        Some(SourceEvidence {
            source_kind: SourceKind::WeChat,
            external_transaction_id: external_id.clone(),
            import_batch_id: Uuid::new_v4(),
            source_fields_digest: [7; 32],
        }),
    )
    .unwrap();

    let candidate = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    let candidate_id = candidate.id;
    let candidate_batch = Uuid::new_v4();
    assert_eq!(
        repo.insert_transaction_with_evidence(
            candidate,
            Some(SourceEvidence {
                source_kind: SourceKind::WeChat,
                external_transaction_id: external_id,
                import_batch_id: candidate_batch,
                source_fields_digest: [8; 32],
            }),
        ),
        Err(RepositoryError::Conflict)
    );

    assert_eq!(repo.get_transaction(candidate_id).unwrap(), None);
    assert_eq!(repo.posting_count().unwrap(), 1);
    assert_eq!(repo.source_evidence_count().unwrap(), 1);
    assert_eq!(repo.import_batch_count().unwrap(), 1);
}

#[test]
fn distinct_transactions_reuse_the_same_import_batch() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = account("CNY", 0);
    repo.insert_account(account.clone()).unwrap();

    let first = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    let second = expense(account.id, TransactionId::new(), TransactionStatus::Pending);
    let batch = Uuid::new_v4();
    let first_evidence = SourceEvidence {
        source_kind: SourceKind::WeChat,
        external_transaction_id: Some("wx-batch-entry-1".into()),
        import_batch_id: batch,
        source_fields_digest: [1; 32],
    };
    let second_evidence = SourceEvidence {
        source_kind: SourceKind::WeChat,
        external_transaction_id: Some("wx-batch-entry-2".into()),
        import_batch_id: batch,
        source_fields_digest: [2; 32],
    };

    repo.insert_transaction_with_evidence(first.clone(), Some(first_evidence))
        .unwrap();
    repo.insert_transaction_with_evidence(second.clone(), Some(second_evidence))
        .unwrap();

    assert_eq!(repo.get_transaction(first.id).unwrap(), Some(first));
    assert_eq!(repo.get_transaction(second.id).unwrap(), Some(second));
    assert_eq!(repo.posting_count().unwrap(), 2);
    assert_eq!(repo.source_evidence_count().unwrap(), 2);
    assert_eq!(repo.import_batch_count().unwrap(), 1);
}

#[test]
fn invalid_posting_rolls_back_transaction_and_postings_atomically() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let candidate = expense(
        AccountId::new(),
        TransactionId::new(),
        TransactionStatus::Confirmed,
    );
    assert!(matches!(
        repo.insert_transaction(candidate.clone()),
        Err(RepositoryError::InvalidData(_))
    ));
    assert_eq!(repo.get_transaction(candidate.id).unwrap(), None);
    assert_eq!(repo.posting_count().unwrap(), 0);
}

#[test]
fn balances_skip_pending_then_apply_once_after_confirmation() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = account("CNY", 10_000);
    let account_id = account.id;
    repo.insert_account(account).unwrap();
    let pending = expense(account_id, TransactionId::new(), TransactionStatus::Pending);
    repo.insert_transaction(pending.clone()).unwrap();
    let service = TransactionService::new(repo);

    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        10_000
    );
    service.confirm(pending.clone()).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        8_750
    );
    service.confirm(pending).unwrap();
    assert_eq!(
        service
            .account_balance(account_id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        8_750
    );
}

#[test]
fn update_methods_reject_missing_identifiers() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    assert_eq!(
        repo.update_account(account("CNY", 0)),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        repo.update_transaction(expense(
            AccountId::new(),
            TransactionId::new(),
            TransactionStatus::Pending
        )),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn reference_balance_projects_backwards_through_confirmed_postings() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let mut wallet = account("CNY", 10_000);
    wallet.opening_balance_effective_at = Some(utc("2026-01-04T23:59:59.999999Z"));
    repo.insert_account(wallet.clone()).unwrap();
    let mut payment = expense(
        wallet.id,
        TransactionId::new(),
        TransactionStatus::Confirmed,
    );
    payment.occurred_at = utc("2026-01-04T12:00:00Z");
    repo.insert_transaction(payment).unwrap();
    assert_eq!(
        repo.balance_as_of(wallet.id, utc("2026-01-03T23:59:59.999999Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        11_250
    );
}

#[test]
fn unset_balance_survives_reopen_and_can_be_anchored_to_a_local_day() {
    use chrono::{FixedOffset, NaiveDate};
    use ledger_app::AccountService;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("optional.sqlite3");
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let wallet = Account::new(
        "WeChat",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    let id = wallet.id;
    AccountService::new(repo).create_account(wallet).unwrap();
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(
        repo.balance_as_of(id, utc("2026-01-05T15:59:59.999999Z"))
            .unwrap(),
        None
    );
    let mut payment = expense(id, TransactionId::new(), TransactionStatus::Confirmed);
    payment.occurred_at = utc("2026-01-04T10:00:00Z");
    repo.insert_transaction(payment).unwrap();
    let mut payment = expense(id, TransactionId::new(), TransactionStatus::Confirmed);
    payment.occurred_at = utc("2026-01-04T16:00:00Z"); // 5th, local midnight
    repo.insert_transaction(payment).unwrap();
    let accounts = AccountService::new(repo);
    accounts
        .set_day_closing_balance(
            id,
            Money::new(10_000, CurrencyCode::new("CNY", 2).unwrap()),
            NaiveDate::from_ymd_opt(2026, 1, 4).unwrap(),
            FixedOffset::east_opt(8 * 3600).unwrap(),
        )
        .unwrap();
    drop(accounts);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    for (day, expected) in [(3, 11_250), (4, 10_000), (5, 8_750)] {
        assert_eq!(
            repo.balance_as_of(id, utc(&format!("2026-01-{day:02}T15:59:59.999999Z")))
                .unwrap()
                .unwrap()
                .minor_units(),
            expected
        );
    }
    assert_eq!(repo.list_transactions().unwrap().len(), 2);
    let accounts = AccountService::new(repo);
    accounts.clear_opening_balance(id).unwrap();
    assert_eq!(
        accounts
            .repository()
            .balance_as_of(id, utc("2026-01-05T15:59:59.999999Z"))
            .unwrap(),
        None
    );
}

#[test]
fn version_two_upgrade_preserves_accounts_postings_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../migrations/0001_initial.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../migrations/0002_classification_rules.sql"))
        .unwrap();
    let wallet = account("CNY", 10_000);
    let id = wallet.id;
    let transaction_id = TransactionId::new();
    connection
        .execute_batch("INSERT INTO schema_version VALUES (1, 0), (2, 0);")
        .unwrap();
    connection
        .execute(
            "INSERT INTO accounts VALUES (?1, 'Legacy', 'asset', 'CNY', 2, 10000, 0)",
            [id.as_uuid().to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO transactions(id, occurred_at_utc_micros, kind, status) VALUES (?1, 1, 'expense', 'confirmed')", [transaction_id.as_uuid().to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO postings VALUES (?1, 0, ?2, -1250, 'CNY', 2)",
            rusqlite::params![
                transaction_id.as_uuid().to_string(),
                id.as_uuid().to_string()
            ],
        )
        .unwrap();
    drop(connection);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(
        repo.get_account(id)
            .unwrap()
            .unwrap()
            .opening_balance
            .unwrap()
            .minor_units(),
        10_000
    );
    assert_eq!(
        repo.balance_as_of(id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        8_750
    );
    assert_eq!(repo.posting_count().unwrap(), 1);
    assert!(
        repo.insert_transaction(expense(
            AccountId::new(),
            TransactionId::new(),
            TransactionStatus::Confirmed
        ))
        .is_err()
    );
}

#[test]
fn cash_flow_reports_work_without_a_balance_and_ignore_reference_changes() {
    use ledger_app::{AccountService, ReportQuery, ReportService};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reports.sqlite3");
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    let wallet = Account::new(
        "Alipay",
        AccountKind::Asset,
        CurrencyCode::new("CNY", 2).unwrap(),
    );
    let id = wallet.id;
    repo.insert_account(wallet).unwrap();
    repo.insert_transaction(expense(
        id,
        TransactionId::new(),
        TransactionStatus::Confirmed,
    ))
    .unwrap();
    let query = ReportQuery {
        from_utc_inclusive: utc("2026-01-01T00:00:00Z"),
        until_utc_exclusive: utc("2026-02-01T00:00:00Z"),
        month_boundaries_utc: vec![],
    };
    let accounts = AccountService::new(repo);
    accounts
        .set_opening_balance(
            id,
            Money::new(0, CurrencyCode::new("CNY", 2).unwrap()),
            utc("2026-01-03T00:00:00Z"),
        )
        .unwrap();
    assert_eq!(
        accounts
            .repository()
            .balance_as_of(id, utc("2026-01-03T00:00:00Z"))
            .unwrap()
            .unwrap()
            .minor_units(),
        0
    );
    accounts.clear_opening_balance(id).unwrap();
    assert_eq!(accounts.repository().list_transactions().unwrap().len(), 1);
    assert_eq!(
        accounts
            .repository()
            .balance_as_of(id, utc("2026-01-03T00:00:00Z"))
            .unwrap(),
        None
    );
    drop(accounts);
    let report_repo = SqliteLedgerRepository::open(&path).unwrap();
    let summary = ReportService::new(report_repo).summarize(query).unwrap();
    assert_eq!(
        (
            summary.income_cny_minor,
            summary.expense_cny_minor,
            summary.net_cny_minor
        ),
        (0, 1250, -1250)
    );
}
