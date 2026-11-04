# OKX Read and Trading Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show public OKX markets without credentials, preserve optional private account reads, and add explicitly confirmed spot/SWAP/FUTURES order workflows in simulated and production environments.

**Architecture:** Keep the existing read-only credential group, add separate simulated and production trade credential groups in the native keyring, and extend the OKX client with signed fixed-path POST requests. Tauri commands validate account context and product rules; React previews and confirms each order before sending it.

**Tech Stack:** Rust, Tauri, React/TypeScript, SQLite schemas already at version 15, native credential store, existing local HTTP and Playwright test harnesses.

**Spec:** `.superpowers/specs/2026-11-04-okx-trading-design.md`

## Global Constraints

- Public SPOT tickers and candles work without credentials.
- Read-only, demo-trading, and production-trading keys are distinct credential groups stored in the native credential store.
- Demo is the default environment; production requires an explicit switch and a second confirmation for every write.
- Supported order instruments are `SPOT`, `SWAP`, and `FUTURES`; supported order types are `market` and `limit`.
- The app never calls withdrawal, transfer, leverage-setting, position-mode-setting, or account-mode-setting endpoints.
- Every private HTTP path is fixed in Rust; IPC accepts typed request fields only, never an arbitrary path or method.
- Decimal prices and quantities stay strings and are validated against OKX tick/lot/minimum rules without JavaScript floating-point rounding.
- Order POSTs are never automatically retried; unclear results are queried by the same `clOrdId`.
- OKX order and crypto history data never enter fiat transactions, postings, budgets, reports, or pending fiat records.

---

### Task 1: Separate credentials and make public OKX available before setup

**Files:**
- Modify: `apps/tauri-ui/src-tauri/src/connectors/model.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/credential_store.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/credentials.rs`
- Modify: `apps/tauri-ui/src/api/connections.ts`
- Modify: `apps/tauri-ui/src/features/markets/MarketsPage.tsx`
- Create: `apps/tauri-ui/src/features/markets/okx/OkxCredentialSettings.tsx`
- Test: `apps/tauri-ui/src-tauri/src/connectors/credential_store_tests.rs`
- Test: `apps/tauri-ui/tests/okx-workflows.test.mjs`

**Interfaces:**
- `ProviderId` adds `okx_trade_demo` and `okx_trade_live`; `CredentialInput` accepts the existing `okx` read group and matching trade groups with `api_key`, `secret_key`, and `passphrase`.
- `CredentialService::load(provider)` returns only the credential group named by that provider; removing a trade group leaves the read group and the other environment untouched.
- `MarketsPage` renders `OkxWorkspace` for the OKX account route even when no key is configured, and passes three separate configured flags into its private/Trade panels.

- [ ] **Step 1: Add a failing key-group isolation test.** Save distinct read, demo, and live credentials; assert all three statuses are configured and that removing demo credentials leaves read/live configured.
- [ ] **Step 2: Run the targeted Rust test and confirm the missing variants cause failure.** Run `cargo test -p ledger-tauri-ui credential_store_tests -- --nocapture` from `apps/tauri-ui/src-tauri`.
- [ ] **Step 3: Add provider IDs and typed credential variants.** Add required fields for each group and keep each group under its own keyring account prefix; do not change the existing `okx` key names so saved read credentials continue to work.
- [ ] **Step 4: Add a failing no-key UI workflow test.** Open OKX with every credential status false; assert the public ticker call and market tab appear, private tabs show a settings link, and neither `okx_place_order` nor credential input is invoked/rendered on the market tab.
- [ ] **Step 5: Run the focused UI test and confirm it fails because the current route redirects to credential setup.** Run `node --test tests/okx-workflows.test.mjs` from `apps/tauri-ui`.
- [ ] **Step 6: Implement public-first routing and a dedicated credential settings panel.** Keep read, demo, and live save/remove actions separate, mask values, and label required OKX permissions as Read or Read+Trade without asking for Withdraw.
- [ ] **Step 7: Re-run credential and focused UI tests.** Run `cargo test -p ledger-tauri-ui credential_store_tests -- --nocapture` and `node --test tests/okx-workflows.test.mjs`.

### Task 2: Extend the OKX client for signed order requests

