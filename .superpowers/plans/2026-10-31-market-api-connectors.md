# Market and Exchange API Connectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add read-only Binance, OKX, and Tonghuashun iFinD connections and a dedicated 「交易所」 navigation page while preserving 「法币汇率」.

**Architecture:** Reuse the shared credential store and connection DTOs. Each provider has its own signed Rust adapter: Binance and OKX return account balances, while iFinD obtains a short-lived token and returns one market quote. React displays provider cards and an additional-provider placeholder; no market call writes to the ledger.

**Tech Stack:** Rust 2024, Tauri 2.11, `ureq` 3.4, `hmac`, `sha2`, `base64`, `hex`, `url`, React 19, TypeScript 5.9, Playwright.

**Spec:** `.superpowers/specs/2026-10-31-readonly-api-integrations-design.md`

**Dependency:** Complete `.superpowers/plans/2026-10-31-provider-credential-foundation.md` and then `.superpowers/plans/2026-10-31-payment-api-connectors.md` first. This order keeps navigation and the shared `connections_test` dispatcher serialized.

## Global Constraints

- External requests originate in Rust and pass only necessary results through typed Tauri IPC.
- The app remains local-only and adds no API server.
- The first release performs no ledger writes, payment mutations, trading, transfers, or withdrawals.
- Credentials use Android Keystore-backed or desktop OS credential storage; there is no plaintext-file or SQLite fallback.
- Credentials, signatures, access tokens, full bill URLs, and raw provider responses never enter backups, exports, logs, or returned UI DTOs.
- Provider HTTP requests use fixed HTTPS endpoints, bounded response bodies, and explicit timeouts.
- Binance and OKX use read-only permissions; iFinD uses a refresh token and process-memory access-token cache.
- The existing staged and unstaged workspace changes are preserved; commits use `git commit --only` with explicit paths.

---

## File Map

- Create `apps/tauri-ui/src-tauri/src/connectors/markets/mod.rs`, `binance.rs`, `okx.rs`, `ifind.rs`, and `tests.rs`.
- Modify `apps/tauri-ui/src-tauri/Cargo.toml`, `Cargo.lock`, `connectors/mod.rs`, and `lib.rs` for signing dependencies and market test dispatch.
- Create `apps/tauri-ui/src/features/markets/MarketsPage.tsx`, `MarketProviderCard.tsx`, and `apps/tauri-ui/src/styles/markets.css`.
- Modify `apps/tauri-ui/src/app/Navigation.tsx`, `App.tsx`, `apps/tauri-ui/src/styles/index.css`, `apps/tauri-ui/src/api/connections.ts`, and navigation/IPC tests.
- Update `scripts/tree_descriptions.json` and regenerate directory indices.

### Task 1: Implement Binance read-only account adapter

**Interfaces:**

- `BinanceClient::account_balances(credentials, agent) -> Result<Vec<AssetBalance>, ProviderError>`.
- `AssetBalance { asset: String, free: String, locked: String }` preserves exchange decimal strings and omits zero balances.
- `MarketConnectionSummary::BinanceBalances { balances: Vec<AssetBalance> }`.

- [ ] **Step 1: Add failing tests** for the Binance HMAC-SHA256 query signature using a published known vector, URL parameter ordering, API-key header, zero-balance filtering, decimal-string preservation, error-code mapping and missing permission.

```rust
#[test]
fn hmac_sha256_matches_the_known_signature_vector() {
    assert_eq!(
        sign_query("key", "The quick brown fox jumps over the lazy dog"),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
}
```

- [ ] **Step 2: Run market tests and confirm failure** on missing signing and parsing helpers.

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::binance`

- [ ] **Step 3: Add the `hmac`, `sha2`, and `hex` dependencies** and implement a signer that takes an injected timestamp and `recvWindow` for deterministic tests.
- [ ] **Step 4: Implement the fixed-host `GET /api/v3/account` call** with `USER_DATA` signing and `X-MBX-APIKEY`; do not add order, trade, withdrawal or transfer methods.
- [ ] **Step 5: Parse bounded JSON into string-valued balances** and map authentication, permission, timestamp, network, timeout, rate-limit and malformed-response cases to safe codes.
- [ ] **Step 6: Run Binance tests.**

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::binance`

Expected: local HTTP fixtures assert exact signed query and header, and malformed or oversized responses fail without returning raw bodies.

### Task 2: Implement OKX read-only account adapter

**Interfaces:**

- `OkxClient::account_balances(credentials, agent) -> Result<Vec<AssetBalance>, ProviderError>`.
- `OkxCredentials` requires API Key, Secret, and Passphrase and contains no Trade or Withdraw operation.

- [ ] **Step 1: Add failing tests** for ISO-8601 UTC timestamp formatting, the `timestamp + method + requestPath + body` prehash, HMAC-SHA256/Base64 signature, required access headers, balance parsing, and OKX error-code mapping.
- [ ] **Step 2: Run the OKX tests and confirm failure.**

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::okx`

- [ ] **Step 3: Implement `OkxClient`** for fixed `GET /api/v5/account/balance`; include `OK-ACCESS-KEY`, `OK-ACCESS-SIGN`, `OK-ACCESS-TIMESTAMP`, and `OK-ACCESS-PASSPHRASE`. Do not call any Trade, Withdraw, or transfer endpoint.
- [ ] **Step 4: Parse the `data[].details[]` response** into nonzero currency balances using decimal strings, and reject missing account data or malformed amounts.
- [ ] **Step 5: Run OKX tests.**

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::okx`

