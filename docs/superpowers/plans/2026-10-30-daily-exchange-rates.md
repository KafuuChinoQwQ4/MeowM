# 每日汇率更新与趋势图实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 增加本地可离线查看的 CNY 参考汇率历史和曲线，并让 Android 在界面关闭后通过持久化任务每日尝试补齐汇率。

**Architecture:** SQLite 以 schema 13 保存汇率观察值和安全同步状态，Rust 汇率服务独占 API 请求、校验、事务和 Tauri 命令。Android WorkManager 经 JNI 调用同一 Rust 服务；桌面启动时在后台线程检查，React 页面只查询本地历史并绘制 SVG。加密快照、旧备份升级和目录索引同步包含新增数据及文件。

**Tech Stack:** Rust 2024、SQLite/rusqlite、chrono、serde_json arbitrary precision、ureq 3.4.2、Tauri 2、Kotlin、AndroidX WorkManager 2.12.0、React 19、TypeScript、Node test、Playwright。

**Spec:** `docs/superpowers/specs/2026-10-30-daily-exchange-rates-design.md`

## Global Constraints

- 以人民币（CNY）为基准，保存 USD、EUR、JPY、HKD、GBP、KRW、AUD、CAD、SGD、CHF 的每日参考汇率。
- 使用 Frankfurter v2 公共接口的默认混合数据。请求最新数据、日期范围和来源信息；不发送账本、账户或交易数据，不需要用户 API 密钥。
- 在本机 SQLite 保存汇率观察值、来源和同步状态；更新幂等，断网、超时或数据无效不覆盖已有记录。
- Android 使用 WorkManager 的唯一周期任务，要求有网络时运行并启用失败退避；任务启动后通过现有 Rust JNI 桥接入口调用 Rust 汇率服务，复用同一 SQLite 迁移、校验和写入代码。
- 桌面版在启动及汇率页手动刷新时更新；桌面应用进程退出后不安装系统级计划任务。
- 把汇率数据纳入 schema 迁移、快照校验、加密备份和恢复。
- 周末及来源未发布数据的节假日不插入伪造观察值；每个日期只保存来源实际返回的观察值。
- 汇率以精确十进制文本持久化，不通过二进制浮点数舍入后写入数据库。
- Android 周期任务受网络和省电调度影响，不承诺在某个精确时刻完成。

---

## File Map

- `crates/ledger-storage/migrations/0013_exchange_rates.sql` defines the two new tables and their constraints.
- `crates/ledger-storage/src/sqlite/exchange_rates.rs` validates and persists observations and synchronization state; `mod.rs` exports its repository methods.
- `crates/ledger-storage/src/sqlite/migrations.rs` applies schema 13 after schema 12; `snapshots/schema.rs` and `snapshots/mod.rs` include the new tables in trusted portable snapshots.
- `crates/ledger-storage/tests/exchange_rates.rs` covers idempotency, validation, migration from schema 12, and reopen persistence.
- `crates/ledger-storage/tests/sqlite_repository.rs`, `recurring_persistence.rs`, `reconciliation_persistence.rs`, and `search_and_rules.rs` keep their current-schema assertions aligned with schema 13.
- `crates/ledger-backup/src/archive.rs` accepts schema 13; `crates/ledger-backup/tests/encrypted_roundtrip.rs` covers new history and legacy schema upgrades.
- `apps/tauri-ui/src-tauri/Cargo.toml` adds the HTTPS client and exact JSON-number parsing features; workspace `Cargo.lock` records the resolved graph.
- `apps/tauri-ui/src-tauri/src/exchange_rates/` contains the provider contract, HTTP client, synchronization service, DTOs, Tauri commands, and Android JNI boundary.
- `apps/tauri-ui/src-tauri/src/lib.rs` registers the new module and commands and starts the desktop foreground-process check.
- `apps/tauri-ui/src-tauri/gen/android/app/build.gradle.kts` adds WorkManager runtime and test dependencies; `MainActivity.kt`, `RustBridge.kt`, and new `ExchangeRateWorker.kt`/`ExchangeRateWorkScheduler.kt` schedule and run Android work.
- `apps/tauri-ui/src-tauri/gen/android/app/src/androidTest/java/com/example/localledger/ExchangeRateWorkSchedulerTest.kt` verifies unique periodic scheduling and network constraints.
- `apps/tauri-ui/src/api/exchangeRates.ts` defines typed history, status, and refresh calls.
- `apps/tauri-ui/src/features/exchangeRates/` contains the page, range/currency selectors, and accessible SVG line chart.
- `apps/tauri-ui/src/app/Navigation.tsx`, `App.tsx`, and `styles/index.css` add the finance destination and styles.
- `apps/tauri-ui/tests/exchange-rates.test.mjs` covers the visible page, range changes, data points, empty history, and refresh failure through mocked Tauri IPC.
- `docs/architecture.md`, `scripts/tree_descriptions.json`, and generated `tree` files document the new boundaries.
- `docs/superpowers/plans/2026-10-30-daily-exchange-rates.md` records the staged implementation and validation sequence.

