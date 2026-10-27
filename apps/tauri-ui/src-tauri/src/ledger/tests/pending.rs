use super::super::*;

fn stage_confirm_all_fixture(ledger: &Ledger, account: &str) -> Vec<String> {
    let mut csv =
        String::from("交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,原交易单号\n");
    for index in 0..205 {
        csv.push_str(&format!(
            "2026-10-02,消费,商户,消费,支出,1.00,bulk-{index},\n"
        ));
    }
    csv.push_str("2026-10-01,消费,商户,原消费,支出,10.00,bulk-parent,\n2026-10-03,退款,商户,退款,收入,3.00,bulk-refund,bulk-parent\n2026-09-30,消费,商户,周期外,支出,1.00,outside,\n2026-10-04,消费,商户,忽略,支出,1.00,ignored,\n2026-10-05,消费,商户,已确认,支出,1.00,confirmed,\n");
    let preview = ledger.bill_preview("wechat", csv.into_bytes()).unwrap();
    ledger
        .stage_preview(
            &preview.preview_id,
            account,
            preview.rows.iter().map(|row| row.id.clone()).collect(),
            vec![],
        )
        .unwrap()
}

#[test]
fn confirm_all_pending_crosses_pages_preserves_filters_and_links_refunds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("confirm-all.sqlite");
    let ledger = Ledger::open(&path).unwrap();
    let accounts = ledger.accounts().unwrap();
    let account = &accounts[0].id;
    let ids = stage_confirm_all_fixture(&ledger, account);
    ignore_pending(&ledger, vec![ids[208].clone()]).unwrap();
    ledger
        .confirm_pending(vec![ids[209].clone()], vec![])
        .unwrap();
    let mut excluded = vec![ids[207].clone(), ids[208].clone()];
    for (provider, platform, order) in [
        ("alipay", account, "other-source"),
        ("wechat", &accounts[1].id, "other-platform"),
    ] {
        let csv = format!(
            "交易时间,交易类型,交易对方,商品,商品说明,收/支,金额(元),交易金额,交易单号,交易订单号\n2026-10-02,消费,商户,消费,消费,支出,1.00,1.00,{order},{order}\n"
        );
        let preview = ledger.bill_preview(provider, csv.into_bytes()).unwrap();
        excluded.extend(
            ledger
                .stage_preview(
                    &preview.preview_id,
                    platform,
                    vec![preview.rows[0].id.clone()],
                    vec![],
                )
                .unwrap(),
        );
    }
    let query = || {
        serde_json::from_value(serde_json::json!({
            "status": "pending", "source": "wechat", "account_id": account,
            "from_date": "2026-10-01", "until_date": "2026-11-01",
            "offset": 20, "limit": 1
        }))
        .unwrap()
    };
    // The selected-IDs API remains bounded; confirm-all ignores pagination only.
    assert!(ledger.confirm_pending(ids[..207].to_vec(), vec![]).is_err());
    assert_eq!(ledger.confirm_all_pending(query()).unwrap(), 207);
    assert_eq!(ledger.confirm_all_pending(query()).unwrap(), 0);
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let repo = ledger.repo().unwrap();
    for id in &ids[..207] {
        assert_eq!(
            repo.get_transaction(transaction_id(id).unwrap())
                .unwrap()
                .unwrap()
                .status,
            TransactionStatus::Confirmed
        );
    }
    for (index, id) in excluded.iter().enumerate() {
        assert_eq!(
            repo.get_transaction(transaction_id(id).unwrap())
                .unwrap()
                .unwrap()
                .status,
            if index == 1 {
                TransactionStatus::Ignored
            } else {
                TransactionStatus::Pending
            }
        );
    }
    let refund = repo
        .get_transaction(transaction_id(&ids[206]).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        refund.original_transaction_id,
        Some(transaction_id(&ids[205]).unwrap())
    );
    assert_eq!(ledger.day(account, "2026-10-02").unwrap().expense, "205.00");
    assert_eq!(ledger.day(account, "2026-10-03").unwrap().expense, "-3.00");
    assert!(
        ledger
            .confirm_all_pending(
                serde_json::from_value(serde_json::json!({
                    "status": "ignored", "offset": 0, "limit": 20
                }))
                .unwrap()
            )
            .is_err()
    );
}

