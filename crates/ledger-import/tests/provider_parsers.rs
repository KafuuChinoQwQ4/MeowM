use ledger_domain::{SourceKind, TransactionKind};
use ledger_import::{
    ImportError, ImportPreview, parse_alipay_csv, parse_bank_csv, parse_wechat_csv,
};
use std::fmt::Write as _;

#[test]
fn platform_and_merchant_order_ids_are_separate_and_preserve_leading_zeroes() {
    for (parse, header) in [
        (
            parse_wechat_csv as fn(&[u8]) -> Result<ledger_import::ParsedImport, ImportError>,
            "交易单号,商户单号",
        ),
        (
            parse_alipay_csv as fn(&[u8]) -> Result<ledger_import::ParsedImport, ImportError>,
            "交易订单号,商家订单号",
        ),
    ] {
        let csv = format!(
            "交易时间,交易对方,收/支,交易金额,{header}\n2026-10-29 12:34:56,商户,支出,6.64,00001234,00005678\n2026-10-29 12:34:56,商户,支出,6.64,,00009999\n"
        );
        let parsed: ledger_import::ParsedImport = parse(csv.as_bytes()).unwrap();
        assert_eq!(
            parsed.rows[0].external_transaction_id.as_deref(),
            Some("00001234")
        );
        assert_eq!(
            parsed.rows[0].merchant_order_id.as_deref(),
            Some("00005678")
        );
        assert!(parsed.rows[1].external_transaction_id.is_none());
        assert_eq!(
            parsed.rows[1].merchant_order_id.as_deref(),
            Some("00009999")
        );
    }
}

#[test]
fn wechat_utf8_bom_quoted_rows_and_direction_are_normalized() {
    let csv = "\u{feff}交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-01 08:30:00,消费,咖啡店,早餐,支出,12.30,wx-1\n2026-10-02,转账,公司,,收入,1,wx-2\n";
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.source_kind, SourceKind::WeChat);
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[0].amount_minor, 1230);
    assert_eq!(parsed.rows[1].kind, TransactionKind::Income);
    assert_eq!(parsed.rows[1].amount_minor, 100);
    assert_ne!(parsed.rows[0].source_fields_digest, [0; 32]);
}

#[test]
fn wechat_slash_direction_uses_wallet_transaction_type_and_status() {
    let csv = "交易时间,交易类型,当前状态,收/支,金额(元)\n2026-10-01,零钱充值,充值完成,/,1.00\n2026-10-02,零钱提现,提现已到账,/,2.00\n2026-10-03,其他,已存入零钱,/,3.00\n2026-10-04,未知交易,完成,/,4.00\n";
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();

    assert_eq!(
        parsed.rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
        [
            TransactionKind::Income,
            TransactionKind::Expense,
            TransactionKind::Income,
        ]
    );
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].row, 5);
    assert_eq!(parsed.errors[0].code, "invalid_direction");
}

#[test]
fn alipay_refund_and_duplicate_external_id_are_explicit() {
    let csv = "交易时间,交易对方,商品说明,收/支,交易金额,交易订单号\n2026/10/01 12:00:00,商户,退货,退款,5.00,a-1\n2026/10/01 12:00:00,商户,退货,退款,5.00,a-1\n";
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows[0].kind, TransactionKind::Refund);
    assert_eq!(
        parsed.rows[1].warnings[0].code,
        "duplicate_external_transaction_id"
    );
}

#[test]
fn alipay_gbk_export_with_preamble_and_record_time_headers_is_supported() {
    // Synthetic GBK bytes with the same layout as the cashbook export.
    let gbk = b"\xd5\xcb\xb5\xa5\xcb\xb5\xc3\xf7\r\n\
        \xbc\xc7\xc2\xbc\xca\xb1\xbc\xe4,\xb7\xd6\xc0\xe0,\xca\xd5\xd6\xa7\xc0\xe0\xd0\xcd,\xbd\xf0\xb6\xee,\xb1\xb8\xd7\xa2,\xd5\xcb\xbb\xa7,\xc0\xb4\xd4\xb4,\xb1\xea\xc7\xa9\r\n\
        2026-10-29 12:34:56,\xb2\xcd\xd2\xfb,\xd6\xa7\xb3\xf6,12.30,\xce\xe7\xb2\xcd,\xd6\xa7\xb8\xb6\xb1\xa6,\xd5\xcb\xb5\xa5,\r\n";

    let parsed = parse_alipay_csv(gbk).unwrap();
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].row_number, 3);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[0].amount_minor, 1230);
    assert_eq!(parsed.rows[0].description.as_deref(), Some("午餐"));
    assert!(parsed.rows[0].merchant.is_none());
}

