use super::*;

fn request(account: &str, kind: &str, amount: &str) -> WriteTransaction {
    WriteTransaction {
        edit: PendingEdit {
            destination_id: None,
            id: String::new(),
            expected_version: 0,
            account_id: account.into(),
            date: "2026-10-14".into(),
            amount: amount.into(),
            kind: kind.into(),
            merchant: "测试商户".into(),
            description: "阶段二测试".into(),
            category_id: None,
            original_transaction_id: None,
        },
        destination_id: None,
        source: None,
        transaction_number: None,
        merchant_order_id: None,
        time: None,
        manual_fx_rate: None,
    }
}

#[test]
fn manual_order_ids_and_optional_note_survive_restart_and_prevent_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manual-orders.sqlite");
    let ledger = Ledger::open(&path).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let mut manual = request(&account, "expense", "6.64");
    manual.edit.merchant = "饭小侠".into();
    manual.edit.description.clear();
    manual.source = Some("wechat".into());
    manual.transaction_number = Some("00001234".into());
    manual.merchant_order_id = Some("00005678".into());
    manual.time = Some("17:26:28".into());
    let id = ledger.write_transaction(manual).unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    let detail = ledger.transaction_detail(&id).unwrap();
    assert!(detail.description.is_empty());
    assert_eq!(detail.time, "17:26:28");
    assert_eq!(detail.sources[0].external_id.as_deref(), Some("00001234"));
    assert_eq!(
        detail.sources[0].merchant_order_id.as_deref(),
        Some("00005678")
    );
    let bill = |number: &str| {
        format!("交易时间,交易对方,商品,收/支,金额(元),交易单号,商户单号\n2026-10-14 17:26:28,美团,饭小侠-美团App-测试,支出,6.64,{number},00005678\n").into_bytes()
    };
    let preview = ledger
        .bill_preview_for_account("wechat", bill("00001234"), Some(&account))
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
    let different = ledger
        .bill_preview_for_account("wechat", bill("00009999"), Some(&account))
        .unwrap();
    assert_eq!(different.new_count, 1);
    let mut duplicate = request(&account, "expense", "6.64");
    duplicate.source = Some("wechat".into());
    duplicate.transaction_number = Some("00001234".into());
    assert!(ledger.write_transaction(duplicate).is_err());
    assert_eq!(ledger.repo().unwrap().list_transactions().unwrap().len(), 1);
}

#[test]
fn cached_foreign_rate_without_attribution_requires_manual_fallback() {
    use ledger_storage::sqlite::ExchangeRateObservation;

    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("unattributed-fx.sqlite3")).unwrap();
    let usd = ledger
        .create_platform_currency("无来源美元", "bank", Some("USD"))
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    ledger
        .repo()
        .unwrap()
        .upsert_exchange_rates(&[ExchangeRateObservation {
            effective_date: date,
            base_currency: "CNY".into(),
            quote_currency: "USD".into(),
            rate: "7.5".into(),
            provider: "frankfurter".into(),
            provider_sources: vec![],
            fetched_at_utc: chrono::Utc::now(),
        }])
        .unwrap();

    let unavailable = ledger.preview_foreign_transaction("10.00", "USD", "2026-10-14", None);
    assert!(matches!(unavailable, Err(message) if message.contains("可用本地汇率")));

    let manual = ledger
        .preview_foreign_transaction("10.00", "USD", "2026-10-14", Some("7.5"))
        .unwrap();
    assert_eq!(manual.source, "manual");
    assert_eq!(manual.report_amount_cny, "1.33");

    let mut write = request(&usd.id, "expense", "10.00");
    write.manual_fx_rate = Some("7.5".into());
    let id = ledger.write_transaction(write).unwrap();
    let detail = ledger.transaction_detail(&id).unwrap();
    assert_eq!(detail.report_amount_cny.as_deref(), Some("1.33"));
    assert_eq!(detail.fx_snapshot.unwrap().source, "manual");
}

