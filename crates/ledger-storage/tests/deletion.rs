use ledger_app::{
    DeletionScope, ImportRecord, ImportRepository, LedgerRepository, MobileLedger, MobileSettings,
    TransactionDraft,
};
use ledger_domain::*;
use ledger_storage::sqlite::SqliteLedgerRepository;
fn fixture() -> (MobileLedger<SqliteLedgerRepository>, AccountId) {
    let app = MobileLedger::new(SqliteLedgerRepository::in_memory().unwrap());
    let id = app
        .create_account(
            "Test",
            AccountKind::Asset,
            Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
            "2026-01-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    (app, id)
}
fn draft(
    account: AccountId,
    kind: TransactionKind,
    parent: Option<TransactionId>,
) -> TransactionDraft {
    TransactionDraft {
        occurred_at: "2026-10-07T00:00:00Z".parse().unwrap(),
        kind,
        account_id: account,
        destination_id: None,
        amount: Money::new(100, CurrencyCode::new("CNY", 2).unwrap()),
        report_amount_cny: None,
        category_id: None,
        original_transaction_id: parent,
        merchant: None,
        memo: None,
    }
}
#[test]
fn deletion_requires_confirmation_and_refund_parent_blocks_atomic_delete() {
    let (app, id) = fixture();
    let expense = app
        .create_transaction(draft(id, TransactionKind::Expense, None))
        .unwrap();
    let refund = app
        .create_transaction(draft(id, TransactionKind::Refund, Some(expense)))
        .unwrap();
    assert!(
        app.delete_data(DeletionScope::Transaction(refund), false)
            .is_err()
    );
    assert!(
        app.delete_data(DeletionScope::Transaction(expense), true)
            .is_err()
    );
    assert_eq!(app.repository().list_transactions().unwrap().len(), 2);
    app.delete_data(DeletionScope::Transaction(refund), true)
        .unwrap();
    app.delete_data(DeletionScope::Transaction(expense), true)
        .unwrap();
    assert!(app.repository().list_transactions().unwrap().is_empty());
}
#[test]
fn batch_delete_preserves_other_evidence_and_all_delete_resets_every_setting() {
    let (app, id) = fixture();
    let tid = app
        .create_transaction(draft(id, TransactionKind::Expense, None))
        .unwrap();
    let t = app.repository().get_transaction(tid).unwrap().unwrap();
    let batch = uuid::Uuid::new_v4();
    app.confirm_import_batch(
        batch,
        vec![ImportRecord {
            transaction: t,
            evidence: SourceEvidence {
                source_kind: SourceKind::WeChat,
                external_transaction_id: Some("synthetic-1".into()),
                import_batch_id: batch,
                source_fields_digest: [0; 32],
            },
        }],
    )
    .unwrap();
    app.delete_data(DeletionScope::ImportBatch(batch), true)
        .unwrap();
    assert!(app.repository().list_transactions().unwrap().is_empty());
    app.save_settings(MobileSettings {
        notification_wechat: true,
        api_enabled: true,
        api_wechat: true,
        api_alipay: true,
        api_bank: true,
        backup_enabled: true,
        default_account: Some(id),
        ..Default::default()
    })
    .unwrap();
    app.delete_data(DeletionScope::AllLocal, true).unwrap();
    assert_eq!(app.settings().unwrap(), MobileSettings::default());
    assert!(app.repository().list_accounts().unwrap().is_empty());
    assert!(app.list_import_records().unwrap().is_empty());
}
#[test]
fn failed_batch_delete_rolls_back_evidence_and_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let app = MobileLedger::new(SqliteLedgerRepository::open(&path).unwrap());
    let id = app
        .create_account(
            "Test",
            AccountKind::Asset,
            Money::new(10000, CurrencyCode::new("CNY", 2).unwrap()),
            "2026-01-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    let tid = app
        .create_transaction(draft(id, TransactionKind::Expense, None))
        .unwrap();
    let t = app.repository().get_transaction(tid).unwrap().unwrap();
    let batch = uuid::Uuid::new_v4();
    app.confirm_import_batch(
        batch,
        vec![ImportRecord {
            transaction: t,
            evidence: SourceEvidence {
                source_kind: SourceKind::WeChat,
                external_transaction_id: Some("synthetic-2".into()),
                import_batch_id: batch,
                source_fields_digest: [0; 32],
            },
        }],
    )
    .unwrap();
    rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON transactions BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        app.delete_data(DeletionScope::ImportBatch(batch), true)
            .is_err()
    );
    assert_eq!(app.list_import_records().unwrap().len(), 1);
    assert_eq!(app.repository().list_transactions().unwrap().len(), 1);
    assert!(app.delete_data(DeletionScope::AllLocal, true).is_err());
    assert_eq!(app.repository().list_accounts().unwrap().len(), 1);
}

#[test]
fn all_delete_includes_refunds_rules_and_every_archive_table() {
    use ledger_app::{BackupRepository, ClassificationRule, ClassificationRuleRepository};
    let (app, id) = fixture();
    let category = app.create_category("Synthetic category").unwrap();
    app.repository()
        .insert_rule(ClassificationRule {
            id: RuleId::new(),
            priority: 1,
            merchant_contains: Some("Synthetic".into()),
            description_contains: None,
            category_id: category,
            enabled: true,
        })
        .unwrap();
    let parent = app
        .create_transaction(draft(id, TransactionKind::Expense, None))
        .unwrap();
    app.create_transaction(draft(id, TransactionKind::Refund, Some(parent)))
        .unwrap();
    app.delete_data(DeletionScope::AllLocal, true).unwrap();
    assert!(app.repository().list_transactions().unwrap().is_empty());
    assert!(app.repository().list_rules().unwrap().is_empty());
    assert!(app.repository().list_categories().unwrap().is_empty());
    assert!(app.repository().list_accounts().unwrap().is_empty());
    assert!(app.repository().backup_snapshot().is_ok());
}

#[test]
fn deleting_last_record_removes_batch_replay_marker() {
    let (app, id) = fixture();
    let tid = app
        .create_transaction(draft(id, TransactionKind::Expense, None))
        .unwrap();
    let t = app.repository().get_transaction(tid).unwrap().unwrap();
    let batch = uuid::Uuid::new_v4();
    let record = ImportRecord {
        transaction: t,
        evidence: SourceEvidence {
            source_kind: SourceKind::WeChat,
            external_transaction_id: Some("delete-reimport".into()),
            import_batch_id: batch,
            source_fields_digest: [9; 32],
        },
    };
    app.confirm_import_batch(batch, vec![record.clone()])
        .unwrap();
    app.delete_data(DeletionScope::Transaction(tid), true)
        .unwrap();
    assert_eq!(app.repository().import_batch_count().unwrap(), 0);
    assert_eq!(app.confirm_import_batch(batch, vec![record]).unwrap(), 1);
}
