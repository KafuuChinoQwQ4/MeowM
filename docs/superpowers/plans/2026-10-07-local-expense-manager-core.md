# Local Expense Manager Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a platform-independent Rust core that stores accounts and transactions locally and correctly calculates balances, classifications, and income/expense reports.

**Architecture:** A Cargo workspace separates domain types and invariants, application services, and SQLite persistence. Domain and application behavior stays independent of Android and Slint; later import, notification, and UI plans consume the public application interfaces defined here.

**Tech Stack:** Rust stable, Cargo workspace, SQLite through `rusqlite`, UUID-backed identifiers, SQL migrations, Rust unit and integration tests.

**Spec:** `docs/superpowers/specs/2026-10-07-local-expense-manager-design.md`

## Global Constraints

- “第一版不需要用户账号、服务端或云同步。”
- “账户间转账同时影响转出和转入账户，但不改变总收入/支出。”
- “信用卡消费增加应付余额并计入消费；偿还信用卡是资金账户与信用卡账户间的转账。”
- “退款与原交易关联，保留退款记录并在净支出中抵减，不删除原消费。”
- “所有金额使用定点十进制或最小货币单位整数保存，不使用二进制浮点数。”
- “待确认通知不改变正式余额或统计。”
- “图表只纳入已确认交易。”
- “多币种总览按交易保存的人民币报表金额计算；账户详情仍显示账户原币余额。”
- “数据迁移采用显式 schema 版本。”

---

## File Structure

- `Cargo.toml`: workspace membership and shared Rust package metadata.
- `crates/ledger-domain/`: currency, money, accounts, transactions, postings, categories, source evidence, and domain validation.
- `crates/ledger-app/`: account and transaction use cases, classification rules, and report queries; depends on domain and a storage port.
- `crates/ledger-storage/`: SQLite schema migrations and the storage-port implementation.
- `crates/ledger-app/tests/`: integration tests using an in-memory storage implementation for domain workflows.
- `crates/ledger-storage/tests/`: SQLite migration and repository behavior tests.

The exact public types created in Task 1 are the interfaces consumed by later tasks. Import parsers, Android services, Slint screens, encrypted backups, and app lock are separate plans and do not belong in these crates.

## Task 1: Create the Rust Workspace and Domain Crate

**Files:**
- Create: `Cargo.toml`
- Create: `crates/ledger-domain/Cargo.toml`
- Create: `crates/ledger-domain/src/lib.rs`
- Create: `crates/ledger-domain/src/money.rs`
- Create: `crates/ledger-domain/src/account.rs`
- Create: `crates/ledger-domain/src/transaction.rs`
- Create: `crates/ledger-domain/src/category.rs`
- Create: `crates/ledger-domain/src/source.rs`

**Interfaces:**
- Produces `CurrencyCode`, `Money`, `AccountId`, `Account`, `AccountKind`, `TransactionId`, `Transaction`, `TransactionKind`, `TransactionStatus`, `Posting`, `CategoryId`, `Category`, and `SourceEvidence` from `ledger_domain`.
- `Money::new(minor_units: i64, currency: CurrencyCode) -> Money` stores signed minor units and never uses `f32` or `f64`.
- `Transaction` contains an ID, occurrence time in UTC, kind, status, zero or more postings, optional category, optional original transaction ID, and optional report amount in CNY.
- `Account` contains an ID, display name, kind, currency, opening balance, and opening-balance effective time in UTC. IDs are UUID-backed newtypes.
- `Posting` contains an account ID and signed delta in that account's currency; asset deltas increase owned funds and liability deltas increase amounts owed.

- [ ] **Step 1: Write failing domain tests** in `crates/ledger-domain/src/money.rs`:

```rust
#[test]
fn money_keeps_minor_units_without_float_rounding() {
    let cny = CurrencyCode::new("CNY", 2).unwrap();
    let amount = Money::new(1234, cny);
    assert_eq!(amount.minor_units(), 1234);
    assert_eq!(amount.decimal_string(), "12.34");
}

#[test]
fn money_rejects_arithmetic_across_currency_codes() {
    let cny = Money::new(100, CurrencyCode::new("CNY", 2).unwrap());
    let usd = Money::new(100, CurrencyCode::new("USD", 2).unwrap());
    assert!(cny.checked_add(usd).is_err());
}
```