## Task 1: Add schema 13 and exchange-rate storage

**Files:**
- Create: `crates/ledger-storage/migrations/0013_exchange_rates.sql`
- Create: `crates/ledger-storage/src/sqlite/exchange_rates.rs`
- Create: `crates/ledger-storage/tests/exchange_rates.rs`
- Modify: `crates/ledger-storage/tests/sqlite_repository.rs`
- Modify: `crates/ledger-storage/tests/recurring_persistence.rs`
- Modify: `crates/ledger-storage/tests/reconciliation_persistence.rs`
- Modify: `crates/ledger-storage/tests/search_and_rules.rs`
- Modify: `crates/ledger-backup/src/archive.rs`
- Modify: `crates/ledger-backup/tests/encrypted_roundtrip.rs`
- Modify: `crates/ledger-storage/src/sqlite/mod.rs`
- Modify: `crates/ledger-storage/src/sqlite/migrations.rs`
- Modify: `crates/ledger-storage/src/sqlite/snapshots/mod.rs`
- Modify: `crates/ledger-storage/src/sqlite/snapshots/schema.rs`
- Modify: `crates/ledger-storage/src/sqlite/snapshots/validation.rs`

**Interfaces:**
- Produces public `ExchangeRateObservation { effective_date: NaiveDate, base_currency: String, quote_currency: String, rate: String, provider: String, provider_sources: Vec<String>, fetched_at_utc: DateTime<Utc> }`.
- Produces public `ExchangeRateSyncState { last_attempt_utc: Option<DateTime<Utc>>, last_success_utc: Option<DateTime<Utc>>, status: String, error_code: Option<String> }`.
- Produces `SqliteLedgerRepository::upsert_exchange_rates(&[ExchangeRateObservation]) -> Result<usize, RepositoryError>`, `exchange_rates(&str, from_inclusive: NaiveDate, until_exclusive: NaiveDate) -> Result<Vec<ExchangeRateObservation>, RepositoryError>`, `exchange_rate_sync_state() -> Result<Option<ExchangeRateSyncState>, RepositoryError>`, and `record_exchange_rate_sync(&ExchangeRateSyncState)`.

- [ ] **Step 1: Write failing storage and backup tests.** Add storage tests that insert two dates for USD, repeat one observation without creating a duplicate, reject unsupported currency and nonpositive/nondecimal rates, persist sync status, reopen the file, and upgrade a schema-12 database while preserving its accounts and transactions. Add an encrypted backup test that stores a quote, verifies the current schema version and observation rate survive inspection, and restores the observation into another repository.

```rust
#[test]
fn rate_observations_are_validated_idempotent_and_persisted() {
    let repo = SqliteLedgerRepository::in_memory().unwrap();
    let friday = observation("2026-10-30", "USD", "7.12");
    assert_eq!(repo.upsert_exchange_rates(&[friday.clone()]).unwrap(), 1);
    assert_eq!(repo.upsert_exchange_rates(&[friday]).unwrap(), 0);
    let rows = repo.exchange_rates("USD", date("2026-10-01"), date("2026-11-01")).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].rate, "7.12");
}
```

