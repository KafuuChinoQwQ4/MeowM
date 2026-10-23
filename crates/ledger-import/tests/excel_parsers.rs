use chrono::{Local, TimeZone, Utc};
use ledger_domain::{SourceKind, TransactionKind};
use ledger_import::{Provider, parse_bill};
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const WECHAT: &[u8] = include_bytes!("fixtures/wechat.xlsx");

#[test]
fn xlsx_skips_notes_and_preserves_dates_text_ids_and_original_row_numbers() {
    let parsed = parse_bill(Provider::Wechat, WECHAT).unwrap();
    assert_eq!(parsed.source_kind, SourceKind::WeChat);
    assert_eq!(parsed.rows.len(), 2);
    let row = &parsed.rows[0];
    assert_eq!(row.row_number, 5);
    assert_eq!(row.amount_minor, 1230);
    assert_eq!(row.kind, TransactionKind::Expense);
    assert_eq!(
        row.external_transaction_id.as_deref(),
        Some("00012345678901234567890")
    );
    assert_eq!(row.merchant.as_deref(), Some("店,铺"));
    assert_eq!(row.description.as_deref(), Some("早餐\"套餐\"\n饮品"));
    assert_eq!(
        row.occurred_at,
        Local
            .with_ymd_and_hms(2026, 10, 2, 8, 30, 0)
            .unwrap()
            .with_timezone(&Utc)
    );
    assert_eq!(parsed.rows[1].row_number, 6);
    assert_eq!(parsed.rows[1].amount_minor, 29);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 8);
    assert_eq!(parsed.errors[0].code, "invalid_date");
}

#[test]
fn xls_alipay_retains_numeric_amounts_and_refunds() {
    let parsed = parse_bill(Provider::Alipay, include_bytes!("fixtures/alipay.xls")).unwrap();
    assert!(parsed.errors.is_empty());
    assert_eq!(parsed.rows[0].amount_minor, 29);
    assert_eq!(
        parsed.rows[0].external_transaction_id.as_deref(),
        Some("00098765432109876543210")
    );
    assert_eq!(parsed.rows[1].kind, TransactionKind::Refund);
    assert_eq!(parsed.rows[1].amount_minor, 500);
}

#[test]
fn xls_bank_uses_the_workbooks_1904_date_epoch() {
    let parsed = parse_bill(Provider::Bank, include_bytes!("fixtures/bank-1904.xls")).unwrap();
    assert!(parsed.errors.is_empty());
    assert_eq!(parsed.rows[0].amount_minor, 10001);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(
        parsed.rows[0].occurred_at,
        Local
            .with_ymd_and_hms(2026, 10, 3, 0, 0, 0)
            .unwrap()
            .with_timezone(&Utc)
    );
}

#[test]
fn unified_import_still_reads_csv_with_the_same_normalized_digest() {
    let csv = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 08:30:00,消费,\"店,铺\",\"早餐\"\"套餐\"\"\n饮品\",支出,12.30,00012345678901234567890\n";
    let csv = parse_bill(Provider::Wechat, csv.as_bytes()).unwrap();
    let excel = parse_bill(Provider::Wechat, WECHAT).unwrap();
    assert_eq!(
        csv.rows[0].source_fields_digest,
        excel.rows[0].source_fields_digest
    );
}

#[test]
fn excel_cell_errors_and_unsafe_numeric_ids_stay_out_of_importable_rows() {
    let xml = bank_sheet(&[
        (
            2,
            "2026-10-03",
            "<c r=\"B2\"><v>0.29</v></c>",
            "<c r=\"D2\"><v>12345</v></c>",
        ),
        (3, "2026-10-03", "<c r=\"B3\"><v>1.001</v></c>", ""),
        (
            4,
            "2026-10-03",
            "<c r=\"B4\"><v>1</v></c>",
            "<c r=\"D4\"><v>12345678901234567</v></c>",
        ),
        (
            5,
            "2026-10-03",
            "<c r=\"B5\" t=\"e\"><v>#DIV/0!</v></c>",
            "",
        ),
    ]);
    let parsed = parse_bill(Provider::Bank, &replace_sheets(&xml, None)).unwrap();
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].amount_minor, 29);
    assert_eq!(
        parsed.rows[0].external_transaction_id.as_deref(),
        Some("12345")
    );
    assert_eq!(
        parsed.errors.iter().map(|e| e.row).collect::<Vec<_>>(),
        [3, 4, 5]
    );
    assert_eq!(parsed.errors[0].code, "invalid_amount");
    assert!(parsed.errors[1].message.contains("文本"));
}