Also test JPY formatting as `1234`.
- [ ] **Step 2: Run `cargo test -p ledger-domain`** and verify the tests fail because the workspace package and types do not exist. Configure the initial root workspace with only `crates/ledger-domain`; later tasks add their members when their manifests are created.
- [ ] **Step 3: Implement the workspace and domain types.** `CurrencyCode::new(code: &str, minor_unit_exponent: u8) -> Result<CurrencyCode, DomainError>` validates three uppercase ASCII letters and an exponent from 0 through 9; `Money` stores an `i64` and its currency and exposes checked addition/subtraction and decimal formatting. Account kinds are `Asset` and `Liability`; transaction kinds are `Income`, `Expense`, `Transfer`, `Refund`, and `BalanceAdjustment`; statuses are `Pending`, `Confirmed`, and `Ignored`. A source evidence item stores source kind, optional external transaction ID, import batch ID, and a digest of source fields, never the notification body.
- [ ] **Step 4: Run `cargo test -p ledger-domain`** and verify the money tests pass. Add unit tests for currency validation, zero/negative amounts, posting currency consistency, and transaction-kind invariants.
- [ ] **Step 5: Commit** the workspace and domain crate with `git add Cargo.toml crates/ledger-domain && git commit -m "feat: add ledger domain types"`.

## Task 2: Implement Transaction Invariants and Report Effects

**Files:**
- Modify: `crates/ledger-domain/src/transaction.rs`
- Modify: `crates/ledger-domain/src/account.rs`
- Create: `crates/ledger-domain/src/validation.rs`
- Modify: `crates/ledger-domain/src/lib.rs`

**Interfaces:**
- Produces `validate_transaction_shape(transaction: &Transaction, accounts: &[Account]) -> Result<(), DomainError>`; it validates posting/account shape but leaves refund-parent lookup to the application service.
- Produces `ReportEffect { income_cny_minor: i64, expense_cny_minor: i64 }` and `report_effect(transaction: &Transaction) -> Option<ReportEffect>`.
- Asset account balances are positive when owned; liability account balances are positive when owed. Posting deltas follow that convention.

- [ ] **Step 1: Write failing tests** for the behavior below. The refund test checks the required parent ID here; the application-level test in Task 3 checks that the referenced stored transaction is actually an expense.

```rust
#[test]
fn asset_expense_has_negative_asset_posting_and_positive_expense_effect() {
    let wallet = asset_account("wallet", "CNY");
    let expense = confirmed_expense(wallet.clone(), 2500);
    validate_transaction_shape(&expense, &[wallet]).unwrap();
    assert_eq!(expense.postings[0].delta.minor_units(), -2500);
    assert_eq!(report_effect(&expense).unwrap().expense_cny_minor, 2500);
}

#[test]
fn transfer_has_two_postings_and_no_report_effect() {
    let cash = asset_account("cash", "CNY");
    let bank = asset_account("bank", "CNY");
    let transfer = confirmed_transfer(cash.clone(), bank.clone(), 5000);
    validate_transaction_shape(&transfer, &[cash, bank]).unwrap();
    assert_eq!(transfer.postings.len(), 2);
    assert_eq!(report_effect(&transfer), None);
}
```

Add corresponding tests for card purchase, card repayment, pending transactions, and refunds.
The test module defines fixture helpers with these signatures: `asset_account(name: &str, currency: &str) -> Account`, `confirmed_expense(account: Account, minor_units: i64) -> Transaction`, and `confirmed_transfer(from: Account, to: Account, minor_units: i64) -> Transaction`.
- [ ] **Step 2: Run `cargo test -p ledger-domain`** and verify the new tests fail on missing validation and report functions.
- [ ] **Step 3: Implement `validate_transaction_shape`.** Require at least one posting for confirmed financial transactions; ensure every posting references an existing account and uses that account's currency; require exactly two postings to distinct accounts for transfers; require refunds to include an original transaction ID. Pending and ignored transactions may have no postings. Verify a refund parent exists and is an expense in Task 3 before inserting it.
- [ ] **Step 4: Implement report effects and run `cargo test -p ledger-domain`.** Only confirmed income, expense, and refund transactions produce report effects. Transfers and balance adjustments produce none; refunds subtract from expense. Add tests for pending transactions and cross-currency transactions.
- [ ] **Step 5: Commit** with `git add crates/ledger-domain && git commit -m "feat: validate ledger transaction effects"`.

## Task 3: Add Application Ports and Ledger Use Cases

**Files:**
- Create: `crates/ledger-app/Cargo.toml`
- Create: `crates/ledger-app/src/lib.rs`
- Create: `crates/ledger-app/src/ports.rs`
- Create: `crates/ledger-app/src/accounts.rs`
- Create: `crates/ledger-app/src/transactions.rs`
- Create: `crates/ledger-app/tests/ledger_workflows.rs`

**Interfaces:**
- `LedgerRepository` exposes `insert_account`, `get_account`, `list_accounts`, `insert_transaction`, `get_transaction`, `list_transactions`, and `balance_as_of(account_id, cutoff_utc)` operations using domain types and `Result<_, RepositoryError>`.
- `AccountService::create_account(account: Account) -> Result<(), AppError>` and `AccountService::set_opening_balance(account_id, opening_balance: Money, effective_at_utc) -> Result<(), AppError>`.
- `TransactionService::confirm(transaction: Transaction) -> Result<TransactionId, AppError>`, `ignore(transaction_id: TransactionId) -> Result<(), AppError>`, and `account_balance(account_id: AccountId, at_utc) -> Result<Money, AppError>`.
- The application crate consumes a `LedgerRepository` implementation and does not depend on SQLite or Android.

