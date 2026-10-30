# 多币种账户与外币记账实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 支持十种外币账户的手工收支、退款和同币种转账，并以固定的交易日汇率将外币收支纳入 CNY 报表。

**Architecture:** 领域层定义固定币种精度、精确整数折算和交易汇率快照；SQLite schema 14 与交易写入事务原子保存快照并纳入可信快照及加密备份。Tauri 通过本地汇率存储解析交易汇率并验证账户币种，React 表单显示原币输入和 CNY 折算预览；既有 CNY 导入、采集和周期流程维持 CNY 语义。

**Tech Stack:** Rust 2024、SQLite/rusqlite、chrono、Tauri 2、React 19、TypeScript、Node test、Playwright、Android WorkManager。

**Spec:** `docs/superpowers/specs/2026-10-30-multi-currency-accounting-design.md`

## Global Constraints

- 支持 CNY、USD、EUR、JPY、HKD、GBP、KRW、AUD、CAD、SGD、CHF；JPY、KRW 为 0 位小数，其余列出的币种为 2 位。
- 参考汇率定义为 `1 CNY = rate 外币`；交易 CNY 分额按 `外币金额 / rate` 用十进制整数运算四舍五入。
- 汇率只能取交易日期或更早的本地观察；无可用观察时必须显式输入手工汇率。
- 账户 posting 和余额保留原币；CNY 报表金额与外币汇率快照在交易确认时固定。
- 外币收支和退款进入 CNY 报表；只允许同币种转账；不实现跨币种转账。
- 外币账单导入、通知/支付页采集、预算、周期模板及规则生成仍限制为 CNY。
- 既有账户默认为 CNY，币种创建后不可编辑；旧 IPC 不传币种时仍按 CNY 处理。
- 不用二进制浮点数决定持久化金额；错误和溢出必须在交易提交前返回。

---

## File Map

- `crates/ledger-domain/src/exchange_rate.rs`：固定币种目录、预设 exponent、快照类型和外币到 CNY minor-unit 的纯整数换算。
- `crates/ledger-domain/src/transaction.rs`：加入 `TransactionFxSnapshot` 与 `Transaction.fx_snapshot`，并由 `validation.rs` 检查 posting、报表 CNY 金额、快照币种和转账币种一致。
- `crates/ledger-domain/src/validation.rs` 及测试：领域不变量与精确边界舍入测试。
- `crates/ledger-storage/migrations/0014_transaction_fx_snapshots.sql`：一对一外币交易汇率快照表。
- `crates/ledger-storage/src/sqlite/transaction_write.rs`、`core_repository.rs`、`mapping.rs`、`exchange_rates.rs`：快照与交易同一 SQLite 事务写入、加载和更新，并按交易日期查询最新有效汇率。
- `crates/ledger-storage/src/sqlite/migrations.rs`、`snapshots/schema.rs`、`snapshots/mod.rs`、`snapshots/validation.rs`：迁移 14、可信快照和数据校验。
- `crates/ledger-storage/src/sqlite/` 交易和快照集成测试、`crates/ledger-backup/tests/encrypted_roundtrip.rs`、`crates/ledger-backup/src/archive.rs`：迁移、往返和加密备份兼容。
- `apps/tauri-ui/src-tauri/src/ledger/accounts.rs`、`lib.rs`：账户币种 DTO、创建参数和只读账户币种；移除通用账户读取的 CNY 限制，并为账单导入和待确认流程提供 CNY 专用校验。
- `apps/tauri-ui/src-tauri/src/transactions.rs`、`exchange_rates/commands.rs` 与 tests：本地汇率日期查询、currency-aware 金额解析、确认预览数据、原币/CNY DTO 和快照写入。
- `apps/tauri-ui/src-tauri/src/ledger/day.rs`、`ledger/query.rs`、`analytics/reports.rs`：日账快捷收支、参考余额、流水列表和报表余额都明确使用账户币种；CNY 日收入/支出保持折算后总额。
- `apps/tauri-ui/src/api/accounts.ts`、`transactions.ts`、`entries.ts`、`reports.ts`、`ledger.ts` 及 `src/shared/format.ts`：更新兼容 DTO、币种金额校验和展示格式。
- `apps/tauri-ui/src/features/accounts/AccountForm.tsx`、`AccountsPage.tsx` 及样式/测试：仅新建时选择币种，展示账户币种。
- `apps/tauri-ui/src/features/transactions/TransactionEditor.tsx`、`TransactionHistory.tsx`、`features/pending/PendingEditor.tsx`、`features/ledger/LedgerPage.tsx`、`ReferenceForm.tsx` 和相关工作流测试：按账户币种录入与显示、预览折算，CNY 专用待确认流程过滤外币账户。
- `apps/tauri-ui/src/features/reports/`：余额按每个账户的币种与金额展示，禁止聚合不同币种余额。
- `docs/architecture.md`、`README.md`、`scripts/tree_descriptions.json` 和生成的目录树：记录外币录入及固定汇率语义。

