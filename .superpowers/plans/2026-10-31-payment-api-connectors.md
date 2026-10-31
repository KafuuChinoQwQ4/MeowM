# Payment API Connectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the local user's WeChat Pay and Alipay merchant accounts to read and validate one selected historical merchant bill without writing it into MeowM's ledger.

**Architecture:** Reuse the shared provider credential and IPC foundation. Rust adapters sign platform requests, download bounded bill content in memory, validate WeChat's SHA1 digest over the original uncompressed file, and return only a safe bill result. React places these controls under Settings → Platforms and Collection.

**Tech Stack:** Rust 2024, Tauri 2.11, `ureq` 3.4, RSA PKCS#1 v1.5 with SHA-256 (RSA2), `sha2`, `sha1`, `flate2`, `url`, React 19, TypeScript 5.9, Playwright.

**Spec:** `.superpowers/specs/2026-10-31-readonly-api-integrations-design.md`

**Dependency:** Complete `.superpowers/plans/2026-10-31-provider-credential-foundation.md` first.

## Global Constraints

- External requests originate in Rust and pass only necessary results through typed Tauri IPC.
- The app remains local-only and adds no API server.
- The first release performs no ledger writes, payment mutations, trading, transfers, or withdrawals.
- Credentials use Android Keystore-backed or desktop OS credential storage; there is no plaintext-file or SQLite fallback.
- Credentials, signatures, access tokens, full bill URLs, and raw provider responses never enter backups, exports, logs, or returned UI DTOs.
- Provider HTTP requests use fixed HTTPS endpoints, bounded response bodies, and explicit timeouts.
- WeChat bill dates are selected by the user, are within the documented three-month window, and are not today; the file hash is checked after GZIP decompression.
- The existing staged and unstaged workspace changes are preserved; commits use `git commit --only` with explicit paths.

---

## File Map

- Create `apps/tauri-ui/src-tauri/src/connectors/payments/mod.rs`, `wechat.rs`, `alipay.rs`, and `tests.rs` for the separate signed clients and shared dispatch.
- Modify `apps/tauri-ui/src-tauri/Cargo.toml` and `Cargo.lock` for direct cryptographic, URL-validation, hash, and GZIP dependencies.
- Create `apps/tauri-ui/src/features/connections/PaymentConnectionsPage.tsx` and `PaymentProviderCard.tsx`; create `apps/tauri-ui/src/styles/paymentConnections.css`.
- Modify `apps/tauri-ui/src/app/Navigation.tsx`, `App.tsx`, `apps/tauri-ui/src/styles/index.css`, `apps/tauri-ui/src/api/connections.ts`, and workflow IPC/navigation tests.
- Update `scripts/tree_descriptions.json` and regenerate directory indices.

### Task 1: Lock down bill request contracts and signing tests

**Interfaces:**

- `PaymentProviderId::{WechatPay, Alipay}`.
- `PaymentTestRequest { provider: ProviderId, bill_date: String }`.
- `ConnectionTestResult` uses the shared shape from the foundation plan. Safe payment summary is `BillReadSummary { bill_date: String, downloaded_bytes: u64, hash_verified: Option<bool> }`.
- `payments::test(provider, credentials, bill_date, agent) -> Result<BillReadSummary, ProviderError>` accepts an injected `ureq::Agent` and fixed-host configuration for tests.

- [ ] **Step 1: Add failing tests** for canonical date validation, refusal of today's or an out-of-window WeChat date, URL path encoding, and deterministic RSA2/RSA signature verification with test-only PEM fixtures.

```rust
#[test]
fn wechat_rejects_today_and_accepts_a_valid_prior_day() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
    assert!(validate_wechat_bill_date("2026-10-31", today).is_err());
    assert!(validate_wechat_bill_date("2026-10-30", today).is_ok());
}
```

- [ ] **Step 2: Run the payment tests and confirm failure** on the missing request/signing helpers.

Run: `cargo test -p ledger-tauri-ui connectors::payments`

Expected: unresolved payment test module symbols fail compilation.

- [ ] **Step 3: Add direct Rust dependencies** for RSA signing, SHA-256, SHA-1, URL parsing/encoding, and GZIP decoding; add only test-only fixtures or deterministic test-key generation dependencies under `[dev-dependencies]`.

- [ ] **Step 4: Implement date validation and signature canonicalization helpers.** Test WeChat's method/path/query/timestamp/nonce/body canonical string and Alipay's sorted parameter canonical string before making network calls.

- [ ] **Step 5: Verify signatures against the test public keys and rerun focused tests.**

Run: `cargo test -p ledger-tauri-ui connectors::payments`

Expected: invalid key, invalid date, and altered canonical input are rejected; fixed test signatures verify.

### Task 2: Implement WeChat Pay V3 trade-bill read

**Files:** `connectors/payments/wechat.rs`, `connectors/payments/tests.rs`.

- [ ] **Step 1: Add local-server tests** for the signed `GET /v3/bill/tradebill` request, JSON bill metadata, five-minute download URL handling, GZIP response, and a SHA1 mismatch.
- [ ] **Step 2: Run the WeChat tests and confirm failure.**

Run: `cargo test -p ledger-tauri-ui connectors::payments::tests::wechat`

Expected: missing `WechatClient` methods fail compilation.

