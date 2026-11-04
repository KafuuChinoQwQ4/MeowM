# Task 5 report: OKX order workflow UI

**Status:** DONE

Added a dedicated OKX trading tab with typed Tauri IPC wrappers for fresh trade context, order submission, reconciliation lookup, user-requested history, and single-order cancellation. Demo is selected by default; the live key is gated independently and production warnings appear in the form, review, and result. Spot market buy sizes use received base currency, spot market sells use received quote currency, and contract orders carry the selected action, margin mode, contract size, and mapped side. Financial values remain decimal strings; UI percentage input is converted by exact string arithmetic (`1%` becomes `0.01`).

Every draft preview first reloads product/account context and remains local until the accessible review dialog's deliberate confirmation. The confirmation event submits exactly once. Accepted orders are labeled as submitted, not filled; rejected responses preserve OKX `sCode`/`sMsg`; unknown responses preserve the same client ID and block another preview until the user checks the order detail and acknowledges the reported state. Pending rows use the validated `live`/`partially_filled` state and `ord_id`; cancellation names the selected instrument and order in a second confirmation and refreshes context after accepted/unknown results. Stale context, preview, history, and post-cancel responses are discarded when the user changes environment or instrument. No order data is written to fiat ledger or crypto history records.

The Task 4 frontend integration review exposed a serialization mismatch: Rust order acknowledgment and snapshot models deserialize OKX camel-case fields but also serialized them as camel case across Tauri IPC, while the frozen frontend contract is snake case. Their Serde fields now use distinct exchange deserialization and IPC serialization names; a Rust regression test checks both directions. This correction is included because Task 5 consumes those exact commands.

Independent review found that inverse derivative limit prices could be labeled with their settlement currency. The UI now uses the instrument quote currency or derives the quote segment from the instrument ID; for example, `BTC-USD-SWAP` displays USD even when the contract settles in BTC. Review also prompted explicit handling for uncertain cancellation results: further writes stay blocked until the user refreshes pending orders. The unknown-order browser flow now verifies that an empty lookup remains blocked until a later query finds the order.

## TDD evidence

- **RED:** `node --test --test-name-pattern='trading tab guides users' tests/okx-workflows.test.mjs` failed as expected because the `交易下单` tab did not exist (Playwright locator timeout).
- **RED:** The five initial trading browser workflows all failed on the missing tab before trade IPC could run.
- **RED:** Deferred-context browser cases reproduced stale review resurrection after a draft edit and stale Demo rows after switching to Production. Both pass with revision checks.
- **RED:** `order_ipc_models_keep_snake_case_while_deserializing_okx_field_names` first failed because an OKX acknowledgment serialized as `ordId`/`clOrdId`/`sCode`/`sMsg`. Separate Serde names now preserve OKX input fields and emit snake-case IPC.
- **GREEN:** Initial OKX browser workflow suite: 16 passed, 0 failed.
- **GREEN:** Review regressions for inverse USD quote units, an empty client-ID lookup, and uncertain cancellation reconciliation: OKX browser workflow suite 18 passed, 0 failed.

## Verification

- `node --test tests/okx-workflows.test.mjs` — 18 passed, 0 failed after review fixes.
- `npm run test:ui` — full browser suite 118 passed, 0 failed after review fixes.
- `npm run build` — passed.
- `npm run format:check` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture` — 39 passed, 0 failed, 1 ignored (live public OKX test).
- `cargo test -p ledger-tauri-ui crypto -- --nocapture` — 2 passed, 0 failed.
- `git diff --check` — passed.
- Rust build reports two existing dead-code warnings for `create_platform` and `save_entry`.

All exchange calls were browser-mocked or served by local Rust test servers. No demo or production credentials were used and no exchange order was sent.

## Files changed

- `apps/tauri-ui/src/api/crypto.ts`
- `apps/tauri-ui/src/features/markets/okx/OkxWorkspace.tsx`
- `apps/tauri-ui/src/features/markets/okx/TradingPanel.tsx`
- `apps/tauri-ui/src/features/markets/okx/OrderReviewDialog.tsx`
- `apps/tauri-ui/src/styles/okx.css`
- `apps/tauri-ui/tests/okx-workflows.test.mjs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/model.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/tests.rs`