- [ ] **Step 2: Run the focused tests and verify RED.** Run `cargo test -p ledger-storage --test exchange_rates`; it must fail because the observation API and schema 13 do not exist.

- [ ] **Step 3: Add the migration and storage APIs.** `exchange_rates` has primary key `(effective_date, base_currency_code, quote_currency_code)`, a `CNY` base check, fixed provider metadata, exact rate text, and UTC fetch time. `exchange_rate_sync` has singleton key `1`, nullable attempt/success times, constrained status and optional safe error code. Validate exact positive decimal syntax and supported currency codes before the transaction. Use `INSERT ... ON CONFLICT DO NOTHING` for immutable daily observations; update sync state in its own transaction.

```sql
CREATE TABLE exchange_rates (
  effective_date TEXT NOT NULL,
  base_currency_code TEXT NOT NULL CHECK(base_currency_code = 'CNY'),
  quote_currency_code TEXT NOT NULL CHECK(length(quote_currency_code) = 3),
  rate TEXT NOT NULL,
  provider TEXT NOT NULL,
  provider_sources TEXT NOT NULL,
  fetched_at_utc_micros INTEGER NOT NULL,
  PRIMARY KEY(effective_date, base_currency_code, quote_currency_code)
);
CREATE TABLE exchange_rate_sync (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  last_attempt_utc_micros INTEGER,
  last_success_utc_micros INTEGER,
  status TEXT NOT NULL CHECK(status IN ('success','no_new_data','network_error','timeout','provider_error','invalid_data')),
  error_code TEXT
);
```

- [ ] **Step 4: Upgrade migration and snapshot paths.** Append migration 13 to the trusted snapshot migration list, invoke its repository migration from `sqlite/mod.rs` after `apply_merchant_order_migration`, accept version 13 in `migrations.rs`, append both tables to snapshot table order, extend `empty_at` to 13, and validate date/rate/status fields during snapshot restore. Update existing repository tests that assert the current schema version from 12 to 13.

- [ ] **Step 5: Run focused storage and snapshot tests.** Run `cargo test -p ledger-storage`; it must pass, including old schema-12 upgrade, snapshot restore, and corrupted row rejection.

- [ ] **Step 6: Extend encrypted backup compatibility.** Change the accepted schema range in `archive.rs` from `1..=12` to `1..=13`; update the legacy upgrade assertion to 13 and add a schema-13 roundtrip assertion that preserves the sample observation. Run `cargo test -p ledger-backup`.

- [ ] **Step 7: Update direct-file descriptions and verify indexes.** Add exact descriptions for the new Rust files, migration, backup test, and storage test to `scripts/tree_descriptions.json`; run `python3 scripts/update_tree.py` and `python3 scripts/update_tree.py --check`.

- [ ] **Step 8: Commit the storage boundary.** Stage only the migration, storage implementation/tests, backup changes, affected snapshot/migration files, and their generated indexes; commit as `feat: store exchange rate history`.

## Task 2: Add the validated Frankfurter provider and Rust sync service

**Files:**
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/mod.rs`
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/provider.rs`
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/service.rs`
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/tests.rs`
- Modify: `apps/tauri-ui/src-tauri/Cargo.toml`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`

**Interfaces:**
- `RateProvider::fetch(from: Option<NaiveDate>, through: NaiveDate) -> Result<Vec<ExchangeRateObservation>, RateProviderError>` is injectable for tests; both supplied bounds are inclusive because Frankfurter's `from`/`to` query is inclusive. The service queries `newest_exchange_rate_date()` to calculate incremental backfill.
- `ExchangeRateService::sync(repository, provider, now_utc, force) -> Result<ExchangeRateSyncState, ExchangeRateSyncError>` stores the validated observations and sync state.
- `HttpRateProvider::new(agent, base_url)` defaults to `https://api.frankfurter.dev/v2` and supports a test-local base URL.

- [ ] **Step 1: Write parser and synchronization tests.** Cover accepted v2 rows, preservation of the JSON number's exact decimal spelling, rejected unknown quote/nonpositive rate/bad date/malformed provider attribution, initial 365-day request, incremental request from the newest date plus one day, empty weekend response, duplicate synchronization, and stale-cache preservation after transport failure.