#[test]
fn manual_shop_matches_bill_product_without_note() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("manual-aux.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let mut manual = request(&account, "expense", "6.64");
    manual.edit.merchant = "饭小侠".into();
    manual.edit.description.clear();
    manual.time = Some("17:27:30".into());
    ledger.write_transaction(manual).unwrap();
    let csv = "交易时间,交易对方,商品,收/支,金额(元),交易单号,商户单号\n2026-10-14 17:26:28,美团,饭小侠-美团App-测试,支出,6.64,00123,00456\n";
    let preview = ledger
        .bill_preview_for_account("wechat", csv.as_bytes().to_vec(), Some(&account))
        .unwrap();
    assert_eq!(preview.suspected_duplicates, 1);
    assert_eq!(preview.new_count, 0);
    let other = ledger.accounts().unwrap()[1].id.clone();
    assert_eq!(
        ledger
            .bill_preview_for_account("wechat", csv.as_bytes().to_vec(), Some(&other))
            .unwrap()
            .new_count,
        1
    );
}

#[test]
fn manual_merchant_order_without_channel_id_is_only_an_advisory_match() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("manual-only-merchant-id.sqlite")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let mut manual = request(&account, "expense", "6.64");
    manual.edit.description.clear();
    manual.source = Some("wechat".into());
    manual.merchant_order_id = Some("00005678".into());
    manual.time = Some("17:26:28".into());
    ledger.write_transaction(manual).unwrap();
    for order in ["", "00005678"] {
        let csv = format!(
            "交易时间,交易对方,收/支,金额(元),商户单号\n2026-10-14 17:26:28,测试商户,支出,6.64,{order}\n"
        );
        let preview = ledger
            .bill_preview_for_account("wechat", csv.into_bytes(), Some(&account))
            .unwrap();
        assert_eq!(preview.exact_duplicates, 0);
        assert_eq!(preview.suspected_duplicates, 1);
    }
}

#[test]
fn new_transactions_auto_classify_known_unknown_and_custom_merchants() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap()[0].id.clone();
    let categories = ledger.categories().unwrap();
    let drinks = categories
        .iter()
        .find(|c| c.name == "饮品")
        .unwrap()
        .id
        .clone();
    let other = categories
        .iter()
        .find(|c| c.name == "其他")
        .unwrap()
        .id
        .clone();
    let mut known = request(&account, "expense", "12.00");
    known.edit.merchant = "喜茶中心店".into();
    let known_id = ledger.write_transaction(known).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&known_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        drinks
    );
    assert_eq!(
        ledger.day(&account, "2026-10-14").unwrap().entries[0]
            .category_name
            .as_deref(),
        Some("饮品")
    );

    let mut unknown = request(&account, "expense", "8.00");
    unknown.edit.merchant = "小众小店".into();
    let unknown_id = ledger.write_transaction(unknown).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&unknown_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        other
    );

    ledger
        .write_rule(crate::search_rules::RuleInput {
            id: None,
            priority: 0,
            merchant_contains: Some("小众小店".into()),
            description_contains: None,
            category_id: drinks.clone(),
            enabled: true,
        })
        .unwrap();
    let mut custom = request(&account, "expense", "9.00");
    custom.edit.merchant = "小众小店二号".into();
    let custom_id = ledger.write_transaction(custom).unwrap();
    assert_eq!(
        ledger
            .repo()
            .unwrap()
            .get_transaction(ledger::transaction_id(&custom_id).unwrap())
            .unwrap()
            .unwrap()
            .category_id
            .unwrap()
            .as_uuid()
            .to_string(),
        drinks
    );
}

#[test]
fn all_platform_day_totals_count_refunds_and_transfers_once() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let accounts = ledger.accounts().unwrap();
    let source = &accounts[0].id;
    let destination = &accounts[1].id;
    ledger
        .write_transaction(request(source, "income", "10.10"))
        .unwrap();
    ledger
        .write_transaction(request(destination, "income", "20.20"))
        .unwrap();
    let expense = ledger
        .write_transaction(request(source, "expense", "15.10"))
        .unwrap();
    let mut refund = request(source, "refund", "3.00");
    refund.edit.original_transaction_id = Some(expense);
    ledger.write_transaction(refund).unwrap();
    let mut transfer = request(source, "transfer", "5.00");
    transfer.destination_id = Some(destination.clone());
    ledger.write_transaction(transfer).unwrap();
    ledger
        .save_entry(source, "2026-10-15", "99.99", "", true)
        .unwrap();
    let day = ledger.day_all("2026-10-14").unwrap();
    assert_eq!(
        (&*day.income, &*day.expense, &*day.net),
        ("30.30", "12.10", "18.20")
    );
    assert_eq!(day.entries.len(), 5);
    assert!(day.balance.is_none() && day.reference.is_none());
    assert_eq!(ledger.day(source, "2026-10-14").unwrap().income, "10.10");
    assert_eq!(ledger.day_all("2026-10-13").unwrap().net, "0.00");
}