#[test]
fn confirm_all_pending_rolls_back_every_page_when_a_refund_parent_is_outside_filter() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("confirm-all-rollback.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let ids = stage_confirm_all_fixture(&ledger, &account);
    ledger
        .edit_pending(PendingEdit {
            id: ids[206].clone(),
            expected_version: 0,
            account_id: account.clone(),
            destination_id: None,
            date: "2026-10-03".into(),
            amount: "3.00".into(),
            kind: "refund".into(),
            merchant: "商户".into(),
            description: "退款".into(),
            category_id: None,
            original_transaction_id: Some(ids[205].clone()),
        })
        .unwrap();
    assert!(
        ledger
            .confirm_all_pending(
                serde_json::from_value(serde_json::json!({
                    "status": "pending", "from_date": "2026-10-02", "until_date": "2026-10-04",
                    "offset": 0, "limit": 20
                }))
                .unwrap()
            )
            .is_err()
    );
    assert!(
        ledger
            .repo()
            .unwrap()
            .list_transactions()
            .unwrap()
            .iter()
            .all(|tx| tx.status == TransactionStatus::Pending)
    );
    assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "0.00");
}

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
    TransactionService::new(ledger.repo().unwrap())
        .confirm(Transaction {
            id: TransactionId::new(),
            occurred_at: time(date("2026-10-02").unwrap(), false)
                .unwrap()
                .with_timezone(&Utc),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            postings: vec![Posting {
                account_id: account_id(&account).unwrap(),
                delta: money(-1230),
            }],
            category_id: None,
            original_transaction_id: None,
            report_amount_cny: Some(money(1230)),
            merchant: Some("商户".into()),
            description: Some("manual lunch".into()),
        })
        .unwrap();
    for _ in 0..205 {
        ledger
            .save_entry(&account, "2026-10-03", "1.00", "other", false)
            .unwrap();
    }
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 12:03:00,消费,商户,午餐,支出,12.30,manual-later-import\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    assert_eq!(preview.new_count, 0);
    assert_eq!(preview.exact_duplicates, 0);
    assert_eq!(preview.suspected_duplicates, 1);
    assert!(preview.rows[0].duplicate_id.is_some());
}

#[test]
fn saved_order_ids_exclude_other_orders_from_fuzzy_matching() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("distinct-orders.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let bill = |id: &str| {
        format!(
            "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02 12:00:00,消费,商户,午餐,支出,15.00,{id}\n"
        )
        .into_bytes()
    };
    let preview = ledger.bill_preview("wechat", bill("order-1")).unwrap();
    ledger
        .stage_preview(
            &preview.preview_id,
            &account,
            vec![preview.rows[0].id.clone()],
            vec![],
        )
        .unwrap();
    let next = ledger.bill_preview("wechat", bill("order-2")).unwrap();
    assert_eq!(next.new_count, 1);
    assert_eq!(next.suspected_duplicates, 0);
    let same = ledger.bill_preview("wechat", bill("order-1")).unwrap();
    assert_eq!(same.exact_duplicates, 1);

    let page = ledger
        .page(serde_json::from_value(serde_json::json!({"offset": 0, "limit": 20})).unwrap())
        .unwrap_or_else(|error| panic!("{}", error.message));
    let page = serde_json::to_value(page).unwrap();
    assert_eq!(
        page["items"][0]["external_ids"],
        serde_json::json!([{"source": "wechat", "id": "order-1"}])
    );
}