```rust
#[test]
fn parses_decimal_rate_without_binary_float_rounding() {
    let rows = parse_rates(r#"[{"date":"2026-10-30","base":"CNY","quote":"USD","rate":7.12345678912345}]"#).unwrap();
    assert_eq!(rows[0].rate, "7.12345678912345");
}

#[test]
fn provider_failure_keeps_existing_observations_and_records_safe_status() {
    let repo = seeded_repo_with_rate("2026-10-30", "USD", "7.12");
    let state = ExchangeRateService::sync(&repo, &FailingProvider::timeout(), utc("2026-10-31T04:00:00Z"), true).unwrap();
    assert_eq!(state.status, "timeout");
    assert_eq!(repo.exchange_rates("USD", day("2026-10-30"), day("2026-10-31")).unwrap()[0].rate, "7.12");
}
```

- [ ] **Step 2: Run `cargo test -p ledger-tauri-ui --lib exchange_rates` and verify RED.** The parser and service tests must fail because the module and provider contract are absent.

- [ ] **Step 3: Implement the provider and service.** Add `ureq = { version = "3.4.2", features = ["json"] }` and enable `serde_json` `arbitrary_precision` for this crate. Configure a 20-second global timeout, stream at most 512 KiB, request only the fixed ten quotes, expand provider attribution, inspect status before parsing, accept partial quote sets per returned date but reject unexpected quotes, and persist only after validating the complete response. Initial sync asks for 365 natural days (`today - 364 days` through today, inclusive); later sync starts one day after the newest stored effective date and ends at the service date. Translate the half-open storage/UI end date to Frankfurter's inclusive `to` by subtracting one day. Manual sync sets `force = true`; scheduled sync skips a second attempt on the same local date.

```rust
pub const QUOTE_CURRENCIES: [&str; 10] = ["USD", "EUR", "JPY", "HKD", "GBP", "KRW", "AUD", "CAD", "SGD", "CHF"];
pub const HISTORY_DAYS: i64 = 365;
pub const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
```

- [ ] **Step 4: Add desktop startup synchronization.** Register the module in `lib.rs`; on non-Android targets only, Tauri `setup` clones the database path and spawns a Rust thread that calls the non-forced sync service. Do not block window creation and do not capture any account or transaction data in the request.

- [ ] **Step 5: Run Rust service tests and formatting.** Run `cargo test -p ledger-tauri-ui --lib exchange_rates` and `cargo fmt --all -- --check`; tests use only fixture transport and do not depend on public network availability.

- [ ] **Step 6: Update indexes and commit.** Add module file descriptions, run `python3 scripts/update_tree.py --check`, then commit only provider/service/dependency and related index files as `feat: sync daily reference exchange rates`.

## Task 3: Expose rate history/status and a shared Android JNI entry point

