use ledger_domain::{TransactionKind, TransactionStatus};
use ledger_import::{ImportSource, WarningCode, parse_alipay_csv, parse_wechat_csv};

const WECHAT_HEADER: &str = "交易时间,交易类型,交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号,商户单号,备注,原交易单号";
const ALIPAY_HEADER: &str = "交易时间,交易分类,交易对方,对方账号,商品说明,收/支,金额,收/付款方式,交易状态,交易订单号,商家订单号,备注";

#[test]
fn wechat_bom_preamble_and_footer_preserve_integer_amount_and_pending_provenance() {
    let csv = format!(
        "\u{feff}微信支付账单明细\r\n微信昵称：[脱敏]\r\n起始时间：[2026-10-01 00:00:00]\r\n共1笔记录\r\n----------------------微信支付账单明细列表--------------------\r\n{WECHAT_HEADER}\r\n2026-10-01 08:01:02,商户消费,示例商店,\"茶,点心\",支出,¥12.34,零钱,支付成功,wx-001\t,shop-1,/,\r\n--------------------\r\n共1笔记录\r\n"
    );
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.source, ImportSource::WeChat);
    assert_eq!(parsed.rows.len(), 1);
    assert!(parsed.warnings.is_empty());
    let row = &parsed.rows[0];
    assert_eq!(row.occurred_at.to_rfc3339(), "2026-10-01T00:01:02+00:00");
    assert_eq!(row.amount.minor_units(), 1234);
    assert_eq!(row.report_amount_cny.as_ref().unwrap().minor_units(), 1234);
    assert_eq!(row.kind, TransactionKind::Expense);
    assert_eq!(row.status, TransactionStatus::Pending);
    assert_eq!(row.account_hint.as_deref(), Some("零钱"));
    assert_eq!(row.merchant.as_deref(), Some("示例商店"));
    assert_eq!(row.raw.row_number, 7);
    assert_eq!(row.raw.external_transaction_id.as_deref(), Some("wx-001"));
    assert_eq!(row.raw.description.as_deref(), Some("茶,点心"));
    assert_eq!(row.raw.amount_text, "¥12.34");
    assert_eq!(parsed.raw_rows.len(), 1);
    assert_ne!(row.raw.source_fields_digest, [0; 32]);
}

#[test]
fn alipay_utf8_handles_quoted_thousands_and_duplicate_external_ids_without_dropping_rows() {
    let csv = format!(
        "支付宝交易记录明细查询\n起始日期：2026-10-01\n{ALIPAY_HEADER}\n2026-10-02 09:10:11,餐饮美食,示例餐厅,masked,套餐,支出,\"1,234.56\",余额宝,交易成功,ali-1,order-1,/\n2026-10-02 09:10:11,餐饮美食,示例餐厅,masked,套餐,支出,\"1,234.56\",余额宝,交易成功,ali-1,order-1,/\n--------------------\n共2笔记录\n"
    );
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.source, ImportSource::Alipay);
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0].amount.minor_units(), 123456);
    assert_eq!(
        parsed.rows[0].raw.external_transaction_id.as_deref(),
        Some("ali-1")
    );
    assert_eq!(
        parsed.rows[0].raw.source_fields_digest,
        parsed.rows[1].raw.source_fields_digest
    );
}

#[test]
fn legacy_alipay_header_and_refund_reference_are_supported() {
    let csv = "交易号,商家订单号,交易创建时间,付款时间,最近修改时间,交易来源地,类型,交易对方,商品名称,金额（元）,收/支,交易状态,服务费（元）,成功退款（元）,备注,资金状态,原交易号\nrefund-1,shop-r,2026-10-02 12:00:00,,,其他,退款,示例商店,退货,8.20,收入,退款成功,0.00,8.20,/,已收入,original-1\n";
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    let row = &parsed.rows[0];
    assert_eq!(row.kind, TransactionKind::Refund);
    assert_eq!(row.amount.minor_units(), 820);
    assert_eq!(
        row.raw.original_external_transaction_id.as_deref(),
        Some("original-1")
    );
}