- [ ] **Step 3: Implement `WechatClient::request_trade_bill(date)`** using `merchant_id`, certificate serial, and merchant private key. The method signs with `WECHATPAY2-SHA256-RSA2048`, requests `tar_type=GZIP`, validates the JSON envelope, and maps `NO_STATEMENT_EXIST`, `STATEMENT_CREATING`, `SIGN_ERROR`, and `FREQUENCY_LIMITED` into safe error codes.
- [ ] **Step 4: Implement `WechatClient::download_and_verify(url, expected_sha1)`.** Require HTTPS and the documented WeChat API host, sign the download request, cap compressed and decompressed bodies at 32 MiB, decompress in memory, compare the SHA1 of the uncompressed bytes, then zeroize and discard the file bytes.
- [ ] **Step 5: Run WeChat tests and verify no result exposes the URL or bill body.**

Run: `cargo test -p ledger-tauri-ui connectors::payments::tests::wechat`

Expected: matching SHA1 returns `hash_verified: Some(true)`; invalid signatures, URL hosts, oversized bodies, bad GZIP and hash mismatch return safe errors.

### Task 3: Implement Alipay merchant bill read

**Files:** `connectors/payments/alipay.rs`, `connectors/payments/tests.rs`.

- [ ] **Step 1: Add local-server tests** for RSA2 signed bill-url lookup, provider error responses, HTTPS scheme upgrade for the documented HTTP sample, rejection of untrusted hosts and redirects, download bounds, and credential-safe errors.
- [ ] **Step 2: Run the Alipay tests and confirm failure.**

Run: `cargo test -p ledger-tauri-ui connectors::payments::tests::alipay`

Expected: missing `AlipayClient` methods fail compilation.

- [ ] **Step 3: Implement `AlipayClient::request_bill_url(date)`** for `alipay.data.dataservice.bill.downloadurl.query`, constructing `app_id`, timestamp, version, `sign_type=RSA2`, `biz_content`, and the RSA2 signature from the configured app private key. Verify the signed response with the configured Alipay public key before trusting the returned URL.
- [ ] **Step 4: Implement bounded HTTPS bill download.** Accept only the official `dwbillcenter.alipay.com` host, upgrade the documented `http` URL to `https` before any request, reject other hosts, credentials, ports, and redirects, and return a safe `secure_download_unsupported` error if HTTPS is unavailable. Never follow or retry over HTTP. Keep file bytes in zeroizing memory and return date and byte count only; do not parse transaction rows or write to SQLite.
- [ ] **Step 5: Run Alipay tests.**

Run: `cargo test -p ledger-tauri-ui connectors::payments::tests::alipay`

Expected: valid signed fixtures succeed; bad signatures, invalid hosts, provider errors and oversized files fail without exposing the raw response.

### Task 4: Add payment commands and Settings UI

**Files:** `payments/mod.rs`, `lib.rs`, `features/connections/PaymentConnectionsPage.tsx`, `PaymentProviderCard.tsx`, `Navigation.tsx`, `App.tsx`, `paymentConnections.css`, `styles/index.css`.

- [ ] **Step 1: Add failing Playwright coverage** for navigating Settings → Platforms and Collection → Payment API, entering both providers' credentials, saving masked state, choosing a bill date, testing success/error states, clearing the form, and disconnecting credentials.
- [ ] **Step 2: Add IPC fixture behavior** in `tests/helpers/workflow-ipc.mjs` for status/save/remove/test commands. Make the test fixture record command names and reject unexpected write-payment commands.
- [ ] **Step 3: Implement the shared `connections_test(request)` command** for WeChat Pay and Alipay. Call the ledger privacy guard, release the ledger mutex before storage or network work, load zeroizing credentials, run the provider adapter on a blocking worker, and return the shared `ConnectionTestResult` with only a `BillRead` summary or safe error code. Return a safe unsupported outcome for market providers until the market plan extends this same dispatcher.
- [ ] **Step 4: Add `connections` under the existing `sources` settings directory** and render the two payment provider cards. Require all credential fields before save; clear secret inputs after save; mask configured values; provide a date input for bill reads; show provider-specific safe status text.
- [ ] **Step 5: Run the payment UI test and TypeScript build.**

Run: `node --test --test-concurrency=1 apps/tauri-ui/tests/payment-api-connections.test.mjs`

Run: `npm --prefix apps/tauri-ui run build`

Expected: the payment API settings route works without changing current bill import, notification capture, account, or ledger flows.

### Task 5: Integrate checks and commit payment adapters

- [ ] **Step 1: Run Rust payment tests.**

Run: `cargo test -p ledger-tauri-ui connectors::payments`

- [ ] **Step 2: Run focused UI and formatting checks.**

Run: `node --test --test-concurrency=1 apps/tauri-ui/tests/payment-api-connections.test.mjs`

Run: `npm --prefix apps/tauri-ui run format:check`

- [ ] **Step 3: Regenerate and check directory indices.**

Run: `python scripts/update_tree.py`

Run: `python scripts/update_tree.py --check`

## Payment Acceptance

- Valid credentials can request and download a selected merchant bill; no endpoint can create or refund a payment.
- WeChat bill content is accepted only after SHA1 verification over the uncompressed bytes.
- Payment credentials, signatures, temporary URLs, and bill rows are absent from UI DTOs, logs, backups, and persistent ledger storage.
- `cargo test -p ledger-tauri-ui connectors::payments`, the focused Playwright test, TypeScript build, formatting check, and tree check pass.
