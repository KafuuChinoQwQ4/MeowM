# Task 3 report: OKX order validation and trade context

**Status:** DONE

Implemented strict order draft and product/account context models, plus a fixed OKX request body constructed only after validation. Spot market orders set the received-currency `tgtCcy`; default slippage is 1%, with exact decimal range and precision checks. Spot limit and contract sizes/prices use exact decimal step/minimum checks. Contract actions map to net-mode `reduceOnly` or hedge-mode `posSide`, and close capacity subtracts matching pending close remainder from the position.

Added bounded typed GET readers for public instruments, account configuration, positions, pending orders, and leverage. These are read-only. Demo private reads carry the demo header. Leverage records preserve margin mode and position side; an unavailable leverage read is omitted so optional display data cannot block the required trading context. Instrument metadata reads preserve non-live state for display, while order validation accepts only `live`.

## TDD evidence

- **RED:** `cargo test -p ledger-tauri-ui order_validation -- --nocapture` failed to compile because the requested order DTOs and `order_validation` module did not exist. This was the expected missing-feature failure.
- **RED:** `cargo test -p ledger-tauri-ui unavailable_optional_cross_leverage_does_not_block_trade_context -- --nocapture` failed with `ProviderFailure` when the optional cross leverage endpoint returned an error.
- **GREEN:** The validation filter passes after implementation. The full OKX connector filter passes, including the validation, product/context-reader, and optional-leverage tests.

## Verification

- `cargo test -p ledger-tauri-ui order_validation -- --nocapture` — passed.
- `cargo test -p ledger-tauri-ui connectors::markets::okx -- --nocapture` — 20 passed, 0 failed, 1 ignored (the ignored test requires a live public OKX endpoint).
- `cargo fmt --check` — passed.
- `git diff --check` — passed.

## Files changed

- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/model.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/order_validation.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx.rs`
- `apps/tauri-ui/src-tauri/src/connectors/markets/okx/tests.rs`

## Self-review notes

- Order IPC wiring and actual order submission are deferred to the next task as instructed. Tests use a local HTTP server; no live OKX account or real order was used.
- The Rust build reports dead-code warnings for the new connector API because the next task has not wired its production callers yet.
- Spot market sells use quote-currency size while OKX `lotSz`/`minSz` metadata is base-currency denominated. The validator checks that quote size is a positive exact decimal and leaves its final market minimum acceptance to OKX; applying base lot/min directly to quote units would be incorrect.