Expected: a local server verifies exact request headers and response parsing; invalid signatures, timestamps, and platform error codes return safe outcomes.

### Task 3: Implement Tonghuashun iFinD HTTP connection

**Interfaces:**

- `IfindClient::latest_quote(refresh_token, symbol, agent, token_cache) -> Result<IfindQuote, ProviderError>`.
- `IfindQuote { symbol: String, quote_time: Option<String>, last: String }`.
- `IfindTokenCache` stores a `Zeroizing<String>` access token and expiry in process memory only.

- [ ] **Step 1: Add failing HTTP fixture tests** for refresh-token exchange, reuse of a valid access token, access-token expiry, the `/api/v1/real_time_quotation` body, quote parsing, and authentication/permission errors.
- [ ] **Step 2: Run iFinD tests and confirm failure.**

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::ifind`

- [ ] **Step 3: Implement `POST /api/v1/get_access_token`** with the configured refresh token in the `refresh_token` header; reject responses without an access token and cache valid tokens for at most seven days.
- [ ] **Step 4: Implement the fixed HTTPS real-time quote request** to `/api/v1/real_time_quotation` with `codes`, `indicators: "latest"`, `access_token`, and `ifindlang: "cn"`. Default to `000001.SZ`; accept only validated stock symbols from the UI.
- [ ] **Step 5: Parse `errorcode`, `errmsg`, `time`, and the quote data** into `IfindQuote`; never return the raw response or token. Expire cached tokens on provider authentication failure and retry token acquisition once.
- [ ] **Step 6: Run iFinD tests.**

Run: `cargo test -p ledger-tauri-ui connectors::markets::tests::ifind`

Expected: local server fixtures prove token renewal and quote request behavior without network credentials.

### Task 4: Add market test dispatch and 「交易所」 UI

**Files:** `connectors/markets/mod.rs`, `connectors/mod.rs`, `lib.rs`, `Navigation.tsx`, `App.tsx`, `MarketsPage.tsx`, `MarketProviderCard.tsx`, `markets.css`, `styles/index.css`, `api/connections.ts`.

- [ ] **Step 1: Add failing Playwright tests** for the new main-nav item, separate `markets` route, preservation of `exchange` → 「法币汇率」, provider cards for Binance/OKX/iFinD, a future-provider placeholder, credential masking, connection results, and disconnect behavior.
- [ ] **Step 2: Add IPC fixture responses** for provider status, credentials save/remove, Binance/OKX balance summaries, and iFinD quote results. Fail tests if any write/trade command is invoked.
- [ ] **Step 3: Extend the existing `connections_test(request)` dispatcher** with Binance, OKX, and iFinD cases. Require the app-unlocked guard, release the ledger mutex before network work, load credentials into zeroizing values, dispatch only the three read-only clients, and return their matching shared summary variants.
- [ ] **Step 4: Add `markets` to `View`, `primaryViews`, and `destinations`** while keeping `exchange` unchanged. Render three real provider cards and one disabled 「更多交易所，后续接入」 placeholder.
- [ ] **Step 5: Add configuration forms** with only each provider's required credentials. Clear secret fields after save; render configured status without retrieving credentials; show asset balances and one iFinD quote after a successful test.
- [ ] **Step 6: Run the focused market UI test and TypeScript build.**

Run: `node --test --test-concurrency=1 apps/tauri-ui/tests/market-api-connections.test.mjs`

Run: `npm --prefix apps/tauri-ui run build`

Expected: the new route works at phone width and existing fiat-rate navigation still reaches `ExchangeRatesPage`.

### Task 5: Run integration checks and commit market adapters

- [ ] **Step 1: Run all market provider tests.**

Run: `cargo test -p ledger-tauri-ui connectors::markets`

- [ ] **Step 2: Run focused UI, frontend build, and format checks.**

Run: `node --test --test-concurrency=1 apps/tauri-ui/tests/market-api-connections.test.mjs`

Run: `npm --prefix apps/tauri-ui run build`

Run: `npm --prefix apps/tauri-ui run format:check`

- [ ] **Step 3: Run the required workspace and UI regression suites.**

Run: `cargo test --workspace`

Run: `npm --prefix apps/tauri-ui run test:ui`

Expected: provider integration changes pass alongside existing ledger, account, exchange-rate, and settings workflows.

- [ ] **Step 4: Regenerate and check directory indices.**

Run: `python scripts/update_tree.py`

Run: `python scripts/update_tree.py --check`

- [ ] **Step 5: Build the Android arm64 APK.**

Run: `bash scripts/build_android.sh arm64-v8a`

Expected: the Tauri app and native keyring backend link for Android arm64.

- [ ] **Step 6: Verify credential persistence on an Android device when available.** Save test credentials, restart the app and confirm status remains configured, then disconnect and confirm they are removed. Do not display or log the values. Record this check as unverified if no device is available.

## Market Acceptance

- Binance and OKX account balances are retrievable with read-only permission and no write endpoint exists in these adapters.
- iFinD obtains and refreshes its short-lived access token and returns a parsed quote without persisting the token.
- The 「交易所」 page is independent from 「法币汇率」 and labels iFinD as a market-data provider.
- `cargo test --workspace`, the complete UI suite, focused market tests, UI build, format/tree checks, and Android arm64 build pass. Device credential persistence is reported separately when no device is available.