#[test]
fn manual_transfer_and_refund_keep_links_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    let accounts = ledger.accounts().unwrap();
    let source = &accounts[0].id;
    let destination = &accounts[1].id;
    let expense = ledger
        .write_transaction(request(source, "expense", "25.00"))
        .unwrap();
    let mut refund = request(source, "refund", "5.00");
    refund.edit.original_transaction_id = Some(expense.clone());
    let refund_id = ledger.write_transaction(refund).unwrap();
    let mut transfer = request(source, "transfer", "10.00");
    transfer.destination_id = Some(destination.clone());
    let transfer_id = ledger.write_transaction(transfer).unwrap();
    drop(ledger);
    let ledger = Ledger::open(&path).unwrap();
    assert_eq!(
        ledger
            .transaction_detail(&refund_id)
            .unwrap()
            .original_transaction_id
            .as_deref(),
        Some(expense.as_str())
    );
    assert!(
        ledger
            .transaction_detail(&expense)
            .unwrap()
            .linked_ids
            .contains(&refund_id)
    );
    assert_eq!(
        ledger
            .transaction_detail(&transfer_id)
            .unwrap()
            .destination_id
            .as_deref(),
        Some(destination.as_str())
    );
    let transfer = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&transfer_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(transfer.postings.len(), 2);
    assert_eq!(
        transfer.postings[0].delta.minor_units() + transfer.postings[1].delta.minor_units(),
        0
    );
}

#[test]
fn confirmed_transaction_kind_and_refund_parent_are_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let expense = ledger
        .write_transaction(request(&account, "expense", "25.00"))
        .unwrap();
    let mut edit = request(&account, "income", "25.00");
    edit.edit.id = expense.clone();
    assert!(ledger.write_transaction(edit).is_err());
    let mut refund = request(&account, "refund", "5.00");
    refund.edit.original_transaction_id = Some(expense.clone());
    let refund_id = ledger.write_transaction(refund).unwrap();
    let mut edit = request(&account, "refund", "5.00");
    edit.edit.id = refund_id.clone();
    edit.edit.original_transaction_id = Some(expense);
    assert!(ledger.write_transaction(edit).is_ok());
    let mut change_parent = request(&account, "refund", "5.00");
    change_parent.edit.id = refund_id;
    assert!(ledger.write_transaction(change_parent).is_err());
}

#[test]
fn editing_income_updates_totals_and_rejects_stale_versions() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let id = ledger
        .write_transaction(request(&account, "income", "25.00"))
        .unwrap();
    let mut edit = request(&account, "income", "30.00");
    edit.edit.id = id.clone();
    ledger.write_transaction(edit).unwrap();
    assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
    let mut stale = request(&account, "income", "40.00");
    stale.edit.id = id;
    assert!(ledger.write_transaction(stale).is_err());
    assert_eq!(ledger.day(&account, "2026-10-14").unwrap().income, "30.00");
}

#[test]
fn reversal_updates_day_totals_and_rejects_new_refunds() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.accounts().unwrap().remove(0).id;
    let today = chrono::Local::now().date_naive().to_string();
    let mut expense = request(&account, "expense", "25.00");
    expense.edit.date = today.clone();
    let id = ledger.write_transaction(expense).unwrap();
    ledger.reverse_transaction(&id, "重复记账").unwrap();
    assert_eq!(ledger.day(&account, &today).unwrap().expense, "0.00");
    let mut refund = request(&account, "refund", "5.00");
    refund.edit.original_transaction_id = Some(id);
    assert!(ledger.write_transaction(refund).is_err());
}