#[test]
fn refunded_original_expense_is_not_reclassified_and_unlinked_refund_is_flagged() {
    let csv = format!(
        "{WECHAT_HEADER}\n2026-10-01 08:00:00,商户消费,示例商店,购买,支出,10.00,零钱,已全额退款,wx-original,shop-1,/,\n2026-10-02 08:00:00,商户退款,示例商店,退货,收入,10.00,零钱,退款成功,wx-refund,shop-r,/,\n"
    );
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows[0].kind, TransactionKind::Expense);
    assert_eq!(parsed.rows[1].kind, TransactionKind::Refund);
    assert!(
        parsed.rows[1]
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::MissingRefundOriginal)
    );
}

#[test]
fn invalid_rows_are_reported_with_original_line_numbers_and_do_not_hide_valid_rows() {
    let csv = format!(
        "{WECHAT_HEADER}\n2026-10-01 08:00:00,商户消费,商店,商品,支出,,零钱,支付成功,bad-amount,,,\n2026-02-30 08:00:00,商户消费,商店,商品,支出,1,零钱,支付成功,bad-date,,,\n2026-10-01 08:00:00,其他,商店,商品,不计收支,1,零钱,支付成功,bad-direction,,,\n2026-10-01 08:00:00,商户消费,商店,商品,支出,2.01,零钱,支付成功,good,,,\n"
    );
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.raw_rows.len(), 4);
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].amount.minor_units(), 201);
    for (line, code) in [
        (2, WarningCode::MissingAmount),
        (3, WarningCode::InvalidDate),
        (4, WarningCode::UnknownDirection),
    ] {
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.row_number == line && w.code == code),
            "line {line}: {:?}",
            parsed.warnings
        );
    }
}

#[test]
fn foreign_currency_retains_original_amount_and_requires_cny_conversion() {
    let csv = format!(
        "{ALIPAY_HEADER},币种\n2026-10-01 08:00:00,购物,商店,masked,商品,支出,USD 12.34,信用卡,交易成功,usd-1,,,USD\n2026-10-01 08:00:00,购物,商店,masked,商品,支出,1234,信用卡,交易成功,jpy-1,,,JPY\n"
    );
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0].amount.currency().code(), "USD");
    assert_eq!(parsed.rows[0].amount.minor_units(), 1234);
    assert_eq!(parsed.rows[1].amount.currency().minor_unit_exponent(), 0);
    assert_eq!(parsed.rows[1].amount.minor_units(), 1234);
    for row in &parsed.rows {
        assert!(row.report_amount_cny.is_none());
        assert!(
            row.warnings
                .iter()
                .any(|w| w.code == WarningCode::MissingExchangeRate)
        );
    }
}

#[test]
fn unknown_headers_and_invalid_encoding_produce_readable_errors() {
    for parse in [parse_wechat_csv, parse_alipay_csv] {
        let error = parse(b"date,amount\n2026-10-01,12.00\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("表头"));
        let error = parse(&[0xff, 0xfe]).unwrap_err().to_string();
        assert!(error.contains("UTF-8"));
    }
}

#[test]
fn unsafe_amounts_and_failed_transactions_are_not_candidates() {
    for amount in [
        "1.001",
        "92233720368547758.08",
        "-12.00",
        "NaN",
        "1e3",
        "\"1,23.00\"",
        "USD 2.00",
    ] {
        let csv = format!(
            "{WECHAT_HEADER}\n2026-10-01 08:00:00,商户消费,商店,商品,支出,{amount},零钱,支付成功,id,,,\n"
        );
        let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
        assert!(
            parsed.rows.is_empty(),
            "unexpected accepted amount {amount}"
        );
        assert!(!parsed.warnings.is_empty());
    }
    let csv = format!(
        "{WECHAT_HEADER}\n2026-10-01 08:00:00,商户消费,商店,商品,支出,12.00,零钱,交易关闭,id,,,\n"
    );
    let parsed = parse_wechat_csv(csv.as_bytes()).unwrap();
    assert!(parsed.rows.is_empty());
    assert!(
        parsed
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::UnsettledTransaction)
    );
}

#[test]
fn multiline_description_keeps_physical_line_numbers_and_short_row_warns() {
    let csv = format!(
        "{ALIPAY_HEADER}\n2026-10-01 08:00:00,购物,商店,masked,\"商品\n第二行\",支出,0.01,余额,交易成功,a1,,,\n2026-10-02,broken\n2026-10-03 08:00:00,购物,商店,masked,商品,支出,0.10,余额,交易成功,a2,,\n"
    );
    // A trailing empty provider column is harmless.
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0].raw.row_number, 2);
    assert_eq!(parsed.rows[1].raw.row_number, 5);
    assert!(
        parsed
            .warnings
            .iter()
            .any(|w| w.row_number == 4 && w.code == WarningCode::MalformedRow)
    );
}