#[test]
fn imported_refund_can_be_edited_and_confirmed_without_a_parent() {
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
    ledger.confirm_pending(ids.clone(), vec![]).unwrap();
    let transaction = ledger
        .repo()
        .unwrap()
        .get_transaction(transaction_id(&ids[0]).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(transaction.status, TransactionStatus::Confirmed);
    assert_eq!(transaction.original_transaction_id, None);
    let day = ledger.day(&account, "2026-10-02").unwrap();
    assert_eq!(day.expense, "-4.50");
    assert_eq!(day.income, "0.00");
}

#[test]
fn unlinked_refund_can_be_confirmed_with_expenses_in_any_selection_order() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("refund-batch.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02,消费,商户,午餐,支出,12.30,original-expense\n2026-10-02,退款,商户,部分退款,收入,3.00,partial-refund\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    let mut ids = ledger
        .stage_preview(
            &preview.preview_id,
            &account,
            preview.rows.iter().map(|row| row.id.clone()).collect(),
            vec![],
        )
        .unwrap();
    ids.reverse();
    ledger.confirm_pending(ids.clone(), vec![]).unwrap();
    ledger.confirm_pending(ids.clone(), vec![]).unwrap();
    let refund = ledger
        .repo()
        .unwrap()
        .get_transaction(transaction_id(&ids[0]).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(refund.status, TransactionStatus::Confirmed);
    assert_eq!(
        refund.original_transaction_id, None,
        "merchant similarity is only a suggestion"
    );
    assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "9.30");
}

#[test]
fn refund_order_references_survive_reopen_and_link_in_reverse_selection_order() {
    for provider in ["wechat", "alipay"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("refund-reference.sqlite");
        let ledger = Ledger::open(&path).unwrap();
        let account = ledger.accounts().unwrap()[0].id.clone();
        let raw = "交易时间,交易类型,交易对方,商品,商品说明,收/支,金额(元),交易金额,交易单号,交易订单号,原交易单号\n2026-10-01,消费,商户,午餐,午餐,支出,12.30,12.30,order-1,order-1,\n2026-10-02,退款,商户,部分退款,部分退款,收入,3.00,3.00,refund-1,refund-1,order-1\n";
        let preview = ledger
            .bill_preview(provider, raw.as_bytes().to_vec())
            .unwrap();
        let mut ids = ledger
            .stage_preview(
                &preview.preview_id,
                &account,
                preview.rows.iter().map(|row| row.id.clone()).collect(),
                vec![],
            )
            .unwrap();
        let original = ids[0].clone();
        let refund_id = ids[1].clone();
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        let page = serde_json::to_value(
            ledger
                .page(
                    serde_json::from_value(
                        serde_json::json!({"status":"pending","offset":0,"limit":20}),
                    )
                    .unwrap(),
                )
                .unwrap_or_else(|error| panic!("{}", error.message)),
        )
        .unwrap();
        let refund = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == refund_id)
            .unwrap();
        assert_eq!(refund["refund_matches"][0]["id"], original);
        assert_eq!(refund["refund_matches"][0]["confidence"], "exact");
        ids.reverse();
        ledger.confirm_pending(ids.clone(), vec![]).unwrap();
        ledger.confirm_pending(ids, vec![]).unwrap();
        let refund = ledger
            .repo()
            .unwrap()
            .get_transaction(transaction_id(&refund_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            refund.original_transaction_id,
            Some(transaction_id(&original).unwrap())
        );
        assert_eq!(ledger.day(&account, "2026-10-02").unwrap().expense, "-3.00");
        // Backups must retain provider order references as text, not UUIDs.
        let restored =
            SqliteLedgerRepository::from_snapshot(&ledger.repo().unwrap().snapshot().unwrap())
                .unwrap();
        use ledger_app::FeatureRepository;
        assert_eq!(
            restored.source_evidence(refund.id).unwrap()[0]
                .original_external_transaction_id
                .as_deref(),
            Some("order-1")
        );
    }
}

#[test]
fn matching_merchant_is_a_suggestion_and_explicitly_cleared_links_stay_empty() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("suggest-refund.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,原交易单号\n2026-10-01,消费,商户,午餐,支出,12.30,order-1,\n2026-10-02,退款,商户,部分退款,收入,3.00,refund-1,\n2026-10-03,退款,商户,部分退款,收入,2.00,refund-2,order-1\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    let ids = ledger
        .stage_preview(
            &preview.preview_id,
            &account,
            preview.rows.iter().map(|row| row.id.clone()).collect(),
            vec![],
        )
        .unwrap();
    ledger
        .confirm_pending(vec![ids[0].clone()], vec![])
        .unwrap();
    let page = serde_json::to_value(
        ledger
            .page(
                serde_json::from_value(
                    serde_json::json!({"status":"pending","offset":0,"limit":20}),
                )
                .unwrap(),
            )
            .unwrap_or_else(|error| panic!("{}", error.message)),
    )
    .unwrap();
    let suggestion = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == ids[1])
        .unwrap();
    assert_eq!(suggestion["refund_matches"][0]["id"], ids[0]);
    assert_eq!(suggestion["refund_matches"][0]["confidence"], "suggested");
    ledger
        .confirm_pending(
            vec![ids[1].clone(), ids[2].clone()],
            vec![PendingEdit {
                id: ids[2].clone(),
                expected_version: 0,
                account_id: account,
                destination_id: None,
                date: "2026-10-03".into(),
                amount: "2.00".into(),
                kind: "refund".into(),
                merchant: "商户".into(),
                description: "部分退款".into(),
                category_id: None,
                original_transaction_id: None,
            }],
        )
        .unwrap();
    for id in &ids[1..] {
        assert_eq!(
            ledger
                .repo()
                .unwrap()
                .get_transaction(transaction_id(id).unwrap())
                .unwrap()
                .unwrap()
                .original_transaction_id,
            None
        );
    }
}