#[test]
fn large_numeric_excel_amounts_are_rejected_before_float_precision_can_change_cents() {
    let xml = bank_sheet(&[
        (
            2,
            "2026-10-03",
            "<c r=\"B2\"><v>70368744177664.01</v></c>",
            "",
        ),
        (3, "2026-10-03", "<c r=\"B3\"><v>12.30</v></c>", ""),
    ]);
    let parsed = parse_bill(Provider::Bank, &replace_sheets(&xml, None)).unwrap();
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].amount_minor, 1230);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 2);
    assert!(parsed.errors[0].message.contains("文本"));
}

#[test]
fn malformed_workbooks_and_multiple_bill_sheets_have_actionable_errors() {
    for bytes in [
        b"PK\x03\x04broken".as_slice(),
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1broken".as_slice(),
    ] {
        let message = parse_bill(Provider::Bank, bytes).unwrap_err().to_string();
        assert!(message.contains("Excel"));
    }
    let xml = bank_sheet(&[(2, "2026-10-03", "<c r=\"B2\"><v>1</v></c>", "")]);
    let bytes = replace_sheets(&xml, Some(&xml));
    let message = parse_bill(Provider::Bank, &bytes).unwrap_err().to_string();
    assert!(message.contains("多张"));
    assert!(message.contains("分别"));
    let empty = "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>";
    let message = parse_bill(Provider::Bank, &replace_sheets(empty, None))
        .unwrap_err()
        .to_string();
    assert!(message.contains("表头"));
}

#[test]
fn sparse_xls_extents_are_rejected_before_dense_workbook_loading() {
    let error =
        parse_bill(Provider::Bank, include_bytes!("fixtures/sparse-large.xls")).unwrap_err();
    assert!(error.to_string().contains("128 列"));
}

#[test]
fn row_limit_includes_invalid_rows_and_sparse_cells_do_not_expand_a_dense_range() {
    let sparse = bank_sheet(&[(
        1_000_000,
        "2026-10-03",
        "<c r=\"B1000000\"><v>1</v></c>",
        "",
    )]);
    let parsed = parse_bill(Provider::Bank, &replace_sheets(&sparse, None)).unwrap();
    assert_eq!(parsed.rows[0].row_number, 1_000_000);
    let wide = sparse.replace("B1000000", "XFD1000000");
    assert!(parse_bill(Provider::Bank, &replace_sheets(&wide, None)).is_err());
    let rows: Vec<_> = (2..=1002).map(|n| (n, "invalid-date", "", "")).collect();
    let message = parse_bill(Provider::Bank, &replace_sheets(&bank_sheet(&rows), None))
        .unwrap_err()
        .to_string();
    assert!(message.contains("1000"));
}

// The ZIP container remains a real Excel file; only the synthetic worksheet XML changes.
fn replace_sheets(bill_xml: &str, notes_xml: Option<&str>) -> Vec<u8> {
    let mut source = ZipArchive::new(Cursor::new(WECHAT)).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        output
            .start_file(&name, SimpleFileOptions::default())
            .unwrap();
        let replacement = match name.as_str() {
            "xl/worksheets/sheet2.xml" => Some(bill_xml),
            "xl/worksheets/sheet1.xml" => notes_xml,
            _ => None,
        };
        if let Some(xml) = replacement {
            output.write_all(xml.as_bytes()).unwrap();
        } else {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            output.write_all(&bytes).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}

fn bank_sheet(rows: &[(usize, &str, &str, &str)]) -> String {
    let mut xml = String::from(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>日期</t></is></c><c r=\"B1\" t=\"inlineStr\"><is><t>金额</t></is></c><c r=\"C1\" t=\"inlineStr\"><is><t>收支</t></is></c><c r=\"D1\" t=\"inlineStr\"><is><t>流水号</t></is></c></row>",
    );
    for (n, date, amount_cell, id_cell) in rows {
        xml.push_str(&format!("<row r=\"{n}\"><c r=\"A{n}\" t=\"inlineStr\"><is><t>{date}</t></is></c>{amount_cell}<c r=\"C{n}\" t=\"inlineStr\"><is><t>支出</t></is></c>{id_cell}</row>"));
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}