**Files:**
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/model.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/tests.rs`

**Interfaces:**
- `OkxEnvironment` is a Rust enum with `Demo` and `Production`; only `Demo` adds `x-simulated-trading: 1`.
- `OkxClient::post<T>(path, body, credentials, environment)` accepts a compile-time fixed path from the order module, serializes a typed body once, and signs the exact sent bytes as timestamp + `POST` + request path + body.
- API envelopes preserve per-order `sCode`, `sMsg`, `ordId`, and `clOrdId`; acknowledgment is not treated as a fill.

- [ ] **Step 1: Add a failing local-server signature test.** Send a typed order body to `/api/v5/trade/order`; assert the request body bytes and HMAC prehash match exactly and the Demo header is present only in Demo mode.
- [ ] **Step 2: Run the focused test and confirm `post` is missing.** Run `cargo test -p ledger-tauri-ui signed_order_post_uses_exact_body_and_demo_header -- --nocapture` from `apps/tauri-ui/src-tauri`.
- [ ] **Step 3: Refactor the client around a shared bounded response parser.** Keep existing GET query signing unchanged; add POST body signing, the Demo header, JSON content type, and safe error mapping for HTTP and OKX envelopes.
- [ ] **Step 4: Add negative protocol tests.** Prove public GETs never carry credential headers; nonzero exchange codes and per-order `sCode` never become “filled”; oversized/malformed responses fail closed.
- [ ] **Step 5: Run all OKX connector tests.** Run `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture` from `apps/tauri-ui/src-tauri`.

### Task 3: Add validated order models and account/product context

**Files:**
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/model.rs`
- Create: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/order_validation.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/tests.rs`

**Interfaces:**
- `OrderDraft` contains `environment`, `inst_type`, `inst_id`, `side`, `ord_type`, `size`, optional `price`, optional `slippage_pct`, optional `td_mode`, optional `position_action`, and `cl_ord_id`; Serde rejects unknown fields.
- `InstrumentSpec` contains validated `inst_id`, `state`, `tick_sz`, `lot_sz`, `min_sz`, `ct_val`, `ct_val_ccy`, `ct_type`, and currencies needed for preview.
- `AccountTradeContext` contains `acct_lv`, `pos_mode`, current positions, pending orders, and per-instrument leverage when the endpoint returns it.
- `ValidatedOrder::from_draft(draft, spec, context)` computes a fixed OKX order body only after checking instrument type/state, decimal precision, minimum size, direction, position mode, and available close size.

- [ ] **Step 1: Write failing table-driven validation tests.** Cover spot cash orders, net-mode reduce-only close, long/short open and close side mappings, rejection of wrong instrument type, suspended/post-only state, off-tick price, off-lot/minimum size, missing limit price, invalid slippage precision, and a close larger than remaining position after pending close orders.
- [ ] **Step 2: Run the model tests and confirm they fail because the order types do not exist.** Run `cargo test -p ledger-tauri-ui order_validation -- --nocapture` from `apps/tauri-ui/src-tauri`.
- [ ] **Step 3: Add public product metadata and private account-context readers.** Read `/api/v5/public/instruments`, `/api/v5/account/config`, `/api/v5/account/positions`, `/api/v5/trade/orders-pending`, and `/api/v5/account/leverage-info` through typed response models and existing bounded GET.
- [ ] **Step 4: Implement exact decimal and mode validation.** Spot market buy uses received base currency; spot market sell uses received quote currency; set matching `tgtCcy`, enforce 0–5% slippage with at most two decimal places, and use the documented 1% UI default. Contract quantity uses contract `lotSz` and `minSz`; limit price uses `tickSz`; no floats or account-setting writes.
- [ ] **Step 5: Re-run focused validation and connector tests.** Run `cargo test -p ledger-tauri-ui order_validation -- --nocapture` and `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture`.

### Task 4: Add fixed order, query, history, and cancel commands

**Files:**
- Create: `apps/tauri-ui/src-tauri/src/connectors/markets/okx/orders.rs`
- Modify: `apps/tauri-ui/src-tauri/src/crypto/commands.rs`
- Modify: `apps/tauri-ui/src-tauri/src/crypto/mod.rs`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`
- Modify: `apps/tauri-ui/src-tauri/src/crypto/tests.rs`

**Interfaces:**
- `okx_trade_context(inst_type, inst_id, environment)` returns account mode, selected product rules, applicable positions, pending orders, and leverage display values.
- `okx_place_order(draft)` loads only the selected demo/live key, validates current context, and sends one POST to `/api/v5/trade/order`; it never retries a POST.
- `okx_order_detail(inst_id, cl_ord_id, environment)`, `okx_order_history(inst_type, environment)`, and `okx_cancel_order(inst_id, ord_id, environment)` use fixed GET/POST paths and the same environment credential group.

