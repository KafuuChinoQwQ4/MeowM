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
        merchant_order_id: None,
        original_external_transaction_id: None,
        occurred_at,
        kind,
        transfer_direction: None,
        amount_minor: amount,
        currency: currency.clone(),
        merchant: Some("shop".into()),
        description: None,
        account_hint: None,
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

fn saved(row: &NormalizedImportRow) -> ExistingImport {
    ExistingImport {
        transaction_id: Some(ledger_domain::TransactionId::new()),
        source_kind: row.source_kind,
        external_transaction_id: row.external_transaction_id.clone(),
        merchant_order_id: row.merchant_order_id.clone(),
        source_fields_digest: Some(row.source_fields_digest),
        occurred_at: row.occurred_at,
        kind: row.kind,
        amount_minor: row.amount_minor,
        merchant: row.merchant.clone(),
        description: row.description.clone(),
        date_only: false,
        account_ids: vec![],
    }
}

#[test]
fn merchant_order_is_auxiliary_and_cannot_override_conflicting_platform_ids() {
    let mut incoming = row(Some("new-platform-id"), "2026-10-01T04:00:00Z", 664);
    incoming.merchant_order_id = Some("00001234".into());
    let mut candidate = saved(&incoming);
    candidate.external_transaction_id = None;
    candidate.merchant = Some("另一种商户名称".into());
    candidate.occurred_at += chrono::Duration::hours(1);
    assert_eq!(
        ImportPreview::build(&[incoming.clone()], &[candidate.clone()]).suspected_duplicate_count(),
        1
    );
    candidate.external_transaction_id = Some("different-platform-id".into());
    let mut fallback = candidate.clone();
    fallback.source_kind = SourceKind::Manual;
    fallback.external_transaction_id = None;
    fallback.merchant = incoming.merchant.clone();
    fallback.occurred_at = incoming.occurred_at;
    assert_eq!(
        ImportPreview::build(&[incoming], &[candidate, fallback]).new_count(),
        1
    );
}

#[test]
fn fuzzy_matching_is_account_scoped_but_platform_ids_are_global() {
    let incoming = row(Some("stable"), "2026-10-01T04:00:00Z", 664);
    let target = ledger_domain::AccountId::new();
    let mut candidate = saved(&incoming);
    candidate.account_ids = vec![ledger_domain::AccountId::new()];
    assert_eq!(
        ImportPreview::build_for_account(&[incoming.clone()], &[candidate.clone()], Some(target))
            .exact_duplicate_count(),
        1
    );
    candidate.external_transaction_id = None;
    assert_eq!(
        ImportPreview::build_for_account(&[incoming], &[candidate], Some(target)).new_count(),
        1
    );
}

#[test]
fn unchanged_idless_export_is_exact_and_legacy_date_only_manual_is_suspected() {
    let incoming = row(None, "2026-10-01T04:00:00Z", 664);
    let mut candidate = saved(&incoming);
    assert_eq!(
        ImportPreview::build(&[incoming.clone()], &[candidate.clone()]).exact_duplicate_count(),
        1
    );
    candidate.source_kind = SourceKind::Manual;
    candidate.source_fields_digest = None;
    candidate.date_only = true;
    candidate.occurred_at += chrono::Duration::hours(2);
    assert_eq!(
        ImportPreview::build(&[incoming], &[candidate]).suspected_duplicate_count(),
        1
    );
}

