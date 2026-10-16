use chrono::Utc;
use ledger_app::LedgerRepository;
use ledger_domain::*;
use ledger_export::{ExportFormat, ExportService};
use ledger_storage::sqlite::SqliteLedgerRepository;
fn money(n: i64) -> Money {
    Money::new(n, CurrencyCode::new("CNY", 2).unwrap())
}
#[test]
fn exports_exact_money_refunds_and_reversals_and_defaults_to_confirmed() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let account = Account::new("钱包", AccountKind::Asset, money(0).currency().clone());
    repo.insert_account(account.clone()).unwrap();
    let expense = Transaction {
        id: TransactionId::new(),
        occurred_at: Utc::now(),
        kind: TransactionKind::Expense,
        status: TransactionStatus::Confirmed,
        postings: vec![Posting {
            account_id: account.id,
            delta: money(-9007199254740993),
        }],
        category_id: None,
        original_transaction_id: None,
        report_amount_cny: Some(money(9007199254740993)),
        merchant: Some(" =1+1".into()),
        description: Some("含逗号,换行\n和\"引号\"".into()),
    };
    repo.insert_transaction(expense.clone()).unwrap();
    let mut refund = expense.clone();
    refund.id = TransactionId::new();
    refund.kind = TransactionKind::Refund;
    refund.original_transaction_id = Some(expense.id);
    refund.report_amount_cny = Some(money(120));
    refund.postings[0].delta = money(120);
    repo.insert_transaction(refund.clone()).unwrap();
    let reversal = repo
        .reverse_with_reason(refund.id, Utc::now(), "撤销退款")
        .unwrap();
    let mut pending = expense.clone();
    pending.id = TransactionId::new();
    pending.status = TransactionStatus::Pending;
    repo.insert_transaction(pending).unwrap();
    let before = repo.snapshot().unwrap().fingerprint().unwrap();
    let (bytes, count) =
        ExportService::export(&repo, PendingQuery::default(), false, ExportFormat::Json).unwrap();
    assert_eq!(count, 3);
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let rows = doc["transactions"].as_array().unwrap();
    let original = rows
        .iter()
        .find(|r| r["id"] == expense.id.as_uuid().to_string())
        .unwrap();
    assert_eq!(original["amount_cny"], "90071992547409.93");
    let r = rows
        .iter()
        .find(|r| r["id"] == refund.id.as_uuid().to_string())
        .unwrap();
    assert_eq!(r["expense_cny"], "-1.20");
    assert_eq!(
        r["original_transaction_id"],
        expense.id.as_uuid().to_string()
    );
    let r = rows
        .iter()
        .find(|r| r["id"] == reversal.as_uuid().to_string())
        .unwrap();
    assert_eq!(r["expense_cny"], "1.20");
    assert_eq!(r["reversal_of"], refund.id.as_uuid().to_string());
    let (csv, count) = ExportService::export(
        &repo,
        PendingQuery::default(),
        true,
        ExportFormat::Csv { bom: true },
    )
    .unwrap();
    assert_eq!(count, 4);
    assert!(csv.starts_with(&[239, 187, 191]));
    let mut reader = csv::Reader::from_reader(&csv[3..]);
    let records = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 4);
    let row = records
        .iter()
        .find(|r| r.get(0) == Some(&expense.id.as_uuid().to_string()))
        .unwrap();
    assert_eq!(&row[4], "90071992547409.93");
    assert_eq!(&row[7], "' =1+1");
    assert_eq!(&row[8], "含逗号,换行\n和\"引号\"");
    let (_, count) = ExportService::export(
        &repo,
        PendingQuery {
            merchant: Some("不存在".into()),
            ..Default::default()
        },
        true,
        ExportFormat::Json,
    )
    .unwrap();
    assert_eq!(count, 0);
    assert_eq!(repo.snapshot().unwrap().fingerprint().unwrap(), before);
}