#[test]
fn sunday_foreign_expense_uses_friday_rate_and_manual_fallback() {
    use ledger_storage::sqlite::ExchangeRateObservation;
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("fx.sqlite3")).unwrap();
    let account = ledger
        .create_platform_currency("美元", "bank", Some("USD"))
        .unwrap();
    let observation = |day: &str, rate: &str| ExchangeRateObservation {
        effective_date: chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap(),
        base_currency: "CNY".into(),
        quote_currency: "USD".into(),
        rate: rate.into(),
        provider: "frankfurter".into(),
        provider_sources: vec!["ECB".into()],
        fetched_at_utc: chrono::Utc::now(),
    };
    ledger
        .repo()
        .unwrap()
        .upsert_exchange_rates(&[
            observation("2026-10-09", "7"),
            observation("2026-10-12", "8"),
        ])
        .unwrap();
    let mut sunday = request(&account.id, "expense", "10.00");
    sunday.edit.date = "2026-10-11".into();
    let preview = ledger
        .preview_foreign_transaction("10.00", "USD", "2026-10-11", None)
        .unwrap();
    assert_eq!(preview.report_amount_cny, "1.43");
    assert_eq!(preview.fx_snapshot.effective_date, "2026-10-09");
    let id = ledger.write_transaction(sunday).unwrap();
    let detail = ledger.transaction_detail(&id).unwrap();
    assert_eq!(detail.amount, "10.00");
    assert_eq!(detail.currency_code, "USD");
    assert_eq!(detail.report_amount_cny.as_deref(), Some("1.43"));
    assert_eq!(detail.fx_snapshot.unwrap().effective_date, "2026-10-09");
    let mut earlier = request(&account.id, "expense", "10.00");
    earlier.edit.date = "2026-10-08".into();
    assert!(
        ledger
            .write_transaction(request_with_date(&account.id, "2026-10-08"))
            .is_err()
    );
    earlier.manual_fx_rate = Some("5".into());
    let id = ledger.write_transaction(earlier).unwrap();
    assert_eq!(
        ledger
            .transaction_detail(&id)
            .unwrap()
            .report_amount_cny
            .as_deref(),
        Some("2.00")
    );
}

fn request_with_date(account: &str, day: &str) -> WriteTransaction {
    let mut value = request(account, "expense", "10.00");
    value.edit.date = day.into();
    value
}

#[test]
fn foreign_refund_requires_same_currency_as_expense() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("refund-fx.sqlite3")).unwrap();
    let cny = ledger.accounts().unwrap()[0].id.clone();
    let usd = ledger
        .create_platform_currency("美元", "bank", Some("USD"))
        .unwrap();
    let original = ledger
        .write_transaction(request(&cny, "expense", "10.00"))
        .unwrap();
    let mut refund = request(&usd.id, "refund", "5.00");
    refund.edit.original_transaction_id = Some(original);
    refund.manual_fx_rate = Some("7".into());
    assert!(ledger.write_transaction(refund).is_err());
}

#[test]
fn transfer_rejects_mixed_account_currencies() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("transfer-fx.sqlite3")).unwrap();
    let cny = ledger.accounts().unwrap()[0].id.clone();
    let usd = ledger
        .create_platform_currency("美元", "bank", Some("USD"))
        .unwrap();
    let mut transfer = request(&cny, "transfer", "5.00");
    transfer.destination_id = Some(usd.id);
    assert!(ledger.write_transaction(transfer).is_err());
}

#[test]
fn bill_preview_rejects_foreign_account() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("import-fx.sqlite3")).unwrap();
    let usd = ledger
        .create_platform_currency("美元", "bank", Some("USD"))
        .unwrap();
    let csv = "交易时间,交易对方,商品,收/支,金额(元),交易单号,商户单号\n2026-10-14 12:00:00,商户,测试,支出,1.00,1,2\n";
    assert!(
        ledger
            .bill_preview_for_account("wechat", csv.as_bytes().to_vec(), Some(&usd.id))
            .is_err()
    );
}

#[test]
fn transfer_between_foreign_accounts_keeps_original_currency() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("transfer-usd.sqlite3")).unwrap();
    let source = ledger
        .create_platform_currency("美元甲", "bank", Some("USD"))
        .unwrap();
    let target = ledger
        .create_platform_currency("美元乙", "bank", Some("USD"))
        .unwrap();
    let mut transfer = request(&source.id, "transfer", "5.00");
    transfer.destination_id = Some(target.id);
    let id = ledger.write_transaction(transfer).unwrap();
    let detail = ledger.transaction_detail(&id).unwrap();
    assert_eq!(detail.currency_code, "USD");
    assert!(detail.report_amount_cny.is_none());
}

#[test]
fn metadata_edit_preserves_manual_snapshot_without_resubmitting_rate() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("edit-manual-fx.sqlite3")).unwrap();
    let usd = ledger
        .create_platform_currency("编辑美元", "bank", Some("USD"))
        .unwrap();
    let mut original = request(&usd.id, "expense", "10.00");
    original.manual_fx_rate = Some("5".into());
    let id = ledger.write_transaction(original).unwrap();
    let before = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    let mut edit = request(&usd.id, "expense", "10.00");
    edit.edit.id = id.clone();
    edit.edit.description = "只改备注".into();
    ledger.write_transaction(edit).unwrap();
    let after = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(after.description.as_deref(), Some("只改备注"));
    assert_eq!(after.report_amount_cny, before.report_amount_cny);
    assert_eq!(after.fx_snapshot, before.fx_snapshot);
}