## Task 1: Add exact currency conversion and domain snapshot invariants

**Files:**
- Create: `crates/ledger-domain/src/exchange_rate.rs`
- Modify: `crates/ledger-domain/src/lib.rs`
- Modify: `crates/ledger-domain/src/transaction.rs`, `crates/ledger-domain/src/validation.rs`
- Test: `crates/ledger-domain/src/exchange_rate.rs`, `crates/ledger-domain/src/validation.rs`
- Modify transaction literals in Rust test/source files to set `fx_snapshot: None` when the domain struct gains the field.

**Interfaces:**
- `CurrencyDefinition { code: &'static str, exponent: u8, name_zh: &'static str }` and `supported_currencies() -> &'static [CurrencyDefinition]` expose the fixed list.
- `convert_foreign_minor_to_cny_minor(amount_minor: i64, currency_exponent: u8, rate: &str) -> Result<i64, DomainError>` rejects nonpositive inputs, malformed rates and overflow; the rate is foreign units per CNY.
- `TransactionFxSnapshot { quote_currency: CurrencyCode, rate: String, effective_date: NaiveDate, source: FxSnapshotSource, provider: Option<String>, provider_sources: Vec<String> }` records `Reference` or `Manual` evidence.
- `Transaction.fx_snapshot: Option<TransactionFxSnapshot>` is `None` for legacy/CNY transactions and same-currency transfers.

- [ ] **Step 1: Write failing exact-conversion tests.** Add tests asserting USD 100.00 at `7.1234` becomes CNY 1404 cents; JPY 1000 at `150` becomes CNY 667 cents; a half-cent result rounds upward; JPY exponent 0 accepts no fractional unit; malformed/zero/negative rates and `i64` overflow return errors.

```rust
#[test]
fn converts_foreign_minor_units_to_cny_without_float_rounding() {
    assert_eq!(convert_foreign_minor_to_cny_minor(10_000, 2, "7.1234"), Ok(1_404));
    assert_eq!(convert_foreign_minor_to_cny_minor(1_000, 0, "150"), Ok(667));
}
```

- [ ] **Step 2: Run the focused tests and verify RED.** Run `cargo test -p ledger-domain converts_foreign_minor_units_to_cny_without_float_rounding`; it must fail because the conversion API is absent.
- [ ] **Step 3: Implement the fixed currency directory and integer conversion.** Parse rate digits into coefficient/scale, construct checked integer numerator and denominator, divide and round ties upward for positive amounts, and return `ArithmeticOverflow` on checked-operation failure. Do not add a floating-point or general-purpose decimal dependency.
- [ ] **Step 4: Add the transaction snapshot type and validation tests.** Test that a foreign expense with a matching positive CNY report and quote snapshot validates; a wrong quote, non-CNY report amount, missing snapshot, malformed provider evidence, and cross-currency transfer fail; a same-currency transfer passes.
- [ ] **Step 5: Implement domain snapshot validation and preserve existing constructors.** Add `fx_snapshot` to transaction literals as `None`; use `None` for CNY and transfer fixtures. Keep existing constructors’ CNY semantics.
- [ ] **Step 6: Run the domain package tests and format.** Run `cargo test -p ledger-domain` and `cargo fmt --all -- --check`.
- [ ] **Step 7: Commit the domain deliverable.** Stage only domain files and required transaction literal compatibility edits; commit `feat: model multi-currency transactions`.

## Task 2: Persist FX snapshots atomically and upgrade backup schema