**Files:**
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/commands.rs`
- Create: `apps/tauri-ui/src-tauri/src/exchange_rates/jni.rs`
- Modify: `apps/tauri-ui/src-tauri/src/exchange_rates/mod.rs`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`
- Modify: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/RustBridge.kt`

**Interfaces:**
- Tauri commands: `exchange_rates_history(quote_currency, from_date_inclusive, until_date_exclusive)`, `exchange_rates_sync_state()`, and `exchange_rates_refresh()`.
- JNI method: `RustBridge.nativeSyncExchangeRates(path: String): String`, returning a bounded JSON status containing only `status`, `latest_date`, and optional safe `error_code`.

- [ ] **Step 1: Write Rust command tests.** With a temporary SQLite path, test valid quote/date ranges, reject unsupported quotes and reversed ranges, return empty history without network access, and verify a forced sync calls a fake provider and returns safe status.

```rust
#[test]
fn history_command_rejects_unlisted_currency_and_reversed_range() {
    let ledger = Ledger::open(&temp_path()).unwrap();
    assert!(ledger.exchange_rates_history("XXX", "2026-10-01", "2026-11-01").is_err());
    assert!(ledger.exchange_rates_history("USD", "2026-11-01", "2026-10-01").is_err());
}
```

- [ ] **Step 2: Run focused Tauri Rust tests and verify RED.** Run `cargo test -p ledger-tauri-ui --lib exchange_rates`; command methods and JNI boundary must be missing.

- [ ] **Step 3: Implement commands.** Query and refresh use the existing `LedgerState` path and privacy guard for WebView calls. History returns decimal rates as strings and dates in ISO format. Errors crossing IPC use stable localized messages and do not contain URLs or provider response bodies.

- [ ] **Step 4: Implement the Android JNI function.** Parse the path and call `SqliteLedgerRepository::open` plus the shared sync service inside `catch_unwind`; cap the incoming path length at 4096 bytes and the returned JSON at 1024 bytes. Never panic across JNI or log network response content.

- [ ] **Step 5: Add the Kotlin external declaration.** Add `@JvmStatic external fun nativeSyncExchangeRates(path: String): String` to `RustBridge.kt`. Cover safe status decoding and worker success/retry mapping in the WorkManager tests in Task 4.

- [ ] **Step 6: Run Rust tests and Android compilation.** Run `cargo test -p ledger-tauri-ui --lib exchange_rates` and `cd apps/tauri-ui/src-tauri/gen/android && ./gradlew :app:compileDebugKotlin`.

- [ ] **Step 7: Update indexes and commit.** Update descriptions for `commands.rs` and `jni.rs`, run the tree check, and commit as `feat: expose exchange rate sync commands`.

## Task 4: Schedule Android refresh with WorkManager

**Files:**
- Create: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/ExchangeRateWorker.kt`
- Create: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/ExchangeRateWorkScheduler.kt`
- Create: `apps/tauri-ui/src-tauri/gen/android/app/src/androidTest/java/com/example/localledger/ExchangeRateWorkSchedulerTest.kt`
- Modify: `apps/tauri-ui/src-tauri/gen/android/app/build.gradle.kts`
- Modify: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/MainActivity.kt`

**Interfaces:**
- `ExchangeRateWorkScheduler.ensureScheduled(context)` enqueues one unique 24-hour periodic task with `NetworkType.CONNECTED` and exponential retry.
- `ExchangeRateWorkScheduler.enqueueStartupSync(context)` enqueues unique immediate work with `ExistingWorkPolicy.KEEP`.
- `ExchangeRateWorker` calls `RustBridge.nativeSyncExchangeRates(RustBridge.databasePath(applicationContext))`; only `success` and `no_new_data` return `Result.success()`, while network/provider/invalid-data and JNI failures return `Result.retry()`.

- [ ] **Step 1: Add WorkManager test dependencies and write instrumentation tests.** Use AndroidX WorkManager runtime and test artifacts `2.12.0`; verify one unique periodic job, 24-hour interval, connected-network constraint, and that calling `ensureScheduled` twice still yields exactly one unique job. Add the periodic work name as a request tag so the constraint test can identify it.

```kotlin
@Test
fun ensureScheduledKeepsOneConnectedDailyWorker() {
    ExchangeRateWorkScheduler.ensureScheduled(context)
    ExchangeRateWorkScheduler.ensureScheduled(context)
    val infos = WorkManager.getInstance(context)
        .getWorkInfosForUniqueWork(ExchangeRateWorkScheduler.PERIODIC_WORK_NAME)
        .get()
    assertEquals(1, infos.size)
    assertTrue(infos.single().tags.contains(ExchangeRateWorkScheduler.PERIODIC_WORK_NAME))
}
```

- [ ] **Step 2: Compile/test the Android app and verify RED.** Run `cd apps/tauri-ui/src-tauri/gen/android && ./gradlew :app:connectedDebugAndroidTest`; the new scheduler and test classes must be missing. If no emulator/device is attached, record that limitation and use compilation plus WorkManager request-construction unit coverage until a device is available.

- [ ] **Step 3: Add WorkManager 2.12.0 dependencies.** Add `implementation("androidx.work:work-runtime-ktx:2.12.0")` and `androidTestImplementation("androidx.work:work-testing:2.12.0")` to the Android app dependencies.

