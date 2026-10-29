use chrono::{Local, TimeZone, Utc};
use ledger_domain::{SourceKind, TransactionKind};
use ledger_import::{Provider, parse_bill, parse_bill_with_password};

const ENCRYPTED: &[u8] = include_bytes!("fixtures/boc-encrypted.pdf");
const PLAIN: &[u8] = include_bytes!("fixtures/boc.pdf");

fn replace_text(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    use pdf_extract::{Document, Object};
    let mut document = Document::load_mem(bytes).unwrap();
    let encode = |value: &str| {
        value
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()
    };
    let from = encode(from);
    let to = encode(to);
    for page in document.get_pages().into_values() {
        let mut content = document.get_and_decode_page_content(page).unwrap();
        for operation in &mut content.operations {
            for operand in &mut operation.operands {
                if let Object::String(value, _) = operand {
                    if *value == from {
                        *value = to.clone();
                    }
                }
            }
        }
        document
            .change_page_content(page, content.encode().unwrap())
            .unwrap();
    }
    let mut result = Vec::new();
    document.save_to(&mut result).unwrap();
    result
}

#[test]
fn pdf_invalid_amounts_are_reported_without_dropping_neighboring_records() {
    let bytes = replace_text(PLAIN, "-12.30", "-12.3O");
    let parsed = parse_bill(Provider::Bank, &bytes).unwrap();
    assert_eq!(parsed.rows.len(), 3);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 1);
    assert!(parsed.errors[0].message.contains("第 1 页第 1 笔"));
}

#[test]
fn pdf_balance_discontinuity_is_visible_in_preview() {
    let bytes = replace_text(PLAIN, "9,988.70", "9,988.71");
    let parsed = parse_bill(Provider::Bank, &bytes).unwrap();
    assert_eq!(parsed.rows.len(), 4);
    assert!(
        parsed
            .warnings
            .iter()
            .any(|warning| warning.code == "pdf_balance_mismatch")
    );
}

#[test]
fn generic_bank_transfers_require_platform_mapping_instead_of_counting_as_expense() {
    let mut bytes = replace_text(PLAIN, "网上快捷支付", "账户划转");
    bytes = replace_text(&bytes, "支付宝-充值", "往来款");
    bytes = replace_text(&bytes, "支付宝", "测试往来");
    let parsed = parse_bill(Provider::Bank, &bytes).unwrap();
    let row = &parsed.rows[2];
    assert_eq!(row.kind, TransactionKind::Transfer, "{:?}", row.description);
    assert!(
        row.warnings
            .iter()
            .any(|warning| warning.code == "transfer_accounts_required")
    );
}

#[test]
fn bank_rows_with_amounts_and_balances_are_not_discarded_when_date_and_time_are_missing() {
    let mut bytes = replace_text(PLAIN, "2026-10-04", "");
    bytes = replace_text(&bytes, "00:00:00", "");
    let parsed = parse_bill(Provider::Bank, &bytes).unwrap();
    assert_eq!(parsed.rows.len(), 3);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 4);
    assert!(parsed.errors[0].message.contains("日期"));
}

#[test]
fn pdf_missing_headers_and_image_only_pages_are_rejected() {
    let missing = replace_text(PLAIN, "金额", "");
    assert!(
        parse_bill(Provider::Bank, &missing)
            .unwrap_err()
            .to_string()
            .contains("表头")
    );
    let mut document = pdf_extract::Document::load_mem(PLAIN).unwrap();
    for page in document.get_pages().into_values() {
        document.change_page_content(page, Vec::new()).unwrap();
    }
    let mut image_only = Vec::new();
    document.save_to(&mut image_only).unwrap();
    assert!(
        parse_bill(Provider::Bank, &image_only)
            .unwrap_err()
            .to_string()
            .contains("扫描件")
    );
}

#[test]
fn encrypted_bank_pdf_requires_a_password_instead_of_treating_it_as_csv() {
    let error = parse_bill(Provider::Bank, ENCRYPTED).unwrap_err();
    assert!(error.to_string().contains("PDF") && error.to_string().contains("密码"));
}