**Files:**
- Create: `crates/ledger-storage/migrations/0014_transaction_fx_snapshots.sql`
- Modify: `crates/ledger-storage/src/sqlite/transaction_write.rs`, `core_repository.rs`, `mapping.rs`, `migrations.rs`
- Modify: `crates/ledger-storage/src/sqlite/snapshots/schema.rs`, `mod.rs`, `validation.rs`
- Create: `crates/ledger-storage/tests/transaction_fx.rs`
- Modify: `crates/ledger-backup/src/archive.rs`, `crates/ledger-backup/tests/encrypted_roundtrip.rs`

**Interfaces:**
- SQLite table `transaction_fx_snapshots` has `transaction_id` primary key and `ON DELETE CASCADE`, CNY base, quote code, exact rate text, effective date, source kind, nullable provider, and validated JSON provider source keys.
- `insert_transaction`, `update_transaction`, `get_transaction`, and snapshot export/import include `Transaction.fx_snapshot` inside the same SQLite transaction.
- Current schema version is 14; schema-13 trusted backups upgrade with no fabricated historical FX data.

- [ ] **Step 1: Write failing repository and backup tests.** Insert an external USD transaction with a matching CNY report and reference snapshot, reopen and compare every snapshot field, replace it in an atomic edit, and encrypted-backup round-trip it. Assert a schema-13 database upgrades to 14 without changing old CNY transactions.
- [ ] **Step 2: Run focused tests and verify RED.** Run `cargo test -p ledger-storage --test transaction_fx`; expect missing migration/API failure, then run `cargo test -p ledger-backup --test encrypted_roundtrip` after adding the backup case and verify it fails on schema 14 acceptance.
- [ ] **Step 3: Add migration 14 and register it.** Create the one-to-one table with checks for CNY base, supported quote currencies, source kind, positive canonical decimal, effective date length, safe provider keys and foreign key. Apply it after migration 13 and make schema version 14 current.
- [ ] **Step 4: Persist snapshots in the transaction write transaction.** Upsert/delete the one-to-one snapshot alongside transaction/posting writes; hydrate it in `get_transaction`; validate reconstructed evidence through the domain validator. Ensure failed snapshot writes roll back postings and transaction rows.
- [ ] **Step 5: Add trusted snapshot and restore support.** Append the table to snapshot schema/table order, rebuild it at schema 14, validate provider/manual evidence and quote relationship, and accept old snapshot versions through schema 14 migration.
- [ ] **Step 6: Extend encrypted backup compatibility.** Accept schema 14 archives and add an encrypted round trip that preserves rate string, valid date, source kind and provider sources; retain schema-13 restore coverage.
- [ ] **Step 7: Run focused storage/backup verification and commit.** Run `cargo test -p ledger-storage --test transaction_fx`, `cargo test -p ledger-backup --test encrypted_roundtrip`, and `cargo fmt --all -- --check`; commit `feat: persist transaction exchange snapshots`.

## Task 3: Create currency accounts and build currency-aware Tauri writes

**Files:**
- Modify: `apps/tauri-ui/src-tauri/src/ledger/accounts.rs`, `apps/tauri-ui/src-tauri/src/ledger/mod.rs`, `apps/tauri-ui/src-tauri/src/lib.rs`
- Modify: `crates/ledger-storage/src/sqlite/accounts.rs` and platform-account tests so `save_platform` creates the supplied supported currency while name/category updates retain the existing currency.
- Modify: `apps/tauri-ui/src-tauri/src/transactions.rs`, `apps/tauri-ui/src-tauri/src/ledger/parse.rs`, `ledger/day.rs`, `ledger/import_stage.rs`, `ledger/pending.rs`, `ledger/query.rs`, `analytics/reports.rs`, `exchange_rates/commands.rs`
- Modify: `crates/ledger-storage/src/sqlite/exchange_rates.rs` for the repository date lookup.
- Test: `apps/tauri-ui/src-tauri/src/ledger/tests/daily.rs`, `ledger/tests/pending.rs`, `apps/tauri-ui/src-tauri/src/transactions/tests.rs`, relevant report tests.

