# React migration and modularization implementation plan

> Execute with executing-plans in this session, using the primary agent only. Approved design: `../specs/2026-10-16-react-modularization-design.md`.

**Goal:** Replace imperative DOM rendering with React and separate business responsibilities without changing IPC or database schema. Push main and stop before Task 3.

## 1. Establish baseline and install React

- Run `cargo test --workspace`, UI build, browser tests and format tests before editing. Baseline: 97 Rust, 4 browser and 3 format tests pass.
- Install `react react-dom`, plus `@types/react @types/react-dom @vitejs/plugin-react@5` as development dependencies. Keep Vite 7 and commit the lockfile.
- Configure `jsx: react-jsx`, Vite `plugins: [react()]`, and `/src/main.tsx` as the HTML entry.

## 2. Separate API and shared lifecycle boundaries

- Move DTOs and IPC calls into `src/api/{accounts,ledger,entries,imports,transactions,client}.ts`. Use a typed `TransactionWrite` request and one category API.
- Retain integer money/date helpers in `src/shared/format.ts` and structured command error formatting in `src/shared/errors.ts`.
- Implement reusable Dialog, Field, SelectField, Form, Pager and request hooks. Close guards must cover Escape, browser back, backdrop and save-in-progress; clean up event listeners.

## 3. Migrate complete workflows

- Implement `app/App.tsx`, Navigation and Toolbar. Keep view/account/date selection at the shell; business requests and form drafts belong to features.
- Implement accounts list/filter/edit, ledger summary/reference, transaction list/detail/audit/editor/impact, pending filters/selection/actions/editor/duplicates, import file/preview/row-edit components.
- Preserve selectors used by existing browser behavior tests. Delete old imperative renderers after replacing all workflows.
- Add workflow browser coverage for account/reference persistence, import revision, close lifecycle and transaction reversal. Run existing tests during migration and resolve behavioral failures before continuing.
- Separate CSS by existing semantic sections; retain appearance and mobile layout. Normal formatted modules should remain under about 350 lines.

## 4. Split Rust adapters and SQLite implementation

- Move `ledger.rs` to `ledger/mod.rs`, exporting original DTO names. Extract account/day/query/import/pending/parse modules and existing tests, using restricted visibility.
- Move transaction tests out of production file.
- Move `sqlite.rs` into modules for connection, migrations, platform, core repository, classification, feature queries/pending, import, reversal/audit, writing and row mapping. Keep SQL transaction boundaries and public signatures unchanged.
- Run `cargo test --workspace`, `cargo fmt --all -- --check`, and targeted Clippy after splitting. No schema migration.

## 5. Review, document and publish

- Update architecture docs, tree descriptions and generated directory trees using `python3 scripts/update_tree.py` and `--check`.
- Run UI production build, all browser tests and format tests, Rust checks and `git diff --check`. Review module sizes and stale imports.
- Scan a tracked/unignored snapshot, staged changes, then all reachable history after commit using redacted gitleaks output.
- Commit and push authorized work to origin/main, verify remote SHA equals local HEAD. Leave later drafts in stash and stop.

## Execution result (2026-10-16)

- Steps 1–4 implemented by the primary agent. React 19 is installed with a committed lockfile; original imperative renderers are removed. Frontend production TS/TSX modules are at most 201 lines; the split SQLite modules are at most 300 lines.
- Existing IPC names, version checks, integer money behavior, schema version 5 and SQLite atomic operations are preserved. Later Task 3–9 drafts remain outside this change in the original stash.
- Passed: 97 workspace Rust tests, 9 browser workflow tests, 3 amount/calendar tests, frontend production build, Prettier check, Rust formatting, targeted application/storage and Tauri Clippy checks, and npm audit (zero vulnerabilities).
- Compared computed styles for ledger, records, platforms, pending and reference dialog at 430×820, 1024×768 and 320×568; no static visual-property differences after replacing the original CSS with feature styles. Custom-property whitespace and in-flight navigation animations were excluded from this comparison.
- Browser tests mock Tauri IPC; Rust integration tests use SQLite. Android packaging, physical-device WebView calls and real payment exports were not validated in this refactor.
- Architecture documentation and directory indexes updated. Publication follows the authorized main-branch push after credential scans; stop before the next product phase.
