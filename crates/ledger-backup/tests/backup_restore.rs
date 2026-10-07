use ledger_app::{LedgerRepository, MobileLedger, MobileSettings};
use ledger_backup::EncryptedBackup;
use ledger_domain::{AccountKind, CurrencyCode, Money};
use ledger_storage::sqlite::SqliteLedgerRepository;

fn app() -> MobileLedger<SqliteLedgerRepository> {
    let app = MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap());
    app.save_settings(MobileSettings {
        backup_enabled: true,
        ..Default::default()
    })
    .unwrap();
    app
}
#[test]
fn encrypted_roundtrip_replaces_ledger_and_rejects_wrong_password_without_changes() {
    let source = app();
    source
        .create_account(
            "private account",
            AccountKind::Asset,
            Money::new(1234, CurrencyCode::new("CNY", 2).unwrap()),
            chrono::Utc::now(),
        )
        .unwrap();
    let data = source
        .export_backup(&EncryptedBackup, "strong password", true)
        .unwrap();
    assert!(!data.windows(15).any(|w| w == b"private account"));
    let destination = app();
    assert!(
        destination
            .restore_backup(&EncryptedBackup, &data, "wrong", true)
            .is_err()
    );
    assert!(destination.repository().list_accounts().unwrap().is_empty());
    destination
        .restore_backup(&EncryptedBackup, &data, "strong password", true)
        .unwrap();
    let accounts = destination.repository().list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].opening_balance.minor_units(), 1234);
    destination
        .restore_backup(&EncryptedBackup, &data, "strong password", true)
        .unwrap();
    assert_eq!(destination.repository().list_accounts().unwrap().len(), 1);
}