**Interfaces:**
- Tauri `create_account(name, category, currency_code: Option<String>)` defaults absent currency to CNY and resolves exponent from the domain currency directory.
- `Account` JSON returns `currency_code` and `currency_exponent`; update cannot modify either.
- `WriteTransaction` accepts optional `manual_fx_rate`; `SqliteLedgerRepository::exchange_rate_at_or_before(quote_currency, date)` and Tauri `exchange_rate_for_date` return the latest local observation on or before the date without network access.
- Tauri `preview_foreign_transaction(amount, quote_currency, date, manual_fx_rate)` returns original amount, exact CNY amount, rate, source and effective date using the same Rust conversion path as commit.
- `Detail` and `PageEntry` expose original transaction amount/currency, optional CNY amount, and optional FX snapshot.

- [ ] **Step 1: Write failing account-command tests.** Assert omitted currency creates a CNY account, USD creates exponent 2, JPY creates exponent 0, unsupported currency is rejected, list accounts returns code/exponent, and update cannot change an existing currency.
- [ ] **Step 2: Verify RED with focused Tauri tests.** Run `cargo test -p ledger-tauri-ui --lib create_account_defaults_to_cny`; expect the current two-argument account command/API not to support currency.
- [ ] **Step 3: Implement account currency command behavior.** Resolve currency from the fixed domain directory, pass the `CurrencyCode` into `SqliteLedgerRepository::save_platform`, preserve the default CNY IPC path, and return immutable currency metadata on list.
- [ ] **Step 4: Write failing transaction conversion tests.** Seed USD observations on Friday and Monday; save a Sunday USD expense and assert the Friday observation is snapshotted, posting remains USD, CNY report amount is correctly rounded, and an explicit manual rate is used when no earlier observation exists.
- [ ] **Step 5: Implement date-aware rate resolution, preview and manual fallback.** Query SQLite only for the newest quote observation at or before transaction local date. Reject future-only observations. If none exists, require a positive `manual_fx_rate`, create a manual snapshot effective on transaction date, and calculate report CNY with the domain integer conversion. Expose the same resolver/converter through a preview command; never calculate stored or preview CNY using JavaScript floating point.
- [ ] **Step 6: Update transaction, pending and account validation.** Make `Ledger::account` return any supported currency and add a CNY-only guard for bill-import staging and imported pending edits. Construct posting `Money` from the selected account code/exponent, require refund currency to match its original expense, allow only same-currency transfers, and persist the FX snapshot through `TransactionService` in the same repository transaction.
- [ ] **Step 7: Update the day ledger and reference balance commands.** Parse quick-entry and reference-balance amounts using the account exponent; accept an optional manual rate in the quick-entry command and calculate CNY report amounts/FX snapshots for foreign income/expense. Return account currency with the day view and each entry; keep day aggregate income/expense/net in CNY.
- [ ] **Step 8: Return original and CNY values accurately.** Populate transaction details and pages from posting `Money` as original amount; return `report_amount_cny` separately and include FX source/date/rate. Preserve old records’ absent snapshot fields. Add a repository lookup for each report balance’s account currency and do not aggregate foreign balance values into CNY totals.
- [ ] **Step 9: Run focused backend verification and commit.** Run `cargo test -p ledger-tauri-ui --lib transactions`, targeted day/account/pending/report suites, `cargo fmt --all -- --check`; commit `feat: support foreign-currency ledger writes`.

## Task 4: Complete the React account and transaction workflow

**Files:**
- Modify: `apps/tauri-ui/src/api/accounts.ts`, `transactions.ts`, `entries.ts`, `reports.ts`, `ledger.ts`
- Modify: `apps/tauri-ui/src/shared/format.ts`
- Test: `apps/tauri-ui/tests/multi-currency-accounting.test.mjs`, using `tests/helpers/workflow-ipc.mjs`.
- Modify: `apps/tauri-ui/src/features/accounts/AccountForm.tsx`, `AccountsPage.tsx`
- Modify: `apps/tauri-ui/src/features/transactions/TransactionEditor.tsx`, `TransactionHistory.tsx`
- Modify: `apps/tauri-ui/src/features/pending/PendingEditor.tsx`, `features/reports/` balance rendering, and focused workflow test helpers/tests.
- Modify: `apps/tauri-ui/src/features/ledger/LedgerPage.tsx`, `ReferenceForm.tsx`.

**Interfaces:**
- `Account` includes `currency_code` and `currency_exponent`.
- `TransactionDto` includes `original_amount`, `original_currency`, optional `report_amount_cny`, and optional `fx_snapshot`.
- `TransactionWrite.manual_fx_rate` is sent only when the user enters a manual fallback rate.
- Forms render 0/2 decimal-place inputs based on the account currency, show local reference rate/effective date when found, and show a required manual rate field when absent.