- [x] **Step 1: Add failing command/API request tests.** Assert missing mode-specific credentials block writes, an invalid draft causes no HTTP request, a valid draft generates one request, and timeout returns an unknown-result error containing the same client order ID without a second POST.
- [x] **Step 2: Implement command credential selection and Tauri registration.** Re-check the ledger unlocked state before exchange calls; perform network requests on `spawn_blocking` without holding the ledger mutex; avoid writing order data to SQLite.
- [x] **Step 3: Implement order lookup and cancellation.** Query by `instId` plus `clOrdId` to reconcile an unclear submit; cancel only by a validated `instId` and explicit exchange order ID; return OKX state rather than assuming success.
- [x] **Step 4: Test fixed-path enforcement.** Submit malicious `inst_id`, `cl_ord_id`, method-like strings, and path separators; assert validation rejects them and no request escapes the three supported instrument groups/fixed endpoints.
- [x] **Step 5: Run Rust crypto and connector tests.** Run `cargo test -p ledger-tauri-ui crypto -- --nocapture` and `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture` from `apps/tauri-ui/src-tauri`.

### Task 5: Build the public/private/trading React workflow

**Files:**
- Modify: `apps/tauri-ui/src/api/connections.ts`
- Modify: `apps/tauri-ui/src/api/crypto.ts`
- Modify: `apps/tauri-ui/src/features/markets/MarketsPage.tsx`
- Modify: `apps/tauri-ui/src/features/markets/okx/OkxWorkspace.tsx`
- Create: `apps/tauri-ui/src/features/markets/okx/TradingPanel.tsx`
- Create: `apps/tauri-ui/src/features/markets/okx/OrderReviewDialog.tsx`
- Modify: `apps/tauri-ui/src/styles/okx.css`
- Modify: `apps/tauri-ui/tests/okx-workflows.test.mjs`

**Interfaces:**
- `cryptoApi.tradeContext(instType, instId, environment)`, `cryptoApi.placeOrder(draft)`, `cryptoApi.orderDetail(instId, clOrdId, environment)`, `cryptoApi.orderHistory(instType, environment)`, and `cryptoApi.cancelOrder(instId, ordId, environment)` map one-to-one to the Tauri commands.
- `TradingPanel` defaults to Demo, shows production mode with a distinct warning, and never calls `placeOrder` until the user confirms `OrderReviewDialog`.
- Order rows use OKX states `live`, `partially_filled`, `filled`, and `canceled`; cancel is available only for incomplete orders.

- [ ] **Step 1: Add failing Playwright workflow tests.** Verify opening OKX with no credentials displays public markets; private tabs link to setup; trade defaults to Demo; live mode requires its own key and shows a warning; preview does not invoke `okx_place_order`; confirmation invokes it exactly once with correct spot/contract fields.
- [ ] **Step 2: Add failing order-state tests.** Verify ambiguous timeout preserves `clOrdId` and blocks a second submission until lookup; open orders refresh; cancellation requires confirmation and only targets the selected order.
- [ ] **Step 3: Implement typed frontend IPC and transaction form.** Keep values as decimal strings, reset the preview when any field changes, and fetch fresh trade context before preview and again in Rust before submit.
- [ ] **Step 4: Implement the accessible review dialog and environment distinction.** Render the exact order request and estimate, use a focus-managed dialog, and require a deliberate second action; show an unknown result distinctly from rejected/accepted/filled.
- [ ] **Step 5: Run `node --test tests/okx-workflows.test.mjs` and `npm run build` from `apps/tauri-ui`.** Confirm tests prove no order POST occurs on route entry, draft changes, or preview.

### Task 6: Update documentation and verify the full change

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `scripts/tree_descriptions.json`
- Regenerate: affected `tree` files with `python3 scripts/update_tree.py`
- Test: `apps/tauri-ui/tests/provider-connections.test.mjs`, `apps/tauri-ui/tests/okx-workflows.test.mjs`

- [ ] **Step 1: Document permissions and supported behavior.** State that public quotes need no key, private reads need a user-provided Read key, order submission uses separate Demo/Trade keys, Withdraw is not used, and demo is the default.
- [ ] **Step 2: Update direct-file descriptions and generated indexes.** Add descriptions for every new Rust/React/test/spec/plan file; run `python3 scripts/update_tree.py` and `python3 scripts/update_tree.py --check`.
- [ ] **Step 3: Run focused Rust and frontend validation.** Run `cargo fmt --all -- --check`, `cargo test --workspace`, `npm run build`, `npm run test:ui`, `npm run format:check`, `python3 scripts/update_tree.py --check`, and `git diff --check`.
- [ ] **Step 4: Review the diff for safety invariants.** Confirm no credentials are hard-coded, no Withdraw/transfer/account-setting path exists, no automatic POST retry exists, and no order data reaches fiat ledger commands.
- [ ] **Step 5: Report the exact validation results and state that real-account orders were not submitted during testing.**