#[test]
fn refund_matching_excludes_wrong_platform_provider_future_reversed_and_excess_amounts() {
    for case in ["platform", "provider", "future", "reversed", "excess"] {
        let directory = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(&directory.path().join("guard.sqlite")).unwrap();
        let accounts = ledger.accounts().unwrap();
        let account = accounts[0].id.clone();
        let refund_account = if case == "platform" {
            &accounts[1].id
        } else {
            &account
        };
        let expense_date = if case == "future" {
            "2026-10-03"
        } else {
            "2026-10-01"
        };
        ledger
            .save_entry(&account, expense_date, "10.00", "商户", false)
            .unwrap();
        let original = ledger.day(&account, expense_date).unwrap().entries[0]
            .id
            .clone();
        let batch = uuid::Uuid::new_v4();
        ledger.repo().unwrap().create_import_batch(batch).unwrap();
        ledger
            .repo()
            .unwrap()
            .insert_source_evidence(
                transaction_id(&original).unwrap(),
                ledger_domain::SourceEvidence {
                    source_kind: if case == "provider" {
                        ledger_domain::SourceKind::Alipay
                    } else {
                        ledger_domain::SourceKind::WeChat
                    },
                    external_transaction_id: Some("order-1".into()),
                    original_external_transaction_id: None,
                    import_batch_id: batch,
                    source_fields_digest: [1; 32],
                },
            )
            .unwrap();
        if case == "reversed" {
            ledger.reverse_transaction(&original, "撤销测试").unwrap();
        }
        let amount = if case == "excess" { "11.00" } else { "3.00" };
        let raw = format!(
            "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,原交易单号\n2026-10-02,退款,不同商户,退款,收入,{amount},refund-1,order-1\n"
        );
        let preview = ledger.bill_preview("wechat", raw.into_bytes()).unwrap();
        let ids = ledger
            .stage_preview(
                &preview.preview_id,
                refund_account,
                vec![preview.rows[0].id.clone()],
                vec![],
            )
            .unwrap();
        let refund = ledger
            .repo()
            .unwrap()
            .get_transaction(transaction_id(&ids[0]).unwrap())
            .unwrap()
            .unwrap();
        let matcher_repo = ledger.repo().unwrap();
        let matcher = super::super::refunds::RefundMatcher::load(&matcher_repo, &[]).unwrap();
        assert!(matcher.matches(&refund).unwrap().is_empty(), "{case}");
        ledger.confirm_pending(ids.clone(), vec![]).unwrap();
        assert_eq!(
            ledger
                .repo()
                .unwrap()
                .get_transaction(transaction_id(&ids[0]).unwrap())
                .unwrap()
                .unwrap()
                .original_transaction_id,
            None,
            "{case}"
        );
    }
}