- [ ] **Step 4: Implement unique daily and startup work.** Use `PeriodicWorkRequestBuilder<ExchangeRateWorker>(24, TimeUnit.HOURS)`, `Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED)`, exponential backoff, and `enqueueUniquePeriodicWork(..., ExistingPeriodicWorkPolicy.KEEP, request)`. Startup work uses a stable unique name and `ExistingWorkPolicy.KEEP` to avoid overlapping attempts.

```kotlin
const val PERIODIC_WORK_NAME = "meowm-exchange-rates-daily"
const val STARTUP_WORK_NAME = "meowm-exchange-rates-startup"
private val intervalHours = 24L
```

- [ ] **Step 5: Schedule work from Android startup.** After `super.onCreate(savedInstanceState)`, call `ensureScheduled(this)` and `enqueueStartupSync(this)`. The periodic task remains registered after Activity/process exit and reboot; do not add a foreground service or exact alarm.

- [ ] **Step 6: Run Android checks.** Run `./gradlew :app:compileDebugKotlin :app:assembleDebug`; run `connectedDebugAndroidTest` when an emulator/device is available. Verify generated APK includes WorkManager and Rust JNI symbol resolves in the existing Android Rust build.

- [ ] **Step 7: Update indexes and commit.** Add descriptions for the two Kotlin classes and the instrumented test; run tree check; commit as `feat: schedule Android exchange rate refresh`.

## Task 5: Add the exchange-rate UI and SVG trend chart

**Files:**
- Create: `apps/tauri-ui/src/api/exchangeRates.ts`
- Create: `apps/tauri-ui/src/features/exchangeRates/ExchangeRatesPage.tsx`
- Create: `apps/tauri-ui/src/features/exchangeRates/RateChart.tsx`
- Create: `apps/tauri-ui/src/features/exchangeRates/tree`
- Create: `apps/tauri-ui/src/styles/exchangeRates.css`
- Create: `apps/tauri-ui/tests/exchange-rates.test.mjs`
- Modify: `apps/tauri-ui/src/app/Navigation.tsx`
- Modify: `apps/tauri-ui/src/app/App.tsx`
- Modify: `apps/tauri-ui/src/styles/index.css`
- Modify: `apps/tauri-ui/tests/helpers/navigation.mjs`
- Modify: `apps/tauri-ui/tests/helpers/workflow-ipc.mjs`
- Modify: `apps/tauri-ui/src/shared/calendar.ts`

**Interfaces:**
- API types: `ExchangeRateHistoryRow { effective_date: string; quote_currency: string; rate: string; provider: string; provider_sources: string[] }` and `ExchangeRateSyncState`.
- API methods: `exchangeRatesApi.history(quote, fromDate, untilDate)`, `exchangeRatesApi.state()`, `exchangeRatesApi.refresh()`.
- Page default: USD and 90 days; target currencies are the ten fixed quotes; ranges are 30, 90, and 365 calendar days.

- [ ] **Step 1: Write failing browser workflow tests.** Verify Finance contains an exchange-rate destination; page defaults to USD/90 days with a 90-calendar-day half-open query; changing to JPY requests JPY history; selecting 30 and 365 days sends the matching boundaries; refresh invokes the command; returned observations appear as chart points and status; empty history and rejected refresh retain usable error/retry UI.

```js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite(process.env.EXCHANGE_RATES_UI_TEST_PORT || '1437');

test('exchange rate page renders local history and changing currency reloads it', async () => {
  const page = await newPage();
  try {
    await installWorkflowIpc(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    assert.equal(await page.getByLabel('目标币种').inputValue(), 'USD');
    await page.waitForFunction(() => window.workflowCalls.some(
      (call) => call.command === 'exchange_rates_history'));
    const defaultRange = await page.evaluate(() => window.workflowCalls.find(
      (call) => call.command === 'exchange_rates_history').args);
    assert.equal((Date.parse(defaultRange.untilDate) - Date.parse(defaultRange.fromDate)) / 86400000, 90);
    await page.getByLabel('目标币种').selectOption('JPY');
    await page.waitForFunction(() => window.workflowCalls.some(
      (call) => call.command === 'exchange_rates_history' && call.args.quoteCurrency === 'JPY'));
    assert.equal(await page.getByRole('img', { name: /CNY 至 JPY/ }).count(), 1);
  } finally {
    await page.close();
  }
});
```