- [ ] **Step 1: Write a failing integration test** with a small in-memory `LedgerRepository` fake:

```rust
#[test]
fn confirming_expense_changes_balance_once_and_only_after_its_time() {
    let (service, account_id) = service_with_account("CNY", 10_000);
    let expense = expense_for(account_id, 2_500, utc("2026-01-02T00:00:00Z"));
    service.confirm(expense.clone()).unwrap();
    service.confirm(expense).unwrap();

    assert_eq!(service.account_balance(account_id, utc("2026-01-01T00:00:00Z")).unwrap().minor_units(), 10_000);
    assert_eq!(service.account_balance(account_id, utc("2026-01-03T00:00:00Z")).unwrap().minor_units(), 7_500);
}
```

The test helpers construct domain values and the fake repository stores records by ID.
Define the fixture helpers in `tests/ledger_workflows.rs` as `utc(rfc3339: &str) -> Timestamp`, `service_with_account(currency: &str, opening_minor: i64) -> (TransactionService<InMemoryRepository>, AccountId)`, and `expense_for(account_id: AccountId, minor_units: i64, occurred_at: Timestamp) -> Transaction`. `TransactionService` is generic over the repository implementation so tests can inject the fake.
- [ ] **Step 2: Run `cargo test -p ledger-app --test ledger_workflows`** and verify compilation fails because the application crate and service APIs do not exist.
- [ ] **Step 3: Implement the repository port and use cases.** `confirm` first loads any refund parent and verifies it is a confirmed expense, validates transaction shape, changes status to `Confirmed`, and inserts it exactly once by transaction ID; a repeated ID with identical content is idempotent and a repeated ID with different content is an error. `ignore` changes only a pending transaction. Balance queries start with the account opening balance and add confirmed posting deltas whose occurrence time is after the opening-balance effective time and not later than the query cutoff.
- [ ] **Step 4: Run `cargo test -p ledger-app --test ledger_workflows`.** Add cases for a pending transaction not affecting balance, an ignored transaction not affecting balance, a post-cutoff transaction not affecting balance, a transfer affecting both accounts, and an invalid posting being rejected without repository writes.
- [ ] **Step 5: Commit** with `git add Cargo.toml crates/ledger-app && git commit -m "feat: add ledger application services"`.

## Task 4: Add SQLite Migrations and Repository

**Files:**
- Create: `crates/ledger-storage/Cargo.toml`
- Create: `crates/ledger-storage/src/lib.rs`
- Create: `crates/ledger-storage/src/sqlite.rs`
- Create: `crates/ledger-storage/migrations/0001_initial.sql`
- Create: `crates/ledger-storage/tests/sqlite_repository.rs`

**Interfaces:**
- Produces `SqliteLedgerRepository::open(path: &Path) -> Result<SqliteLedgerRepository, StorageError>` and `SqliteLedgerRepository::in_memory() -> Result<SqliteLedgerRepository, StorageError>` implementing `LedgerRepository`.
- The schema stores account, category, transaction, posting, source-evidence, and import-batch records. Transaction and source external IDs have uniqueness constraints scoped to their source.

- [ ] **Step 1: Write failing SQLite tests** for opening a fresh in-memory database, applying migration version 1 exactly once, persisting and reading an account, and rejecting a duplicate `(source_kind, external_transaction_id)` evidence record.
- [ ] **Step 2: Run `cargo test -p ledger-storage`** and verify the tests fail because the storage crate and schema do not exist.
- [ ] **Step 3: Implement SQLite persistence.** Store money as signed integer minor units plus currency code. Store UTC timestamps as integer Unix microseconds. Use foreign keys, a schema-version table, and a single SQL transaction for each multi-row ledger transaction and its postings/source evidence. Keep SQL mapping code in `sqlite.rs`; do not put SQL in the application crate. Create `0001_initial.sql` with `accounts`, `categories`, `transactions`, `postings`, `source_evidence`, `import_batches`, and `schema_version` tables.
- [ ] **Step 4: Run `cargo test -p ledger-storage`** and verify migrations, round-trip serialization, uniqueness, rollback on an invalid posting, and repository balance queries pass. Run `cargo test --workspace` to verify all domain and application tests still pass.
- [ ] **Step 5: Commit** with `git add Cargo.toml crates/ledger-storage && git commit -m "feat: persist ledger in sqlite"`.

## Task 5: Add Classification Rules and Reports