#[test]
fn encrypted_boc_pdf_retries_with_the_password_without_changing_normalized_rows() {
    let wrong = parse_bill_with_password(Provider::Bank, ENCRYPTED, Some("wrong"))
        .unwrap_err()
        .to_string();
    assert!(wrong.contains("密码不正确"));
    assert!(!wrong.contains("wrong"));
    let unlocked =
        parse_bill_with_password(Provider::Bank, ENCRYPTED, Some("fixture-only")).unwrap();
    let plain = parse_bill(Provider::Bank, PLAIN).unwrap();
    assert_eq!(unlocked, plain);
    assert!(
        parse_bill(Provider::Wechat, PLAIN)
            .unwrap_err()
            .to_string()
            .contains("银行卡")
    );
    assert!(parse_bill(Provider::Bank, b"%PDF-invalid").is_err());
}

#[test]
fn unencrypted_boc_pdf_preserves_columns_wrapped_text_and_page_order() {
    let parsed = parse_bill(Provider::Bank, PLAIN).unwrap();
    assert_eq!(parsed.source_kind, SourceKind::Bank);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.rows.len(), 4);
    let row = &parsed.rows[0];
    assert_eq!(row.amount_minor, 1230);
    assert_eq!(row.kind, TransactionKind::Expense);
    assert_eq!(row.merchant.as_deref(), Some("测试早餐店"));
    assert_eq!(
        row.description.as_deref(),
        Some("网上快捷支付 · 手机银行 · 早餐套餐")
    );
    assert_eq!(
        row.occurred_at,
        Local
            .with_ymd_and_hms(2026, 10, 3, 8, 30, 1)
            .unwrap()
            .with_timezone(&Utc)
    );
    assert!(row.external_transaction_id.is_none());
    assert!(row.merchant_order_id.is_none());
    assert_eq!(parsed.rows[1].kind, TransactionKind::Refund);
    assert_eq!(parsed.rows[1].amount_minor, 100);
    assert_eq!(parsed.rows[2].kind, TransactionKind::Transfer);
    assert_eq!(parsed.rows[2].amount_minor, 10000);
    assert_eq!(
        parsed.rows[2].transfer_direction,
        Some(ledger_import::TransferDirection::Outgoing)
    );
    assert_eq!(parsed.rows[3].kind, TransactionKind::Income);
    assert_eq!(parsed.rows[3].amount_minor, 29);
    assert!(parsed.rows.iter().all(|row| {
        row.warnings
            .iter()
            .any(|warning| warning.code == "pdf_location")
    }));
}

#[test]
fn signed_bank_transfer_amount_identifies_incoming_direction() {
    let mut bytes = replace_text(PLAIN, "-100.00", "100.00");
    bytes = replace_text(&bytes, "9,888.70", "10,088.70");
    bytes = replace_text(&bytes, "9,888.99", "10,088.99");
    let parsed = parse_bill(Provider::Bank, &bytes).unwrap();
    assert_eq!(parsed.rows[2].kind, TransactionKind::Transfer);
    assert_eq!(
        parsed.rows[2].transfer_direction,
        Some(ledger_import::TransferDirection::Incoming)
    );
}

#[test]
fn repeated_text_imports_without_a_bank_id_are_exact_duplicates() {
    use ledger_import::{ImportPreview, PreviewDecision};
    let parsed = parse_bill(Provider::Bank, PLAIN).unwrap();
    let first = ImportPreview::build(&parsed.rows, &[]);
    assert_eq!(first.new_count(), 4);
    let existing: Vec<_> = first
        .rows
        .iter()
        .map(|item| {
            let row = &item.row;
            ledger_import::ExistingImport {
                transaction_id: Some(ledger_domain::TransactionId::new()),
                source_kind: row.source_kind,
                external_transaction_id: None,
                merchant_order_id: None,
                source_fields_digest: Some(row.source_fields_digest),
                occurred_at: row.occurred_at,
                kind: row.kind,
                amount_minor: row.amount_minor,
                merchant: row.merchant.clone(),
                description: row.description.clone(),
                date_only: false,
                account_ids: Vec::new(),
            }
        })
        .collect();
    let second = ImportPreview::build(&parsed.rows, &existing);
    assert_eq!(second.exact_duplicate_count(), 4);
    assert!(
        second
            .rows
            .iter()
            .all(|row| matches!(row.decision, PreviewDecision::ExactDuplicate { .. }))
    );
}