- [ ] **Step 2: Run the focused browser test and verify RED.** Run `node --test tests/exchange-rates.test.mjs`; it must fail because the destination, page, and IPC client do not exist.

- [ ] **Step 3: Implement typed API methods and finance navigation.** Use the existing single `invoke` client. Add `exchange` to `View`, Finance children, and destination metadata; render `ExchangeRatesPage` in `App.tsx` without changing primary navigation.

- [ ] **Step 3a: Extend the shared navigation helper and date range helper.** Add `exchange: ['finance', 'exchange']` to `tests/helpers/navigation.mjs`; implement `recentDaysRange(anchorDate, count)` in `src/shared/calendar.ts` with half-open date boundaries; exercise its 30, 90, and 365 day windows plus leap-day boundaries through captured history IPC arguments in `exchange-rates.test.mjs`.

- [ ] **Step 4: Implement page state.** Use `useResource` for history and sync state, `today()` plus date helpers for query ranges, stable currency labels, a manual refresh button, and retry feedback. Show the service's effective date and source note, and keep old chart data visible when refresh fails.

- [ ] **Step 5: Implement the responsive SVG chart.** Convert the returned decimal strings to finite `Number` values only for drawing; calculate a padded y-axis from min/max, make one SVG point per actual observation, expose point date/rate with focusable buttons or SVG title/accessible labels, and render a no-data state without invented dates.

- [ ] **Step 6: Run frontend verification.** Run `node --test tests/exchange-rates.test.mjs`, `npm run build`, and `npm run format:check` from `apps/tauri-ui`.

- [ ] **Step 7: Update indexes and commit.** Add descriptions for the API, page, chart, stylesheet, test, and navigation entries; run `python3 scripts/update_tree.py --check`; commit as `feat: add exchange rate trend chart`.

## Task 6: Document behavior and run integrated validation

**Files:**
- Modify: `docs/architecture.md`
- Modify: `README.md`
- Modify: `scripts/tree_descriptions.json`
- Modify: generated `tree` files under changed directories.

- [ ] **Step 1: Update architecture documentation.** Describe the Rust provider/storage boundary, Android WorkManager-to-JNI flow, desktop foreground-only behavior, and offline chart reads.

- [ ] **Step 2: Update README feature list.** Add daily reference exchange rates and local historical trend chart, with a short note that rates are public reference values and Android background execution timing is controlled by the OS.

- [ ] **Step 3: Run integrated Rust and UI checks.** Run `cargo test --workspace`, `cargo fmt --all -- --check`, `npm run build`, `npm run test:ui`, `npm run format:check`, and `python3 scripts/update_tree.py --check`.

- [ ] **Step 4: Build Android debug APK.** Run `cd apps/tauri-ui/src-tauri/gen/android && ./gradlew :app:assembleDebug`; if an emulator is available, also run `./gradlew :app:connectedDebugAndroidTest`. Report build and device lifecycle evidence separately.

- [ ] **Step 5: Inspect the final patch.** Run `git diff --check`, review `git status --short`, and verify no changes from the pre-existing platform-deletion work were staged or overwritten.

- [ ] **Step 6: Commit documentation and remaining integration changes.** Stage only this feature's docs/index files and commit as `docs: document daily exchange rate support`.

## Plan self-review

- Spec coverage: provider selection, fixed quotes, exact decimal storage, daily/backfill requests, idempotency, offline history, safe error states, WorkManager constraints/retry, desktop foreground-only behavior, chart UX, schema migration, backup compatibility, and validation all have explicit tasks.
- Placeholder scan: no TBD/TODO steps; each task defines paths, interfaces, commands, and expected outcomes.
- Type consistency: storage rows use ISO dates and decimal strings; Tauri history DTO and TypeScript API preserve those same fields; Android JNI returns status only and shares the storage sync service.