- [ ] **Step 1: Add failing account-form workflow tests.** Verify new-account form defaults to CNY and selecting USD/JPY sends the corresponding currency code; editing an existing platform hides or disables the currency selector.
- [ ] **Step 2: Run the focused UI test and verify RED.** Run `node --test --test-concurrency=1 tests/multi-currency-accounting.test.mjs` from `apps/tauri-ui`; confirm the account currency control is absent.
- [ ] **Step 3: Implement typed account currency UI.** Extend the account API type and create request; show the fixed CNY plus ten supported currency list on new account forms; display code beside account names and leave editing currency immutable.
- [ ] **Step 4: Add failing foreign transaction workflow tests.** In `tests/multi-currency-accounting.test.mjs`, verify USD shows two decimals, JPY rejects fractional input, the Rust preview command receives date/currency and returns source/effective date/CNY result, missing history requires manual rate, same-currency transfer succeeds, mixed-currency transfer shows an error, and history detail shows both currencies.
- [ ] **Step 5: Implement currency-aware entry and confirmation preview.** Use account exponent for validation/labels; request the local rate for amount/date changes; show original amount and converted CNY amount plus source/date; include manual rate only when required; invalidate preview when relevant fields change.
- [ ] **Step 6: Implement CNY-only confirmation filtering and dual-currency display.** Filter foreign accounts from bill/notification pending selectors, render list/detail amounts without the CNY symbol for foreign originals, show the CNY report amount separately, format report balances by each account currency, and format ledger reference balance/entry values using DayView currency metadata.
- [ ] **Step 7: Run the focused UI verification and commit.** Run the focused account/transaction workflow tests, `npm run build`, `npm run format:check`, and `npm run test:ui`; commit `feat: add foreign-currency entry workflow`.

## Task 5: Document, integrate, and verify on Android

**Files:**
- Modify: `README.md`, `docs/architecture.md`, `scripts/tree_descriptions.json`, generated directory index files.
- Test device: `YTROLNA69HIVSSYP` (Xiaomi 23078RKD5C, Android 16/API 36) when still attached.

- [ ] **Step 1: Document the user-visible model.** Explain original-currency account balances, CNY report conversion, stored transaction-date FX snapshots, manual fallback, and unsupported cross-currency transfers in README and architecture docs.
- [ ] **Step 2: Update generated indexes.** Add/adjust descriptions for new domain/storage/API/UI modules; run `python3 scripts/update_tree.py --check`.
- [ ] **Step 3: Run complete relevant validation.** Run `cargo test --workspace`, `cargo fmt --all -- --check`, `npm run build`, `npm run test:ui`, `npm run format:check`, `python3 scripts/update_tree.py --check`, and `git diff --check`.
- [ ] **Step 4: Build and install the latest Android debug APK.** Run `npm run tauri -- android build --debug --apk --target aarch64 --ci`; install with `adb install -r` to preserve user data; confirm package version and existing exchange-rate history.
- [ ] **Step 5: Verify the device workflow.** Create a USD account, enter a USD expense using the locally cached quote, confirm the preview includes exact original and CNY amounts, save, leave and reopen the detail, and confirm the FX snapshot/source and USD account balance persist. Verify CNY existing account data remains visible and the daily WorkManager job remains enqueued after the Activity leaves foreground.
- [ ] **Step 6: Inspect and commit documentation/index changes.** Review `git status --short`, ensure only feature files are staged, run `git diff --cached --check`, commit `docs: document multi-currency accounting`.

## Plan Self-Review

- Spec coverage: currency creation and immutable currency, native-currency balance and formatting, precise dated exchange selection, manual fallback, CNY reports, same-currency transfers, refunds, CNY-only import/capture/recurring/budget behavior, schema 14, backup compatibility, UI preview, Android device workflow and error cases each map to Tasks 1–5.
- Placeholder scan: no unresolved design markers; each task defines a concrete test, command, or implementation interface.
- Type consistency: account and transaction DTO names, `TransactionFxSnapshot` fields, `manual_fx_rate`, and currency conversion signatures are consistent across tasks.