#[test]
fn idless_digest_matches_saved_occurrences_without_discarding_extra_statement_rows() {
    let incoming = row(None, "2026-10-01T04:00:00Z", 664);
    let rows = [incoming.clone(), incoming.clone()];
    let first = ImportPreview::build(&rows, &[]);
    assert_eq!(first.new_count(), 1);
    assert_eq!(first.exact_duplicate_count(), 0);
    assert_eq!(first.suspected_duplicate_count(), 1);
    let reimport = ImportPreview::build(&rows, &[saved(&incoming)]);
    assert_eq!(reimport.exact_duplicate_count(), 1);
    assert_eq!(reimport.suspected_duplicate_count(), 1);
    assert_eq!(
        ImportPreview::build(&rows, &[saved(&incoming), saved(&incoming)]).exact_duplicate_count(),
        2
    );
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

            merchant_order_id: None,
            source_fields_digest: None,
            description: None,
            date_only: false,
            account_ids: vec![],
        },
        ExistingImport {
            transaction_id: None,
            source_kind: SourceKind::WeChat,
            external_transaction_id: None,
            occurred_at: rows[1].occurred_at,
            kind: rows[1].kind,
            amount_minor: 200,
            merchant: Some("shop".into()),

            merchant_order_id: None,
            source_fields_digest: None,
            description: None,
            date_only: false,
            account_ids: vec![],
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

            merchant_order_id: None,
            source_fields_digest: None,
            description: None,
            date_only: false,
            account_ids: vec![],
        };
        let preview = ImportPreview::build(std::slice::from_ref(&row), &[existing]);
        assert_eq!(preview.exact_duplicate_count(), 0);
        assert_eq!(preview.suspected_duplicate_count(), 1);
        assert!(
            matches!(preview.rows[0].decision, PreviewDecision::SuspectedDuplicate { transaction_id: Some(id) } if id == transaction_id)
        );
    }
}

#[test]
fn consecutive_orders_with_distinct_platform_ids_are_new() {
    let rows = [
        row(Some("order-1"), "2026-10-01T04:00:00Z", 1500),
        row(Some("order-2"), "2026-10-01T04:01:00Z", 1500),
    ];
    let preview = ImportPreview::build(&rows, &[]);
    assert_eq!(preview.new_count(), 2);
    assert_eq!(preview.suspected_duplicate_count(), 0);
}

#[test]
fn imported_platform_merchant_can_match_a_manual_shop_in_the_product_text() {
    let mut incoming = row(Some("wechat-order"), "2026-10-29T09:26:28Z", 664);
    incoming.merchant = Some("美团".into());
    incoming.description = Some("饭小侠-美团App-261029-synthetic".into());
    let manual = ExistingImport {
        transaction_id: Some(ledger_domain::TransactionId::new()),
        source_kind: SourceKind::Manual,
        external_transaction_id: None,
        occurred_at: incoming.occurred_at + chrono::Duration::seconds(62),
        kind: incoming.kind,
        amount_minor: 664,
        merchant: Some("饭小侠".into()),

        merchant_order_id: None,
        source_fields_digest: None,
        description: None,
        date_only: false,
        account_ids: vec![],
    };
    let preview = ImportPreview::build(&[incoming], &[manual]);
    assert_eq!(preview.suspected_duplicate_count(), 1);
    assert_eq!(preview.new_count(), 0);
}

#[test]
fn fuzzy_matches_require_close_times_and_known_matching_merchants() {
    let incoming = row(None, "2026-10-01T04:00:00Z", 1500);
    for (at, merchant, suspected) in [
        ("2026-10-01T03:55:00Z", Some("shop"), true),
        ("2026-10-01T04:05:00Z", Some("shop"), true),
        ("2026-10-01T04:05:00.001Z", Some("shop"), false),
        ("2026-10-01T10:00:00Z", Some("shop"), false),
        ("2026-10-01T04:00:00Z", Some("other shop"), false),
        ("2026-10-01T04:00:00Z", None, false),
        ("2026-10-01T04:00:00Z", Some(""), false),
    ] {
        let candidate = ExistingImport {
            transaction_id: None,
            source_kind: SourceKind::Manual,
            external_transaction_id: None,
            occurred_at: DateTime::parse_from_rfc3339(at)
                .unwrap()
                .with_timezone(&Utc),
            kind: incoming.kind,
            amount_minor: incoming.amount_minor,
            merchant: merchant.map(str::to_owned),

            merchant_order_id: None,
            source_fields_digest: None,
            description: None,
            date_only: false,
            account_ids: vec![],
        };
        let preview = ImportPreview::build(std::slice::from_ref(&incoming), &[candidate]);
        assert_eq!(
            preview.suspected_duplicate_count(),
            usize::from(suspected),
            "candidate at {at} with merchant {merchant:?}"
        );
    }
}