use ledger_app::{
    ArchiveCell as Cell, BackupCodec, BackupRepository, ClassificationRule,
    ClassificationRuleRepository, ImportRecord, MobileRepository, PendingDetails, TransactionDraft,
};
use ledger_domain::{
    RuleId, SourceEvidence, SourceKind, Transaction, TransactionId, TransactionKind,
    TransactionStatus,
};
fn populated() -> MobileLedger<SqliteLedgerRepository> {
    let a = app();
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    let account = a
        .create_account(
            "wallet",
            AccountKind::Asset,
            Money::new(10000, currency.clone()),
            chrono::Utc::now() - chrono::Duration::days(1),
        )
        .unwrap();
    let category = a.create_category("food").unwrap();
    a.repository()
        .insert_rule(ClassificationRule {
            id: RuleId::new(),
            priority: 1,
            merchant_contains: Some("shop".into()),
            description_contains: None,
            category_id: category,
            enabled: true,
        })
        .unwrap();
    let mut settings = a.settings().unwrap();
    settings.default_account = Some(account);
    settings.import_wechat = true;
    a.save_settings(settings).unwrap();
    let expense = a
        .create_transaction(TransactionDraft {
            occurred_at: chrono::Utc::now(),
            kind: TransactionKind::Expense,
            account_id: account,
            destination_id: None,
            amount: Money::new(1200, currency.clone()),
            report_amount_cny: None,
            category_id: Some(category),
            original_transaction_id: None,
            merchant: Some("shop".into()),
            memo: Some("lunch".into()),
        })
        .unwrap();
    a.create_transaction(TransactionDraft {
        occurred_at: chrono::Utc::now(),
        kind: TransactionKind::Refund,
        account_id: account,
        destination_id: None,
        amount: Money::new(200, currency.clone()),
        report_amount_cny: None,
        category_id: Some(category),
        original_transaction_id: Some(expense),
        merchant: None,
        memo: None,
    })
    .unwrap();
    let pending = Transaction {
        id: TransactionId::new(),
        occurred_at: chrono::Utc::now(),
        kind: TransactionKind::Refund,
        status: TransactionStatus::Pending,
        postings: vec![],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: None,
        merchant: None,
        description: None,
    };
    a.repository()
        .capture_pending(
            ImportRecord {
                transaction: pending,
                evidence: SourceEvidence {
                    source_kind: SourceKind::Notification,
                    external_transaction_id: Some("external-1".into()),
                    import_batch_id: uuid::Uuid::new_v4(),
                    source_fields_digest: [7; 32],
                },
            },
            PendingDetails {
                amount: Some(Money::new(55, currency)),
                source: "wechat".into(),
                account_hint: Some("wallet".into()),
                original_external_id: Some("expense-1".into()),
            },
        )
        .unwrap();
    a
}
#[test]
fn full_archive_retains_rules_sources_settings_and_unresolved_refund_details() {
    let source = populated();
    let target = app();
    let before = source.repository().backup_snapshot().unwrap();
    let bytes = source
        .export_backup(&EncryptedBackup, "correct password", true)
        .unwrap();
    target
        .restore_backup(&EncryptedBackup, &bytes, "correct password", true)
        .unwrap();
    assert!(before == target.repository().backup_snapshot().unwrap());
    assert_eq!(target.repository().posting_count().unwrap(), 2);
    assert_eq!(target.repository().source_evidence_count().unwrap(), 1);
    assert_eq!(
        target
            .snapshot(chrono::Utc::now(), Default::default())
            .unwrap()
            .accounts[0]
            .balance
            .minor_units(),
        9000
    );
}
#[test]
fn corruption_and_unknown_versions_leave_existing_database_untouched() {
    let source = populated();
    let target = populated();
    let before = target.repository().backup_snapshot().unwrap();
    let bytes = source
        .export_backup(&EncryptedBackup, "correct password", true)
        .unwrap();
    for index in [0, 8, 9, 25, bytes.len() - 1] {
        let mut damaged = bytes.clone();
        damaged[index] ^= 1;
        assert!(
            target
                .restore_backup(&EncryptedBackup, &damaged, "correct password", true)
                .is_err()
        );
        assert!(before == target.repository().backup_snapshot().unwrap());
    }
    assert!(
        target
            .restore_backup(&EncryptedBackup, &bytes[..20], "correct password", true)
            .is_err()
    );
    let mut archive = source.repository().backup_snapshot().unwrap();
    archive.schema_version = 999;
    let unknown = EncryptedBackup
        .encrypt(&archive, "correct password")
        .unwrap();
    assert!(
        target
            .restore_backup(&EncryptedBackup, &unknown, "correct password", true)
            .is_err()
    );
    assert!(before == target.repository().backup_snapshot().unwrap());
}
#[test]
fn invalid_ledger_constraints_are_reexecuted_before_replacement() {
    let source = populated();
    let target = populated();
    let before = target.repository().backup_snapshot().unwrap();
    let original = source.repository().backup_snapshot().unwrap();
    let mut cases = Vec::new();
    let mut a = original.clone();
    a.tables.get_mut("postings").unwrap()[0][3] = Cell::Integer(1200);
    cases.push(a); // Expense direction.
    let mut a = original.clone();
    a.tables.get_mut("postings").unwrap().remove(0);
    cases.push(a); // Confirmed needs posting.
    let mut a = original.clone();
    a.tables.get_mut("transactions").unwrap()[1][6] = Cell::Null;
    cases.push(a); // Broken refund/report.
    let mut a = original.clone();
    let t = a.tables.get_mut("source_evidence").unwrap();
    let mut duplicate = t[0].clone();
    duplicate[0] = Cell::Integer(42);
    t.push(duplicate);
    cases.push(a);
    let mut a = original.clone();
    a.tables.get_mut("source_evidence").unwrap()[0][4] =
        Cell::Text(uuid::Uuid::new_v4().to_string());
    cases.push(a);
    let mut a = original.clone();
    a.tables.get_mut("pending_details").unwrap()[0][2] = Cell::Null;
    cases.push(a);
    let mut a = original.clone();
    a.tables.insert("tokens".into(), vec![]);
    cases.push(a);
    let mut a = original.clone();
    a.tables.get_mut("mobile_settings").unwrap()[0][1] = Cell::Integer(8);
    cases.push(a);
    for a in cases {
        assert!(target.repository().restore_snapshot(a).is_err());
        assert!(before == target.repository().backup_snapshot().unwrap());
    }
}
#[test]
fn schema_three_upgrades_and_empty_restore_works_without_reviving_capture_consent() {
    let source = populated();
    let target = app();
    let mut archive = source.repository().backup_snapshot().unwrap();
    archive.schema_version = 3;
    archive.tables.remove("pending_details");
    archive.tables.remove("mobile_settings");
    target.repository().restore_snapshot(archive).unwrap();
    assert_eq!(target.repository().schema_version().unwrap(), 4);
    assert_eq!(target.repository().list_accounts().unwrap().len(), 1);
    assert!(!target.settings().unwrap().notification_wechat);
    assert!(!target.settings().unwrap().backup_enabled);
    target
        .repository()
        .restore_snapshot(app().repository().backup_snapshot().unwrap())
        .unwrap();
    assert!(target.repository().list_transactions().unwrap().is_empty());
}
#[test]
fn same_open_connections_read_and_write_restored_database_and_failed_writes_roll_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let target = MobileLedger::new(SqliteLedgerRepository::open(&path).unwrap());
    let second = SqliteLedgerRepository::open(&path).unwrap();
    let source = populated();
    target
        .repository()
        .restore_snapshot(source.repository().backup_snapshot().unwrap())
        .unwrap();
    assert_eq!(second.list_accounts().unwrap().len(), 1);
    let second = MobileLedger::new(second);
    second.create_category("after restore").unwrap();
    assert_eq!(target.repository().list_categories().unwrap().len(), 2);
    let mut flags = target.settings().unwrap();
    flags.notification_alipay = true;
    target.save_settings(flags).unwrap();
    target
        .repository()
        .restore_snapshot(source.repository().backup_snapshot().unwrap())
        .unwrap();
    assert!(target.settings().unwrap().notification_alipay);
    assert!(!target.settings().unwrap().notification_wechat);
}
#[test]
fn consent_password_and_randomness_are_enforced() {
    let a = app();
    assert!(
        a.export_backup(&EncryptedBackup, "correct password", false)
            .is_err()
    );
    assert!(a.export_backup(&EncryptedBackup, "short", true).is_err());
    let one = a
        .export_backup(&EncryptedBackup, "correct password", true)
        .unwrap();
    let two = a
        .export_backup(&EncryptedBackup, "correct password", true)
        .unwrap();
    assert_ne!(one, two);
    assert!(
        a.restore_backup(&EncryptedBackup, &one, "correct password", false)
            .is_err()
    );
    a.save_settings(MobileSettings::default()).unwrap();
    assert!(
        a.export_backup(&EncryptedBackup, "correct password", true)
            .is_err()
    );
    assert!(
        a.restore_backup(&EncryptedBackup, &one, "correct password", true)
            .is_err()
    );
}
#[test]
fn large_ledger_roundtrip_is_complete() {
    let source = populated();
    let template = source
        .repository()
        .list_transactions()
        .unwrap()
        .into_iter()
        .find(|t| t.kind == TransactionKind::Expense)
        .unwrap();
    for i in 0..5000 {
        source.create_category(&format!("category {i}")).unwrap();
        let mut transaction = template.clone();
        transaction.id = TransactionId::new();
        source.repository().insert_transaction(transaction).unwrap();
    }
    let bytes = source
        .export_backup(&EncryptedBackup, "correct password", true)
        .unwrap();
    let target = app();
    target
        .restore_backup(&EncryptedBackup, &bytes, "correct password", true)
        .unwrap();
    assert_eq!(target.repository().list_categories().unwrap().len(), 5001);
    assert_eq!(target.repository().list_transactions().unwrap().len(), 5003);
}
#[test]
fn replacement_write_failure_rolls_back_all_deletes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let destination = SqliteLedgerRepository::open(&path).unwrap();
    destination
        .restore_snapshot(populated().repository().backup_snapshot().unwrap())
        .unwrap();
    let before = destination.backup_snapshot().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_restore BEFORE INSERT ON accounts BEGIN SELECT RAISE(ABORT,'write failed'); END").unwrap();
    assert!(
        destination
            .restore_snapshot(populated().repository().backup_snapshot().unwrap())
            .is_err()
    );
    assert!(before == destination.backup_snapshot().unwrap());
}
#[test]
fn malformed_typed_cells_cannot_use_sqlite_coercion() {
    let source = populated();
    let target = app();
    let mut archive = source.repository().backup_snapshot().unwrap();
    archive.tables.get_mut("accounts").unwrap()[0][5] = Cell::Text("10000".into());
    assert!(target.repository().restore_snapshot(archive).is_err());
}
#[test]
fn snapshots_exclude_future_credential_tables_and_deleted_database_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let repository = SqliteLedgerRepository::open(&path).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE future_credentials(token TEXT); INSERT INTO future_credentials VALUES ('never-export-this-token'); CREATE TABLE old_notifications(body TEXT); INSERT INTO old_notifications VALUES('never-export-this-body'); DELETE FROM old_notifications;").unwrap();
    let archive = repository.backup_snapshot().unwrap();
    assert_eq!(archive.tables.len(), 9);
    let serialized = serde_json::to_string(&archive).unwrap();
    assert!(!serialized.contains("never-export"));
    assert!(!serialized.contains("credentials"));
    assert!(!serialized.contains("notifications"));
}

