use chrono::{DateTime, Utc};
use ledger_domain::{CurrencyCode, SourceKind, TransactionKind};
use ledger_import::{ExistingImport, ImportPreview, NormalizedImportRow, PreviewDecision};

fn row(id: Option<&str>, at: &str, amount: i64) -> NormalizedImportRow {
    let currency = CurrencyCode::new("CNY", 2).unwrap();
    let occurred_at = DateTime::parse_from_rfc3339(at)
        .unwrap()
        .with_timezone(&Utc);
    let kind = TransactionKind::Expense;
    NormalizedImportRow {
        row_number: 2,
        source_kind: SourceKind::WeChat,
        external_transaction_id: id.map(str::to_owned),
        occurred_at,
        kind,
        amount_minor: amount,
        currency: currency.clone(),
        merchant: Some("shop".into()),
        description: None,
        source_fields_digest: NormalizedImportRow::digest_for(
            SourceKind::WeChat,
            id,
            occurred_at,
            kind,
            amount,
            &currency,
            Some("shop"),
            None,
        ),
        warnings: vec![],
    }
}

#[test]
fn preview_separates_new_exact_and_suspected_duplicates_without_merging() {
    let rows = vec![
        row(Some("stable"), "2026-10-01T00:00:00Z", 100),
        row(None, "2026-10-02T00:00:00Z", 200),
        row(Some("new"), "2026-10-03T00:00:00Z", 300),
    ];
    let existing = vec![
        ExistingImport {
            transaction_id: None,
            source_kind: SourceKind::WeChat,
            external_transaction_id: Some("stable".into()),
            occurred_at: rows[0].occurred_at,
            kind: rows[0].kind,
            amount_minor: 100,
            merchant: Some("shop".into()),
        },
        ExistingImport {
            transaction_id: None,
            source_kind: SourceKind::WeChat,
            external_transaction_id: None,
            occurred_at: rows[1].occurred_at,
            kind: rows[1].kind,
            amount_minor: 200,
            merchant: Some("shop".into()),
        },
    ];
    let preview = ImportPreview::build(&rows, &existing);
    assert_eq!(
        (
            preview.new_count(),
            preview.exact_duplicate_count(),
            preview.suspected_duplicate_count(),
            preview.pending_count()
        ),
        (1, 1, 1, 2)
    );
    assert!(matches!(
        preview.rows[0].decision,
        PreviewDecision::ExactDuplicate { .. }
    ));
    assert!(matches!(
        preview.rows[1].decision,
        PreviewDecision::SuspectedDuplicate { .. }
    ));
}

#[test]
fn suspected_duplicates_match_other_origins_while_exact_ids_stay_source_scoped() {
    let row = row(Some("same-id"), "2026-10-01T00:00:00Z", 100);
    let transaction_id = ledger_domain::TransactionId::new();
    for source in [
        SourceKind::Manual,
        SourceKind::Notification,
        SourceKind::Alipay,
    ] {
        let existing = ExistingImport {
            transaction_id: Some(transaction_id),
            source_kind: source,
            external_transaction_id: Some("same-id".into()),
            occurred_at: row.occurred_at,
            kind: row.kind,
            amount_minor: row.amount_minor,
            merchant: row.merchant.clone(),
        };
        let preview = ImportPreview::build(std::slice::from_ref(&row), &[existing]);
        assert_eq!(preview.exact_duplicate_count(), 0);
        assert_eq!(preview.suspected_duplicate_count(), 1);
        assert!(
            matches!(preview.rows[0].decision, PreviewDecision::SuspectedDuplicate { transaction_id: Some(id) } if id == transaction_id)
        );
    }
}