#[test]
fn metadata_edit_keeps_reference_snapshot_after_rate_revision_but_amount_edit_revalues() {
    use ledger_storage::sqlite::ExchangeRateObservation;
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("edit-reference-fx.sqlite3")).unwrap();
    let usd = ledger
        .create_platform_currency("汇率美元", "bank", Some("USD"))
        .unwrap();
    let observation = |rate: &str, fetched_at_utc| ExchangeRateObservation {
        effective_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 13).unwrap(),
        base_currency: "CNY".into(),
        quote_currency: "USD".into(),
        rate: rate.into(),
        provider: "frankfurter".into(),
        provider_sources: vec!["ECB".into()],
        fetched_at_utc,
    };
    let now = chrono::Utc::now();
    ledger
        .repo()
        .unwrap()
        .upsert_exchange_rates(&[observation("5", now)])
        .unwrap();
    let id = ledger
        .write_transaction(request(&usd.id, "expense", "10.00"))
        .unwrap();
    let before = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    ledger
        .repo()
        .unwrap()
        .upsert_exchange_rates(&[observation("10", now + chrono::Duration::seconds(1))])
        .unwrap();
    let mut metadata = request(&usd.id, "expense", "10.00");
    metadata.edit.id = id.clone();
    metadata.edit.merchant = "新商户".into();
    ledger.write_transaction(metadata).unwrap();
    let unchanged = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.fx_snapshot, before.fx_snapshot);
    assert_eq!(unchanged.report_amount_cny, before.report_amount_cny);
    let mut amount = request(&usd.id, "expense", "20.00");
    amount.edit.id = id.clone();
    amount.edit.expected_version = 1;
    ledger.write_transaction(amount).unwrap();
    let changed = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(changed.report_amount_cny.unwrap().decimal_string(), "2.00");
    assert_eq!(changed.fx_snapshot.unwrap().rate, "10");
}

#[test]
fn changing_date_or_account_currency_recalculates_foreign_snapshot() {
    use ledger_storage::sqlite::ExchangeRateObservation;
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(&dir.path().join("edit-fx-identity.sqlite3")).unwrap();
    let usd = ledger
        .create_platform_currency("日期美元", "bank", Some("USD"))
        .unwrap();
    let jpy = ledger
        .create_platform_currency("日期日元", "bank", Some("JPY"))
        .unwrap();
    let mut initial = request(&usd.id, "expense", "10.00");
    initial.manual_fx_rate = Some("5".into());
    let id = ledger.write_transaction(initial).unwrap();
    ledger
        .repo()
        .unwrap()
        .upsert_exchange_rates(&[ExchangeRateObservation {
            effective_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            base_currency: "CNY".into(),
            quote_currency: "USD".into(),
            rate: "10".into(),
            provider: "frankfurter".into(),
            provider_sources: vec!["ECB".into()],
            fetched_at_utc: chrono::Utc::now(),
        }])
        .unwrap();
    let mut dated = request(&usd.id, "expense", "10.00");
    dated.edit.id = id.clone();
    dated.edit.date = "2026-10-15".into();
    ledger.write_transaction(dated).unwrap();
    let after_date = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        after_date.report_amount_cny.unwrap().decimal_string(),
        "1.00"
    );
    assert_eq!(
        after_date.fx_snapshot.unwrap().source,
        ledger_domain::FxSnapshotSource::Reference
    );
    let mut currency = request(&jpy.id, "expense", "100");
    currency.edit.id = id.clone();
    currency.edit.date = "2026-10-15".into();
    currency.edit.expected_version = 1;
    currency.manual_fx_rate = Some("200".into());
    ledger.write_transaction(currency).unwrap();
    let after_currency = ledger
        .repo()
        .unwrap()
        .get_transaction(ledger::transaction_id(&id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(after_currency.postings[0].delta.currency().code(), "JPY");
    assert_eq!(
        after_currency.report_amount_cny.unwrap().decimal_string(),
        "0.50"
    );
    assert_eq!(
        after_currency.fx_snapshot.unwrap().source,
        ledger_domain::FxSnapshotSource::Manual
    );
}
