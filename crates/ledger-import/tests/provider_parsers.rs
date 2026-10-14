use ledger_domain::{SourceKind, TransactionKind};
use ledger_import::{ImportError, parse_alipay_csv, parse_bank_csv, parse_wechat_csv};

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
fn bank_csv_supports_negative_amount_and_date_only() {
    let csv = "日期,摘要,借贷标记,交易金额,流水号\n2026-10-03,超市,借,-1,bank-1\n";
    let parsed = parse_bank_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.source_kind, SourceKind::Bank);
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[0].amount_minor, 100);
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