#[test]
fn every_pending_detail_row_requires_a_valid_nonnull_transaction_reference() {
    let target = populated();
    let before = target.repository().backup_snapshot().unwrap();
    let mut archive = app().repository().backup_snapshot().unwrap();
    archive
        .tables
        .get_mut("pending_details")
        .unwrap()
        .push(vec![
            Cell::Null,
            Cell::Integer(-100),
            Cell::Blob(vec![1]),
            Cell::Null,
            Cell::Text("invalid-source".into()),
            Cell::Null,
            Cell::Null,
        ]);
    assert!(target.repository().restore_snapshot(archive).is_err());
    assert!(before == target.repository().backup_snapshot().unwrap());
}

#[test]
fn malformed_pending_payloads_and_all_null_primary_keys_leave_destination_unchanged() {
    let source = populated().repository().backup_snapshot().unwrap();
    let target = populated();
    let before = target.repository().backup_snapshot().unwrap();
    for (column, value) in [
        (0, Cell::Null),
        (0, Cell::Text("not-a-uuid".into())),
        (0, Cell::Text(uuid::Uuid::new_v4().to_string())),
        (1, Cell::Integer(-100)),
        (2, Cell::Blob(vec![1])),
        (2, Cell::Null),
        (3, Cell::Integer(100)),
        (4, Cell::Text("invalid-source".into())),
        (5, Cell::Blob(vec![1])),
        (6, Cell::Integer(1)),
    ] {
        let mut archive = source.clone();
        archive.tables.get_mut("pending_details").unwrap()[0][column] = value;
        assert!(target.repository().restore_snapshot(archive).is_err());
        assert!(before == target.repository().backup_snapshot().unwrap());
    }
    for table in [
        "accounts",
        "categories",
        "transactions",
        "postings",
        "import_batches",
        "source_evidence",
        "classification_rules",
        "mobile_settings",
        "pending_details",
    ] {
        let mut archive = source.clone();
        archive.tables.get_mut(table).unwrap()[0][0] = Cell::Null;
        assert!(target.repository().restore_snapshot(archive).is_err());
        assert!(before == target.repository().backup_snapshot().unwrap());
    }
}
