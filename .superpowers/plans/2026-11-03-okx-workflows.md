# OKX Workflows Implementation Plan

> Execute inline in the shared checkout; user explicitly requested implementation of the supplied four-flow design. No publication or commit is requested.

**Goal:** Complete the four OKX read and review workflows without fiat accounting changes.
**Architecture:** Independent crypto domain values, SQLite records and watchlist, reusable signed OKX GET client, guarded Tauri commands, and React panels in MarketsPage.
**Tech Stack:** Rust, SQLite, Tauri, React/TypeScript, SVG, existing Playwright tests.
**Spec:** `.superpowers/specs/2026-11-03-okx-workflows.md`

## Global Constraints

- Keep credentials in the existing native credential store; use only fixed HTTPS GET endpoints.
- Preserve decimals as strings and use checked integer arithmetic for conversion legs.
- Never write crypto records into fiat transactions, postings, reports, or pending confirmation.
- Persist review status across reimport; explicitly expose pagination and pending upstream states.

## Task 1: Crypto domain and storage

Files: `crates/ledger-domain/src/crypto.rs`, `crates/ledger-storage/src/sqlite/crypto.rs`, migration `0015_crypto.sql`, snapshot schema/validation, backup version compatibility and focused tests.

- [x] Write and run failing tests for exact decimal multiplication and complete two-asset conversions.
- [x] Implement crypto values with validation and checked multiplication.
- [x] Write/run storage tests for immutable confirmed records, deduplication, invalid batch rollback, snapshot restore and full deletion.
- [x] Add schema 15, repository methods, snapshot validation and backup compatibility; update existing version-sensitive tests.
- [x] Run domain, storage and backup checks.

## Task 2: OKX provider and guarded commands

Files: `src-tauri/src/connectors/markets/okx*.rs`, `src-tauri/src/crypto/`, command registration and credentials accessor.

- [x] Add protocol tests proving public requests carry no credentials and signed query strings exactly match requested URLs.
- [x] Implement typed/validated ticker, candle, balance, valuation, history and position reads.
- [x] Add async IPC commands for public market data, private assets/positions/import, local watchlist/list/review.
- [x] Fetch stable UID before imports; page bounds and source IDs must match official API semantics.
- [x] Run connector and command integration tests.

## Task 3: User workflows

Files: `src/api/crypto.ts`, `src/features/markets/okx/`, `MarketsPage.tsx`, `src/styles/okx.css`, UI tests.

- [x] Add failing browser tests for public chart/watchlist, assets, conversion review and contract display.
- [x] Implement panels with loading/errors/retry, accessible candles, source/date/page controls, review filters and exact decimals.
- [x] Preserve provider credential cards, privacy masking and existing navigation.
- [x] Run focused browser tests and production build.

## Task 4: Review and verification

- [x] Review scope, stale requests, arithmetic, page handling, malformed provider responses and restore compatibility.
- [x] Update README, architecture and per-directory descriptions; regenerate/check tree files.
- [x] Run formatting, workspace tests and targeted UI tests; report validation limits for real accounts.

## Verification results

- `cargo test --workspace --quiet`: 322 passed, 2 ignored; the public OKX smoke test was also run explicitly and passed.
- Targeted UI tests (`okx-workflows`, `provider-connections`, `privacy`): 10 passed.
- Production frontend build, Prettier, cargo fmt, git diff whitespace and tree checks passed.
- Live public client read 1,212 tickers, 100 current candles and 100 history candles. This environment required clearing its unavailable ALL_PROXY for that command; production proxy configuration was not changed.
- Code review corrections cover tradeQuoteCcy, invalidation of outstanding imports across lock/restore/delete/credential replacement, workspace reset and UID-bound pagination.
- Real private-account queries and Android device integration require locally configured credentials and were not exercised.
