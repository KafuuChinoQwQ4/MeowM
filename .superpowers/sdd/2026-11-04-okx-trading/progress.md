# SDD ledger — plan: .superpowers/plans/2026-11-04-okx-trading.md

## Workspace and baseline

- Workspace: `/root/code/MeowM/.worktrees/okx-trading-20261104` on branch `feat/okx-trading`.
- This branch was created from `main` HEAD `999af38`, then the original checkout's 56-file dirty/untracked snapshot was copied into it. Keep that inherited snapshot intact and stage only implementation hunks for each task.
- Spec: `.superpowers/specs/2026-11-04-okx-trading-design.md`.
- Plan: `.superpowers/plans/2026-11-04-okx-trading.md`.
- No earlier ledger exists for this plan.

## Preflight scan

| Pair/task | Shared files or interface | Producer / consumer check | Result |
|---|---|---|---|
| Task 1 ↔ Task 5 | `MarketsPage.tsx`, `connections.ts`, `OkxWorkspace` flags | Task 1 exposes separate read/demo/live credential states and public-first route; Task 5 consumes those to gate trade actions. | Compatible; execute Task 1 before Task 5. |
| Task 2 ↔ Task 3 | `connectors/markets/okx.rs`, OKX model/tests | Task 2 adds shared signed POST/environment transport; Task 3 adds typed product/account/order models consumed by that transport. | Compatible; preserve the GET path while adding POST, execute Task 2 first. |
| Task 2 ↔ Task 4 | OKX client POST transport | Task 4's fixed write/query calls consume Task 2's signed client. | Compatible; execute Task 2 before Task 4. |
| Task 3 ↔ Task 4 | `order_validation.rs`, typed order body | Task 3's `ValidatedOrder` and fixed serialized body are consumed by Task 4's command/service. | Compatible; execute Task 3 before Task 4. |
| Task 4 ↔ Task 5 | Tauri command names and IPC DTOs | Task 4 registers typed context/place/detail/history/cancel commands; Task 5 maps one-to-one through `cryptoApi`. | Compatible; freeze exact IPC field names after Task 4. |
| Task 5 ↔ Task 6 | New frontend files and generated directory descriptions | Task 5 creates UI files; Task 6 documents and indexes the final names. | Compatible; execute Task 6 last. |
| Task 1 | Credential model/store and existing OKX UI route | Tests use existing native credential service and browser IPC harness; public-first route must keep old configured-credential behavior. | Internally consistent after package name correction (`ledger-tauri-ui`). |
| Task 2 | Existing GET-only HTTP test helper | POST body/HMAC tests require extending or adding a local HTTP helper; tests must assert sent bytes and environment header. | Internally consistent; helper expansion is within test file. |
| Task 3 | Product, account, position and order DTOs | Metadata is fetched from fixed endpoints and validated without float conversion; validation returns a typed fixed request body. | Internally consistent; exact response models may follow current OKX schema. |
| Task 4 | Tauri commands and exchange calls | Fixed-path functions make invalid drafts fail before network; timeout must return unknown status and must not repeat POST. | Internally consistent; local connector service test is the appropriate boundary if constructing Tauri State is impractical. |
| Task 5 | Browser workflow | Preview is local state only; confirmation is the sole call to `placeOrder`; ambiguous results retain `clOrdId` until queried. | Internally consistent. |
| Task 6 | README, architecture, trees, tests | Index descriptions are generated from the committed final file list. | Internally consistent. |

## Rulings

- Ruling: preserve the inherited dirty snapshot in the isolated worktree and stage implementation hunks only — this allows work on existing OKX code without contaminating or overwriting the user's original checkout; cost if wrong: baseline files may need manual restoration from the original checkout.
- Ruling: execute the approved subagent-driven workflow serially because Tasks 1–6 share APIs and files — this avoids conflicting edits; cost if wrong: slower than parallel work.

## Task progress

- Baseline: `node --test tests/okx-workflows.test.mjs` passed 5/5 after `npm ci`; Rust `cargo test -p ledger-tauri-ui credential_store_tests -- --nocapture` compiled successfully but selected 0 tests (actual tests are nested in `credential_store` module). Existing dead-code warnings are baseline warnings.
- Task 1: complete (commit `a582a8c`, review approved).
- Task 1: minor (deferred): keep the OKX credential settings view open after each group save so users can configure multiple groups without reopening it.
- Task 1: minor (deferred): show the recommended IP allowlist guidance for demo Trade keys as well as live Trade keys.
- Task 2: complete (commit `d846d97`, review approved).
- Task 2: minor (deferred until Task 4 consumes the interfaces): production-library dead-code warnings for POST transport and acknowledgement DTOs.
- Task 3: complete (`425f8f1` plus reviewed fixes `4bd598c`, `3f19d0e`; independent review approved).
- Task 3: deferred minor: spot market sell `sz` uses quote currency, so local base-currency `lotSz/minSz` cannot apply; UI labels the unit and OKX validates final minimum. Spot trade-context skips generic pending-size subtraction because `sz` and `accFillSz` may use different units.
- Task 3 review finding resolved: pending close accounting pages 100 rows, rejects overlap, and makes an empty sixth-page probe after 500 rows; any result or read error at the probe fails closed.
- Task 4: pending.
- Task 5: pending.
- Task 6: pending.
- Task 4: complete locally; commit and independent review pending. Report: `.superpowers/sdd/2026-11-04-okx-trading/task-4-report.md`.
- Task 4: local preflight rejects malformed draft shapes before network; product steps and close capacity are validated against fresh OKX metadata/context. A residual service error or malformed order acknowledgment yields `unknown` plus `clOrdId`; known per-order `sCode` is returned as rejected with its message.
- Task 4: live and demo credential isolation, one POST, unknown reconciliation, fixed detail/history/cancel endpoints, and malformed ID rejection have local-server tests. No order data is persisted.
- Task 4 follow-up: all five commands recheck session state after background work, and pending rows expose validated `ord_id` plus `live`/`partially_filled` state; focused crypto/connector tests pass.