#[test]
fn alipay_supports_three_digit_currency_exponents_and_rejects_unknown_currencies() {
    let csv = format!(
        "{ALIPAY_HEADER},币种\n2026-10-01 08:00:00,购物,商店,masked,商品,支出,KWD 1.234,信用卡,交易成功,kwd-1,,,KWD\n2026-10-01 08:00:00,购物,商店,masked,商品,支出,1.234,信用卡,交易成功,bhd-1,,,BHD\n2026-10-01 08:00:00,购物,商店,masked,商品,支出,1.23,信用卡,交易成功,xxx-1,,,XXX\n"
    );
    let parsed = parse_alipay_csv(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0].amount.currency().code(), "KWD");
    assert_eq!(parsed.rows[0].amount.currency().minor_unit_exponent(), 3);
    assert_eq!(parsed.rows[0].amount.minor_units(), 1234);
    assert_eq!(parsed.rows[1].amount.currency().code(), "BHD");
    assert_eq!(parsed.rows[1].amount.currency().minor_unit_exponent(), 3);
    assert_eq!(parsed.rows[1].amount.minor_units(), 1234);
    assert!(parsed.warnings.iter().any(|warning| {
        warning.row_number == 4 && warning.code == WarningCode::UnsupportedCurrency
    }));
}

#[test]
fn malformed_csv_quotes_return_structural_errors() {
    for malformed in [
        format!(
            "{ALIPAY_HEADER}\n2026-10-01 08:00:00,购物,商店,masked,\"unterminated,支出,1.00,余额,交易成功,a1,,,\n"
        ),
        format!(
            "{ALIPAY_HEADER}\n2026-10-01 08:00:00,购物,商店,masked,商品\"bad,支出,1.00,余额,交易成功,a1,,,\n"
        ),
    ] {
        let error = parse_alipay_csv(malformed.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains("CSV") || error.contains("引号"), "{error}");
    }
}

#[test]
fn transfer_and_repayment_types_take_precedence_over_income_expense_direction() {
    for parse in [parse_wechat_csv, parse_alipay_csv] {
        for type_header in ["交易类型", "类型", "交易分类"] {
            for (kind, direction) in [
                ("信用卡还款", "支出"),
                ("转账", "收入"),
                ("转出", "支出"),
                ("转入", "不计收支"),
            ] {
                let csv = format!(
                    "交易时间,{type_header},交易对方,商品,收/支,金额(元),支付方式,当前状态,交易单号\n2026-10-07 10:30:00,{kind},本人信用卡,还款,{direction},100.00,零钱,支付成功,repay-1\n2026-10-07 10:31:00,商户消费,商店,食品,支出,2.00,零钱,支付成功,expense-1\n"
                );
                let parsed = parse(csv.as_bytes()).unwrap();
                assert_eq!(parsed.rows.len(), 2);
                assert_eq!(
                    parsed.rows[0].kind,
                    TransactionKind::Transfer,
                    "{type_header}: {kind}"
                );
                assert_eq!(parsed.rows[0].status, TransactionStatus::Pending);
                assert!(!parsed.rows[0].warnings.is_empty());
                assert!(parsed.rows[0].warnings[0].message.contains("账户"));
                assert!(parsed.rows[0].report_amount_cny.is_none());
                assert_eq!(parsed.rows[1].kind, TransactionKind::Expense);
            }
        }
    }
}

#[test]
fn optional_columns_never_borrow_values_from_unrelated_columns() {
    for parse in [parse_wechat_csv, parse_alipay_csv] {
        for csv in [
            "交易时间,金额,收/支\n2026-10-07 10:30:00,1.00,支出\n",
            "收/支,备注,金额,交易时间,其他1,其他2,其他3,其他4,其他5\n支出,备注值,1.00,2026-10-07 10:30:00,a,b,c,d,not-an-id\n",
        ] {
            let parsed = parse(csv.as_bytes()).unwrap();
            let row = &parsed.rows[0];
            assert!(row.raw.external_transaction_id.is_none());
            assert!(row.merchant.is_none());
            assert!(row.raw.merchant.is_empty());
            assert!(row.raw.original_external_transaction_id.is_none());
            assert!(row.raw.description.is_none());
            assert!(row.account_hint.is_none());
        }
    }
}