#[test]
fn alipay_legacy_encodings_preserve_the_same_rows_and_dedupe_digest_as_utf8() {
    let csv = "交易时间,交易对方,商品说明,收/支,交易金额,交易订单号\n2026-10-29 12:34:56,咖啡店,午餐,支出,12.30,alipay-encoding-test\n";
    let expected = parse_alipay_csv(csv.as_bytes()).unwrap();
    for encoding in [encoding_rs::GBK, encoding_rs::GB18030] {
        let (bytes, _, had_errors) = encoding.encode(csv);
        assert!(!had_errors);
        assert_eq!(parse_alipay_csv(&bytes).unwrap(), expected);
    }
    let supplementary = csv.replace("午餐", "午餐𠮷");
    let (bytes, _, had_errors) = encoding_rs::GB18030.encode(&supplementary);
    assert!(!had_errors);
    assert_eq!(
        parse_alipay_csv(&bytes).unwrap(),
        parse_alipay_csv(supplementary.as_bytes()).unwrap()
    );
}

#[test]
fn alipay_cashbook_preserves_refunds_and_transfers_without_counting_them_as_income() {
    let csv = "记录时间,分类,收支类型,金额,备注\n2026-10-29 12:34:56,退款,不计收支,1.00,测试\n2026-10-29 12:34:56,借款,不计收支,1.00,测试\n2026-10-29 12:34:56,工资,收入,2.00,测试\n";
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 3);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Refund);
    assert_eq!(parsed.rows[1].kind, TransactionKind::Transfer);
    assert_eq!(parsed.rows[2].kind, TransactionKind::Income);
    assert!(parsed.errors.is_empty());
}

#[test]
fn idless_cashbook_digests_include_exported_account_names() {
    let csv = "记录时间,分类,收支类型,金额,备注,账户\n2026-10-29 12:34:56,转账,不计收支,2.80,转账,银行|余额\n2026-10-29 12:34:56,转账,不计收支,2.80,转账,余额|余额宝\n";
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_ne!(
        parsed.rows[0].source_fields_digest,
        parsed.rows[1].source_fields_digest
    );
}

#[test]
fn bank_csv_supports_negative_amount_and_date_only() {
    let csv = "日期,摘要,借贷标记,交易金额,流水号\n2026-10-03,超市,借,-1,bank-1\n";
    let parsed = parse_bank_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.source_kind, SourceKind::Bank);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[0].amount_minor, 100);
}

#[test]
fn bank_csv_accepts_10000_rows_and_rejects_more() {
    let mut csv = String::from("日期,交易金额,借贷标记,流水号\n");
    for index in 0..10_000 {
        writeln!(csv, "2026-10-03,1,借,bank-{index}").unwrap();
    }

    let parsed = parse_bank_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 10_000);
    let preview = ImportPreview::build(&parsed.rows, &[]);
    assert_eq!(preview.counts.new_count(), 10_000);

    writeln!(csv, "2026-10-03,1,借,bank-over-limit").unwrap();
    let error = parse_bank_csv(csv.as_bytes()).unwrap_err().to_string();
    assert!(error.contains("10,000"));
}

#[test]
fn malformed_dates_and_missing_headers_are_clear_errors() {
    let invalid = "交易时间,收/支,金额(元)\nnot-a-date,支出,1\n";
    let preview = parse_wechat_csv(invalid.as_bytes()).unwrap();
    assert!(preview.rows.is_empty());
    assert_eq!(preview.errors[0].row, 2);
    assert_eq!(preview.errors[0].code, "invalid_date");
    let missing = "交易时间,收/支\n2026-01-01,支出\n";
    assert!(matches!(
        parse_wechat_csv(missing.as_bytes()),
        Err(ImportError::MissingHeaders(_))
    ));
}

#[test]
fn row_errors_are_previewed_and_all_decimal_edge_cases_are_integer_checked() {
    let csv = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-01-01,消费,\"店,铺\",商品,支出,1.01,a\n2026-01-01,消费,,,支出,,b\n2026-01-01,消费,,,支出,1.001,c\n2026-01-01,消费,,,支出,99999999999999999999999,d\n";
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].merchant.as_deref(), Some("店,铺"));
    assert_eq!(parsed.rows[0].amount_minor, 101);
    assert_eq!(parsed.errors.len(), 3);
    assert_eq!(parsed.errors[0].code, "missing_amount");
    assert!(matches!(
        parse_wechat_csv(&[0xff]),
        Err(ImportError::InvalidUtf8)
    ));
}

#[test]
fn bank_positive_debit_labels_take_precedence_and_unknown_directions_are_errors() {
    let csv = "交易日期,交易金额,交易方向,流水号\n2026-10-03,100,转出,out\n2026-10-03,100,付款,pay\n2026-10-03,100,借,borrow\n2026-10-03,100,支出,expense\n2026-10-03,100,转入,in\n2026-10-03,100,贷,credit\n2026-10-03,100,未知方向,unknown\n2026-10-03,-100,,negative\n";
    let parsed = parse_bank_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[1].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[2].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[3].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[4].kind, TransactionKind::Income);
    assert_eq!(parsed.rows[5].kind, TransactionKind::Income);
    assert_eq!(parsed.rows[6].kind, TransactionKind::Expense);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.errors[0].code, "invalid_direction");
}
