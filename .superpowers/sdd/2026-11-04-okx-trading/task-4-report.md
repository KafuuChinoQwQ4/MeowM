# Task 4 report: fixed OKX order commands

**Status:** DONE

Implemented the order service and five Tauri commands: trade context, place order, order detail by `clOrdId`, seven-day order history (bounded to 100 newest rows), and explicit single-order cancel. Demo commands load only `okx_trade_demo` and send the simulated header; production commands load only `okx_trade_live` and omit it. No command falls back to the read-only credential group or another environment.

Order drafts receive local syntax validation before any request, then use fresh public product metadata and private account context for tick/lot/minimum and close-capacity validation. The order POST uses one fixed path and is called once. A 2xx generic exchange failure, HTTP/server/network error, or malformed acknowledgment becomes `unknown` with the same client ID; known client-side/exchange errors remain rejected. Per-order `sCode` and `sMsg` are returned as a rejected acknowledgment. No successful acknowledgment is represented as a fill. Users can query that exact client ID to reconcile uncertainty.

Cancel accepts one validated instrument and numeric exchange order ID, calls only the fixed single-cancel endpoint, and returns accepted/rejected/unknown. Order requests and responses remain in memory and are not sent to SQLite or fiat ledger functions. Ledger unlocked/session state is checked before and after exchange work, with network requests on `spawn_blocking` and no ledger lock held during I/O.

## IPC contract for Task 5

Tauri command arguments use the established camelCase outer keys. `okx_place_order` takes a nested `draft`, whose Serde fields stay snake_case:

```json
{
  "draft": {
    "environment": "demo",
    "inst_type": "SPOT",
    "inst_id": "BTC-USDT",
    "side": "buy",
    "ord_type": "market",
    "size": "0.001",
    "price": null,
    "slippage_pct": null,
    "td_mode": "cash",
    "position_action": null,
    "cl_ord_id": "client123"
  }
}
```

Other argument shapes are `okx_trade_context({ instType, instId, environment })`, `okx_order_detail({ instId, clOrdId, environment })`, `okx_order_history({ instType, environment })`, and `okx_cancel_order({ instId, ordId, environment })`. The enum values are `demo`/`production`, `SPOT`/`SWAP`/`FUTURES`, `buy`/`sell`, `market`/`limit`, `cash`/`cross`/`isolated`, and `open_long`/`open_short`/`close_long`/`close_short`.

## TDD evidence

- **RED:** Initial service tests failed compilation because `orders` and `OkxOrderService` did not exist.
- **RED:** Invalid zero-size and malformed client-ID tests attempted to reach the local listener and returned `NetworkFailure`, proving they were not rejected before I/O. Preflight validation now rejects them before any request.
- **RED:** A generic top-level `50000` response returned `ProviderFailure` instead of an unknown outcome. It is now `Unknown` with the client ID preserved; a same-ID detail query reconciles it without a second order POST.
- **RED:** Order history accepted a response row labeled `SWAP` for a `SPOT` request; history validation now enforces the exact requested instrument type.
- **GREEN:** Focused regression tests pass for each corrected case, Demo/Live credential isolation, one-post submission, per-order rejection details, fixed detail/history/cancel paths, and malicious identifiers.

## Verification

- `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture` — 36 passed, 0 failed, 1 ignored (the existing test that requires live public OKX).
- `cargo test -p ledger-tauri-ui crypto -- --nocapture` — 1 passed, 0 failed.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- Local HTTP servers only. No Demo or Production account credentials were loaded and no exchange order was sent.

## Files changed

- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/orders.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/model.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/order_validation.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/tests.rs`
- `apps/tauri-ui/src-tauri/src/crypto/commands.rs`
- `apps/tauri-ui/src-tauri/src/crypto/mod.rs`
- `apps/tauri-ui/src-tauri/src/lib.rs`

The Rust build reports only the two pre-existing ledger dead-code warnings (`create_platform` and `save_entry`).
