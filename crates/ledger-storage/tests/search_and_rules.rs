use chrono::{DateTime, Utc};
use ledger_app::{
    ClassificationRule, ClassificationRuleRepository, FeatureRepository, LedgerRepository,
};
use ledger_domain::{
    Account, AccountId, AccountKind, Category, CategoryId, CurrencyCode, Money, PendingQuery,
    Posting, RuleId, Transaction, TransactionId, TransactionKind, TransactionStatus,
};
use ledger_storage::sqlite::SqliteLedgerRepository;
fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn setup() -> (SqliteLedgerRepository, AccountId) {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let id = AccountId::new();
    let c = CurrencyCode::new("CNY", 2).unwrap();
    repo.insert_account(Account {
        id,
        display_name: "现金".into(),
        kind: AccountKind::Asset,
        currency: c,
        opening_balance: None,
        opening_balance_effective_at: None,
    })
    .unwrap();
    (repo, id)
}
fn add(
    repo: &SqliteLedgerRepository,
    account: AccountId,
    merchant: &str,
    amount: i64,
    day: &str,
    status: TransactionStatus,
    category_id: Option<CategoryId>,
) -> TransactionId {
    let id = TransactionId::new();
    let c = CurrencyCode::new("CNY", 2).unwrap();
    repo.insert_transaction(Transaction {
        id,
        occurred_at: at(day),
        kind: TransactionKind::Expense,
        status,
        postings: vec![Posting {
            account_id: account,
            delta: Money::new(-amount, c.clone()),
        }],
        category_id,
        original_transaction_id: None,
        report_amount_cny: Some(Money::new(amount, c)),
        merchant: Some(merchant.into()),
        description: Some("午餐".into()),
    })
    .unwrap();
    id
}
#[test]
fn normalized_alias_amount_and_stable_paging_keep_raw_merchant() {
    let (repo, account) = setup();
    let first = add(
        &repo,
        account,
        "  星巴克  ",
        1250,
        "2026-10-01T10:00:00Z",
        TransactionStatus::Confirmed,
        None,
    );
    add(
        &repo,
        account,
        "STARBUCKS",
        2600,
        "2026-10-02T10:00:00Z",
        TransactionStatus::Confirmed,
        None,
    );
    add(
        &repo,
        account,
        "无关",
        500,
        "2026-10-03T10:00:00Z",
        TransactionStatus::Confirmed,
        None,
    );
    repo.save_merchant_alias("星巴克", "starbucks").unwrap();
    let q = PendingQuery {
        keyword: Some(" STARBUCKS ".into()),
        min_amount_cny_minor: Some(1000),
        max_amount_cny_minor: Some(2000),
        limit: 1,
        ..Default::default()
    };
    let page = repo.query_transactions(q.clone()).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].transaction.id, first);
    assert_eq!(
        page.items[0].transaction.merchant.as_deref(),
        Some("  星巴克  ")
    );
    assert_eq!(
        repo.query_transactions(PendingQuery { offset: 1, ..q })
            .unwrap()
            .items
            .len(),
        0
    );
}
#[test]
fn rule_preview_respects_alias_priority_disabled_category_and_manual_override() {
    let (repo, account) = setup();
    let food = Category {
        id: CategoryId::new(),
        name: "餐饮".into(),
        enabled: true,
    };
    let other = Category {
        id: CategoryId::new(),
        name: "其他".into(),
        enabled: true,
    };
    repo.insert_category(food.clone()).unwrap();
    repo.insert_category(other.clone()).unwrap();
    let pending = add(
        &repo,
        account,
        "星巴克",
        1250,
        "2026-10-01T10:00:00Z",
        TransactionStatus::Pending,
        None,
    );
    add(
        &repo,
        account,
        "星巴克",
        1250,
        "2026-10-02T10:00:00Z",
        TransactionStatus::Confirmed,
        None,
    );
    add(
        &repo,
        account,
        "星巴克",
        1250,
        "2026-10-03T10:00:00Z",
        TransactionStatus::Pending,
        Some(other.id),
    );
    repo.save_merchant_alias("星巴克", "starbucks").unwrap();
    let rule = ClassificationRule {
        id: RuleId::new(),
        priority: 1,
        merchant_contains: Some("STARBUCKS".into()),
        description_contains: None,
        category_id: food.id,
        enabled: true,
    };
    let preview = repo.preview_rule(&rule).unwrap();
    assert_eq!(preview.match_count, 3);
    assert_eq!(preview.pending_ids, vec![pending]);
    repo.insert_rule(rule.clone()).unwrap();
    let lower = ClassificationRule {
        id: RuleId::new(),
        priority: 99,
        merchant_contains: Some("starbucks".into()),
        description_contains: None,
        category_id: other.id,
        enabled: true,
    };
    assert_eq!(repo.preview_rule(&lower).unwrap().pending_count, 0);
    let suggestion = repo.suggest_pending(pending).unwrap().unwrap();
    assert_eq!(suggestion.category_id, food.id);
    assert_eq!(suggestion.rule_id, rule.id);
    let second = ClassificationRule {
        id: RuleId::new(),
        priority: 0,
        merchant_contains: Some("star".into()),
        description_contains: None,
        category_id: other.id,
        enabled: true,
    };
    repo.insert_rule(second.clone()).unwrap();
    assert_eq!(
        repo.suggest_pending(pending).unwrap().unwrap().rule_id,
        second.id
    );
    repo.update_category(Category {
        enabled: false,
        ..other
    })
    .unwrap();
    assert_eq!(
        repo.suggest_pending(pending).unwrap().unwrap().rule_id,
        rule.id
    );
}
#[test]
fn search_treats_sql_metacharacters_as_data() {
    let (repo, account) = setup();
    add(
        &repo,
        account,
        "normal",
        100,
        "2026-10-01T10:00:00Z",
        TransactionStatus::Confirmed,
        None,
    );
    for s in ["%' OR 1=1 --", "_", "%"] {
        let page = repo
            .query_transactions(PendingQuery {
                keyword: Some(s.into()),
                limit: 20,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 0);
    }
}
#[test]
fn version_seven_data_migrates_without_changing_merchant_or_transaction_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0002_classification_rules.sql"),
        include_str!("../migrations/0003_optional_balance.sql"),
        include_str!("../migrations/0004_nine_features.sql"),
        include_str!("../migrations/0005_pending_import.sql"),
        include_str!("../migrations/0006_notifications.sql"),
        include_str!("../migrations/0007_recurring.sql"),
    ] {
        db.execute_batch(sql).unwrap()
    }
    for version in 1..=7 {
        db.execute("INSERT INTO schema_version VALUES(?1,0)", [version])
            .unwrap();
    }
    let account = AccountId::new();
    let id = TransactionId::new();
    db.execute(
        "INSERT INTO accounts VALUES(?1,'旧平台','asset','CNY',2,NULL,NULL)",
        [account.as_uuid().to_string()],
    )
    .unwrap();
    db.execute("INSERT INTO transactions(id,occurred_at_utc_micros,kind,status,report_amount_minor,report_currency_code,report_currency_exponent,merchant,description) VALUES(?1,1,'expense','confirmed',1234,'CNY',2,'  STAR   BUCKS  ','早餐')",[id.as_uuid().to_string()]).unwrap();
    db.execute(
        "INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros) VALUES(?1,0)",
        [id.as_uuid().to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO postings VALUES(?1,0,?2,-1234,'CNY',2)",
        rusqlite::params![id.as_uuid().to_string(), account.as_uuid().to_string()],
    )
    .unwrap();
    drop(db);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.schema_version().unwrap(), 9);
    let page = repo
        .query_transactions(PendingQuery {
            keyword: Some("star bucks".into()),
            limit: 20,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].transaction.id, id);
    assert_eq!(
        page.items[0].transaction.merchant.as_deref(),
        Some("  STAR   BUCKS  ")
    );
    drop(repo);
    let repo = SqliteLedgerRepository::open(&path).unwrap();
    assert_eq!(repo.schema_version().unwrap(), 9);
}
