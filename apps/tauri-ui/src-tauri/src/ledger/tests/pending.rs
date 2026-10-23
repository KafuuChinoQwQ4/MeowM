use super::super::*;
#[test]
fn excel_bills_stage_confirm_and_dedupe_against_csv() {
    for (provider, bytes, csv, amount) in [
        (
            "wechat",
            include_bytes!("../../../../../../crates/ledger-import/tests/fixtures/wechat.xlsx")
                .as_slice(),
            "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 08:30:00,消费,\"店,铺\",\"早餐\"\"套餐\"\"\n饮品\",支出,12.30,00012345678901234567890\n",
            "12.30",
        ),
        (
            "alipay",
            include_bytes!("../../../../../../crates/ledger-import/tests/fixtures/alipay.xls")
                .as_slice(),
            "交易时间,交易对方,商品说明,收/支,交易金额,交易订单号\n2026-10-02 09:10:00,商户,午餐,支出,0.29,00098765432109876543210\n",
            "0.29",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&directory.path().join("excel.sqlite")).unwrap();
        let account = ledger.accounts().unwrap()[0].id.clone();
        let preview = ledger.bill_preview(provider, bytes.to_vec()).unwrap();
        assert_eq!(preview.rows[0].amount, amount);
        let ids = ledger
            .stage_preview(
                &preview.preview_id,
                &account,
                vec![preview.rows[0].id.clone()],
                vec![],
            )
            .unwrap();
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "0.00");
        ledger.confirm_pending(ids, vec![]).unwrap();
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, amount);
        let duplicate = ledger
            .bill_preview(provider, csv.as_bytes().to_vec())
            .unwrap();
        assert_eq!(duplicate.exact_duplicates, 1);
        let duplicate = ledger.bill_preview(provider, bytes.to_vec()).unwrap();
        assert_eq!(duplicate.exact_duplicates, 1);
    }
}

#[test]
fn bill_preview_stage_and_pending_confirmation_use_real_sqlite_without_raw_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pending.sqlite");
    let ledger = Ledger::open(&path).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    ledger.set_reference(&account, "2026-10-01", "100").unwrap();
    let raw = "header private marker\n交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,私密原始列\n2026-10-02 08:00:00,消费,商户,午餐,支出,12.30,test-wx-1,RAW_PRIVATE_TEST_SENTINEL\ninvalid-date,消费,商户,,支出,1,test-wx-bad,RAW_PRIVATE_TEST_SENTINEL\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    assert_eq!(preview.rows.len(), 1);
    assert_eq!(preview.errors.len(), 1);
    let selected = vec![preview.rows[0].id.clone()];
    let ids = ledger
        .stage_preview(&preview.preview_id, &account, selected.clone(), vec![])
        .unwrap();
    assert_eq!(
        ledger
            .stage_preview(&preview.preview_id, &account, selected, vec![])
            .unwrap(),
        ids
    );
    assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "0.00");
    assert_eq!(
        ledger
            .day(&account, "2026-10-02")
            .unwrap()
            .balance
            .as_deref(),
        Some("100.00")
    );
    ledger.confirm_pending(ids.clone(), vec![]).unwrap();
    ledger.confirm_pending(ids, vec![]).unwrap();
    assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "12.30");
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "12.30");
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    assert_eq!(preview.exact_duplicates, 1);
    assert!(
        ledger
            .stage_preview(
                &preview.preview_id,
                &account,
                vec![preview.rows[0].id.clone()],
                vec![]
            )
            .is_err()
    );
    let persisted = std::fs::read(&path).unwrap();
    assert!(
        !persisted
            .windows("RAW_PRIVATE_TEST_SENTINEL".len())
            .any(|window| window == b"RAW_PRIVATE_TEST_SENTINEL")
    );
}
#[test]
fn preview_matches_manual_records_even_beyond_first_neighbor_page() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("manual-preview.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    // Earlier same-day target sorts behind over one full page of later rows.
    ledger
        .save_entry(&account, "2026-10-02", "12.30", "manual lunch", false)
        .unwrap();
    for _ in 0..205 {
        ledger
            .save_entry(&account, "2026-10-03", "1.00", "other", false)
            .unwrap();
    }
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 13:00:00,消费,商户,午餐,支出,12.30,manual-later-import\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    assert_eq!(preview.new_count, 0);
    assert_eq!(preview.exact_duplicates, 0);
    assert_eq!(preview.suspected_duplicates, 1);
    assert!(preview.rows[0].duplicate_id.is_some());
}

#[test]
fn imported_refund_requires_explicit_parent_at_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("refund.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02,退款,商户,退款,收入,5,w-refund\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    let ids = ledger
        .stage_preview(
            &preview.preview_id,
            &account,
            vec![preview.rows[0].id.clone()],
            vec![],
        )
        .unwrap();
    ledger
        .edit_pending(PendingEdit {
            destination_id: None,
            id: ids[0].clone(),
            expected_version: 0,
            account_id: account.clone(),
            date: "2026-10-02".into(),
            amount: "4.50".into(),
            kind: "refund".into(),
            merchant: "corrected merchant".into(),
            description: "corrected refund".into(),
            category_id: None,
            original_transaction_id: None,
        })
        .unwrap();
    assert!(ledger.confirm_pending(ids.clone(), vec![]).is_err());
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(transaction_id(&ids[0]).unwrap())
            .unwrap()
            .unwrap()
            .status,
        TransactionStatus::Pending
    );
}