#[test]
fn automatic_links_respect_remaining_amount_and_do_not_confirm_unselected_parents() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("remaining.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号,原交易单号\n2026-10-01,消费,商户,午餐,支出,10.00,order-1,\n2026-10-02,退款,商户,退款,收入,7.00,refund-1,order-1\n2026-10-03,退款,商户,退款,收入,5.00,refund-2,order-1\n2026-10-04,退款,商户,退款,收入,2.00,refund-3,order-1\n";
    let preview = ledger
        .bill_preview("wechat", raw.as_bytes().to_vec())
        .unwrap();
    let ids = ledger
        .stage_preview(
            &preview.preview_id,
            &account,
            preview.rows.iter().map(|row| row.id.clone()).collect(),
            vec![],
        )
        .unwrap();
    ledger
        .confirm_pending(vec![ids[3].clone()], vec![])
        .unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(transaction_id(&ids[3]).unwrap())
            .unwrap()
            .unwrap()
            .original_transaction_id,
        None
    );
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
    ledger
        .confirm_pending(vec![ids[2].clone(), ids[1].clone(), ids[0].clone()], vec![])
        .unwrap();
    let transactions = ledger.repo().unwrap().list_transactions().unwrap();
    let linked = transactions
        .iter()
        .filter(|tx| tx.kind == TransactionKind::Refund && tx.original_transaction_id.is_some())
        .collect::<Vec<_>>();
    assert_eq!(linked.len(), 1);
    assert_eq!(
        linked[0].report_amount_cny.as_ref().unwrap().minor_units(),
        500
    );
}

#[test]
fn reversed_expenses_are_marked_and_pending_refund_links_explain_rejection() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&directory.path().join("reversed-original.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    ledger
        .save_entry(&account, "2026-10-02", "12.30", "原消费", false)
        .unwrap();
    let expense = ledger.day(&account, "2026-10-02").unwrap().entries[0]
        .id
        .clone();
    let raw = "交易时间,交易类型,交易对方,商品,收/支,金额(元),交易单号\n2026-10-02,退款,商户,部分退款,收入,3.00,reversed-refund\n";
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
    let mut edit = PendingEdit {
        id: ids[0].clone(),
        expected_version: 0,
        account_id: account.clone(),
        destination_id: None,
        date: "2026-10-02".into(),
        amount: "3.00".into(),
        kind: "refund".into(),
        merchant: "商户".into(),
        description: "部分退款".into(),
        category_id: None,
        original_transaction_id: Some(expense.clone()),
    };
    ledger.edit_pending(edit.clone()).unwrap();
    let reversal = ledger.reverse_transaction(&expense, "重复记账").unwrap();
    let error = ledger.confirm_pending(ids, vec![]).unwrap_err();
    assert!(
        error.contains("原消费") && error.contains("撤销"),
        "{error}"
    );
    edit.expected_version = 1;
    let error = ledger.edit_pending(edit).unwrap_err();
    assert!(
        error.contains("原消费") && error.contains("撤销"),
        "{error}"
    );
    let page = ledger
        .page(
            serde_json::from_value(
                serde_json::json!({"status": "confirmed", "offset": 0, "limit": 20}),
            )
            .unwrap(),
        )
        .unwrap_or_else(|error| panic!("{}", error.message));
    let page = serde_json::to_value(page).unwrap();
    let original = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == expense)
        .unwrap();
    assert_eq!(original["reversal_id"], reversal);
    assert_eq!(original["reversal_of"], serde_json::Value::Null);
}
