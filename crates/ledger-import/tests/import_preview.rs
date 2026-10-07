use chrono::{DateTime, Utc};
use ledger_app::{ImportRecord, ImportRepository, LedgerRepository};
use ledger_domain::{AccountId, CurrencyCode, Money, TransactionKind, TransactionStatus};
use ledger_import::{
    ImportCandidate, ImportPreview, ImportSource, NormalizedImportRow, RawImportRow,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
use uuid::Uuid;

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn row(
    external: Option<&str>,
    merchant: &str,
    account_id: AccountId,
    minor: i64,
    at: &str,
) -> ImportCandidate {
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    let normalized = NormalizedImportRow {
        raw: RawImportRow {
            row_number: 2,
            time_text: at.into(),
            amount_text: "12.50".into(),
            currency_text: "CNY".into(),
            direction_text: "支出".into(),
            merchant: merchant.into(),
            external_transaction_id: external.map(str::to_owned),
            original_external_transaction_id: None,
            description: None,
            account_hint: None,
            source_fields_digest: [7; 32],
        },
        occurred_at: utc(at),
        amount: Money::new(minor, currency.clone()),
        report_amount_cny: Some(Money::new(minor, currency)),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Pending,
        merchant: Some(merchant.into()),
        account_hint: None,
        warnings: vec![],
    };
    ImportCandidate::from_normalized(
        ImportSource::WeChat,
        normalized,
        account_id,
        Uuid::from_u128(10),
    )
}

#[test]
fn exact_external_id_is_scoped_to_source_and_fuzzy_match_requires_the_full_tuple() {
    let account_id = AccountId::new();
    let old = row(
        Some("same-id"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let mut cross_source = row(
        Some("same-id"),
        "Other",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    cross_source.source = ImportSource::Alipay;
    cross_source.evidence.source_kind = ledger_domain::SourceKind::Alipay;
    let exact = row(
        Some("same-id"),
        "Different",
        account_id,
        999,
        "2026-10-09T12:00:00Z",
    );
    let fuzzy = row(None, "Cafe", account_id, 1250, "2026-10-01T12:02:00Z");
    let amount_date_only = row(None, "Distinct", account_id, 1250, "2026-10-01T12:02:00Z");
    let existing = ImportRecord {
        transaction: old.transaction,
        evidence: old.evidence,
    };
    let preview = ImportPreview::build(
        vec![cross_source, exact, fuzzy, amount_date_only],
        vec![existing],
    )
    .unwrap();
    assert_eq!(
        (
            preview.counts.new,
            preview.counts.duplicate,
            preview.counts.suspected
        ),
        (2, 1, 1)
    );
}

#[test]
fn bank_csv_can_be_a_cross_source_strong_candidate_without_automatic_merge() {
    let account_id = AccountId::new();
    let old = row(
        Some("wx-id"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let mut bank = row(None, "Cafe", account_id, 1250, "2026-10-01T12:02:00Z");
    bank.source = ImportSource::BankCsv;
    bank.evidence.source_kind = ledger_domain::SourceKind::BankCsv;
    let existing = ImportRecord {
        transaction: old.transaction,
        evidence: old.evidence,
    };
    let preview = ImportPreview::build(vec![bank], vec![existing]).unwrap();
    assert_eq!(preview.counts.suspected, 1);
    assert_eq!(preview.counts.new, 0);
    assert_eq!(
        preview.candidates[0].match_status,
        ledger_import::DuplicateMatch::StrongCandidate
    );
}

#[test]
fn duplicate_ids_within_one_file_are_counted_and_written_once() {
    let account_id = AccountId::new();
    let first = row(
        Some("repeated"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let second = row(
        Some("repeated"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let preview = ImportPreview::build(vec![first, second], vec![]).unwrap();
    assert_eq!((preview.counts.new, preview.counts.duplicate), (1, 1));
}

#[test]
fn refunds_need_an_explicit_parent_mapping_before_postings_are_created() {
    let account_id = AccountId::new();
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    let parsed = NormalizedImportRow {
        raw: RawImportRow {
            row_number: 2,
            time_text: "2026-10-01T12:00:00Z".into(),
            amount_text: "5.00".into(),
            currency_text: "CNY".into(),
            direction_text: "支出".into(),
            merchant: "Cafe".into(),
            external_transaction_id: None,
            original_external_transaction_id: Some("parent-external".into()),
            description: None,
            account_hint: None,
            source_fields_digest: [8; 32],
        },
        occurred_at: utc("2026-10-01T12:00:00Z"),
        amount: Money::new(500, currency.clone()),
        report_amount_cny: Some(Money::new(500, currency)),
        kind: TransactionKind::Refund,
        status: TransactionStatus::Pending,
        merchant: Some("Cafe".into()),
        account_hint: None,
        warnings: vec![],
    };
    let unlinked = ImportCandidate::from_normalized(
        ImportSource::WeChat,
        parsed.clone(),
        account_id,
        Uuid::from_u128(12),
    );
    assert!(unlinked.transaction.postings.is_empty());
    assert!(unlinked.transaction.original_transaction_id.is_none());
    assert_eq!(unlinked.warnings.len(), 1);
    let parent = ledger_domain::TransactionId::new();
    let linked = ImportCandidate::from_normalized_with_refund_parent(
        ImportSource::WeChat,
        parsed,
        account_id,
        Uuid::from_u128(13),
        Some(parent),
    );
    assert_eq!(linked.transaction.original_transaction_id, Some(parent));
    assert_eq!(linked.transaction.postings.len(), 1);
    assert!(linked.warnings.is_empty());
}

#[test]
fn confirming_batch_activates_balance_and_report_once_and_replay_is_idempotent() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account_id = AccountId::new();
    let account = ledger_domain::Account {
        id: account_id,
        display_name: "Wallet".into(),
        kind: ledger_domain::AccountKind::Asset,
        currency: CurrencyCode::new("CNY", 2).unwrap(),
        opening_balance: Money::new(0, CurrencyCode::new("CNY", 2).unwrap()),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
    };
    repo.insert_account(account).unwrap();
    let candidate = row(
        Some("txn-1"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let rows = vec![candidate.clone()];
    let batch = candidate.evidence.import_batch_id;
    let preview = ImportPreview::build(rows, repo.list_import_records().unwrap()).unwrap();
    assert_eq!(preview.counts.pending, 1);
    assert_eq!(
        preview.candidates[0].transaction.status,
        TransactionStatus::Pending
    );
    let receipt = preview.confirm(&repo).unwrap();
    assert_eq!(receipt.inserted, 1);
    let saved = repo
        .get_transaction(candidate.transaction.id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.status, TransactionStatus::Confirmed);
    assert_eq!(repo.import_batch_count().unwrap(), 1);
    assert_eq!(repo.source_evidence_count().unwrap(), 1);
    assert_eq!(
        repo.balance_as_of(account_id, utc("2026-12-01T00:00:00Z"))
            .unwrap()
            .minor_units(),
        -1250
    );
    assert!(ledger_domain::report_effect(&saved).is_some());

    let replay =
        ImportPreview::build(vec![candidate], repo.list_import_records().unwrap()).unwrap();
    let replay_receipt = replay.confirm(&repo).unwrap();
    assert_eq!(replay_receipt.inserted, 0);
    assert_eq!(repo.import_batch_count().unwrap(), 1);
    assert_eq!(repo.list_transactions().unwrap().len(), 1);
    assert_eq!(
        repo.balance_as_of(account_id, utc("2026-12-01T00:00:00Z"))
            .unwrap()
            .minor_units(),
        -1250
    );
    assert_eq!(batch, replay_receipt.batch_id);
}

#[test]
fn bank_csv_evidence_round_trips_as_its_own_source_kind() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account_id = AccountId::new();
    repo.insert_account(ledger_domain::Account {
        id: account_id,
        display_name: "Wallet".into(),
        kind: ledger_domain::AccountKind::Asset,
        currency: CurrencyCode::new("CNY", 2).unwrap(),
        opening_balance: Money::new(0, CurrencyCode::new("CNY", 2).unwrap()),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
    })
    .unwrap();
    let mut candidate = row(
        Some("bank-id"),
        "Cafe",
        account_id,
        1250,
        "2026-10-01T12:00:00Z",
    );
    candidate.source = ImportSource::BankCsv;
    candidate.evidence.source_kind = ledger_domain::SourceKind::BankCsv;
    ImportPreview::build(vec![candidate], vec![])
        .unwrap()
        .confirm(&repo)
        .unwrap();
    assert_eq!(
        repo.list_import_records().unwrap()[0].evidence.source_kind,
        ledger_domain::SourceKind::BankCsv
    );
    assert_eq!(repo.schema_version().unwrap(), 4);
}

#[test]
fn failed_confirm_rolls_back_all_rows_postings_evidence_and_batch() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let valid_account = AccountId::new();
    repo.insert_account(ledger_domain::Account {
        id: valid_account,
        display_name: "Wallet".into(),
        kind: ledger_domain::AccountKind::Asset,
        currency: CurrencyCode::new("CNY", 2).unwrap(),
        opening_balance: Money::new(0, CurrencyCode::new("CNY", 2).unwrap()),
        opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
    })
    .unwrap();
    let valid = row(
        Some("good"),
        "Cafe",
        valid_account,
        1250,
        "2026-10-01T12:00:00Z",
    );
    let invalid = row(
        Some("bad"),
        "Shop",
        AccountId::new(),
        250,
        "2026-10-02T12:00:00Z",
    );
    let error = ImportPreview::build(vec![valid, invalid], repo.list_import_records().unwrap())
        .unwrap()
        .confirm(&repo);
    assert!(error.is_err());
    assert!(repo.list_transactions().unwrap().is_empty());
    assert_eq!(repo.posting_count().unwrap(), 0);
    assert_eq!(repo.source_evidence_count().unwrap(), 0);
    assert_eq!(repo.import_batch_count().unwrap(), 0);
}

#[test]
fn csv_without_transaction_ids_confirms_two_distinct_same_second_payments() {
    for parse in [
        ledger_import::parse_wechat_csv,
        ledger_import::parse_alipay_csv,
    ] {
        let repository = SqliteLedgerRepository::in_memory().unwrap();
        let account_id = AccountId::new();
        let currency = CurrencyCode::new("CNY", 2).unwrap();
        repository
            .insert_account(ledger_domain::Account {
                id: account_id,
                display_name: "Wallet".into(),
                kind: ledger_domain::AccountKind::Asset,
                currency: currency.clone(),
                opening_balance: Money::new(0, currency),
                opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
            })
            .unwrap();
        let parsed = parse("交易时间,交易对方,金额,收/支\n2026-10-07 10:30:00,商店甲,1.00,支出\n2026-10-07 10:30:00,商店乙,2.00,支出\n".as_bytes()).unwrap();
        let batch = ImportCandidate::batch_id(parsed.source, &parsed.raw_rows);
        let candidates = parsed
            .rows
            .into_iter()
            .map(|row| ImportCandidate::from_normalized(parsed.source, row, account_id, batch))
            .collect();
        let preview = ImportPreview::build(candidates, vec![]).unwrap();
        assert_eq!(preview.counts.duplicate, 0);
        assert_eq!(preview.counts.new, 2);
        assert_eq!(preview.confirm(&repository).unwrap().inserted, 2);
        assert_eq!(repository.list_transactions().unwrap().len(), 2);
        assert_eq!(
            repository
                .balance_as_of(account_id, utc("2026-12-01T00:00:00Z"))
                .unwrap()
                .minor_units(),
            -300
        );
    }
}

#[test]
fn csv_repayment_requires_two_accounts_and_cannot_be_confirmed_as_expense() {
    for parse in [
        ledger_import::parse_wechat_csv,
        ledger_import::parse_alipay_csv,
    ] {
        let repository = SqliteLedgerRepository::in_memory().unwrap();
        let account_id = AccountId::new();
        let currency = CurrencyCode::new("CNY", 2).unwrap();
        repository
            .insert_account(ledger_domain::Account {
                id: account_id,
                display_name: "Wallet".into(),
                kind: ledger_domain::AccountKind::Asset,
                currency: currency.clone(),
                opening_balance: Money::new(0, currency),
                opening_balance_effective_at: utc("2026-01-01T00:00:00Z"),
            })
            .unwrap();
        let parsed = parse("交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号\n2026-10-07 10:30:00,信用卡还款,本人信用卡,还款,支出,100.00,零钱,支付成功,repay-1\n".as_bytes()).unwrap();
        let batch = ImportCandidate::batch_id(parsed.source, &parsed.raw_rows);
        let candidate = ImportCandidate::from_normalized(
            parsed.source,
            parsed.rows[0].clone(),
            account_id,
            batch,
        );
        assert_eq!(candidate.transaction.kind, TransactionKind::Transfer);
        assert!(candidate.transaction.postings.is_empty());
        let mut expense = row(
            Some("normal-expense"),
            "商店",
            account_id,
            200,
            "2026-10-07T02:31:00Z",
        );
        expense.evidence.import_batch_id = batch;
        assert!(
            ImportPreview::build(vec![expense.clone(), candidate], vec![])
                .unwrap()
                .confirm(&repository)
                .is_err()
        );
        assert!(repository.list_transactions().unwrap().is_empty());
        assert_eq!(repository.import_batch_count().unwrap(), 0);
        // Excluding the unmapped transfer leaves the ordinary expense importable.
        assert_eq!(
            ImportPreview::build(vec![expense], vec![])
                .unwrap()
                .confirm(&repository)
                .unwrap()
                .inserted,
            1
        );
        assert_eq!(
            repository
                .balance_as_of(account_id, utc("2026-12-01T00:00:00Z"))
                .unwrap()
                .minor_units(),
            -200
        );
    }
}