**Files:**
- Create: `crates/ledger-app/src/classification.rs`
- Create: `crates/ledger-app/src/reports.rs`
- Create: `crates/ledger-app/tests/classification_and_reports.rs`
- Modify: `crates/ledger-app/src/ports.rs`
- Create: `crates/ledger-storage/migrations/0002_classification_rules.sql`
- Modify: `crates/ledger-storage/src/sqlite.rs`

**Interfaces:**
- `ClassificationRule { id: RuleId, priority: u32, merchant_contains: Option<String>, description_contains: Option<String>, category_id: CategoryId, enabled: bool }`.
- `ClassificationService::suggest(transaction: &Transaction) -> Result<Option<CategoryId>, AppError>` selects the enabled matching rule with the lowest numeric priority; ties use the rule ID ordering.
- `CategoryService::create(category: Category) -> Result<(), AppError>`, `rename(category_id: CategoryId, name: String) -> Result<(), AppError>`, `disable(category_id: CategoryId) -> Result<(), AppError>`, and `list() -> Result<Vec<Category>, AppError>`. Disabling a category preserves existing transaction references and prevents new classification rules from targeting it.
- `ClassificationRuleRepository` exposes `insert_rule`, `list_rules`, `update_rule`, and category CRUD operations as `Result<_, RepositoryError>`; the application crate depends on this port, not SQLite.
- `ReportQuery { from_utc_inclusive, until_utc_exclusive, month_boundaries_utc: Vec<MonthBoundary> }` and `ReportSummary { income_cny_minor, expense_cny_minor, net_cny_minor, expense_by_category, income_by_month, expense_by_month }`. The UI supplies month boundaries converted from the user's local calendar into UTC; storage timestamps remain UTC.
- `ReportService::summarize(query: ReportQuery) -> Result<ReportSummary, AppError>` uses only confirmed transactions and their stored CNY report amounts.

- [ ] **Step 1: Write failing tests** for category create/rename/disable, case-insensitive merchant matching, priority ordering, disabled rules, unmatched transactions, exclusion of pending/ignored records, refund reducing expense, transfer exclusion, and month-boundary inclusion:

```rust
#[test]
fn report_uses_only_confirmed_effective_transactions() {
    let repo = memory_repository_with(vec![
        confirmed_expense_with_report_amount(3_000, utc("2026-02-01T00:00:00Z")),
        pending_expense_with_report_amount(7_000, utc("2026-02-02T00:00:00Z")),
        confirmed_transfer(5_000, utc("2026-02-03T00:00:00Z")),
        confirmed_refund_with_report_amount(500, utc("2026-02-04T00:00:00Z")),
    ]);
    let summary = ReportService::new(repo)
        .summarize(month_query("2026-02"))
        .unwrap();
    assert_eq!(summary.expense_cny_minor, 2_500);
}
```
The test module defines `memory_repository_with(transactions: Vec<Transaction>)`, `confirmed_expense_with_report_amount(minor_units: i64, occurred_at: Timestamp)`, `pending_expense_with_report_amount(minor_units: i64, occurred_at: Timestamp)`, `confirmed_transfer(minor_units: i64, occurred_at: Timestamp)`, `confirmed_refund_with_report_amount(minor_units: i64, occurred_at: Timestamp)`, and `month_query(year_month: &str) -> ReportQuery` fixtures. Each report fixture uses CNY report amounts; the month helper creates a half-open UTC range and a `MonthBoundary` for the named month.
- [ ] **Step 2: Run `cargo test -p ledger-app --test classification_and_reports`** and verify the new tests fail because the rule and report services do not exist.
- [ ] **Step 3: Implement category, classification, and report services.** Categories can be created, renamed, disabled, and listed. Rules use Unicode lowercase matching on merchant and description; they do not mutate transactions. Reports group by the supplied local-calendar month boundaries and category, and exclude a transaction without a stored CNY report amount rather than silently applying an online rate.
- [ ] **Step 4: Persist rules and run verification.** Add `crates/ledger-storage/migrations/0002_classification_rules.sql` without editing `0001_initial.sql`; implement repository read/write methods, run `cargo test -p ledger-app`, `cargo test -p ledger-storage`, and `cargo test --workspace`.
- [ ] **Step 5: Commit** with `git add crates/ledger-app crates/ledger-storage && git commit -m "feat: add classification and report services"`.

## Final Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Confirm `git diff --check` is clean and every migration has a test that opens a fresh database and upgrades a database created by the prior migration.

## Spec Coverage and Follow-on Plans

This plan implements the local domain model, account and transaction semantics, balances, classification, and report queries. It does not implement the import UI/parsers, Android notification service, Slint screens, encrypted export/import, or optional app lock. Those are separate plans with independently testable deliverables; they consume the domain and application interfaces established here. Before implementation, verify the exact exported headers and date/amount formats of current WeChat and Alipay personal bill files so import fixtures reflect real files without retaining personal data.
