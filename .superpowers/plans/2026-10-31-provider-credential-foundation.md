# Provider Credential Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a typed provider connection contract and native secure credential storage that later payment and market adapters can use.

**Architecture:** Keep credentials and outbound API work in Rust behind Tauri IPC. A typed credential service stores one credential field per native keyring entry, rejects provider/field mismatches, and exposes only configured status to React. Payment and market API behavior remains in the two dependent plans.

**Tech Stack:** Rust 2024, Tauri 2.11, `keyring-core` 1.0.1, Android native keyring store 1.0.0, Windows native keyring store 1.1.0, Apple native keyring store 1.0.2, zbus Secret Service keyring store 1.0.1, `zeroize`, React 19, TypeScript 5.9.

**Spec:** `.superpowers/specs/2026-10-31-readonly-api-integrations-design.md`

## Global Constraints

- External requests originate in Rust and pass only necessary results through typed Tauri IPC.
- The app remains local-only and adds no API server.
- The first release performs no ledger writes, payment mutations, trading, transfers, or withdrawals.
- Credentials use Android Keystore-backed or desktop OS credential storage; there is no plaintext-file or SQLite fallback.
- Credentials, signatures, access tokens, full bill URLs, and raw provider responses never enter backups, exports, logs, or returned UI DTOs.
- Provider HTTP requests use fixed HTTPS endpoints, bounded response bodies, and explicit timeouts.
- The existing staged and unstaged workspace changes are preserved; commits use `git commit --only` with explicit paths.

---

## File Map

- Create `apps/tauri-ui/src-tauri/src/connectors/mod.rs`, `model.rs`, `credentials.rs`, `credential_store.rs`, and `tests.rs` for provider types, secret handling, and storage.
- Modify `apps/tauri-ui/src-tauri/Cargo.toml` for target-specific keyring backends and `apps/tauri-ui/src-tauri/src/lib.rs` for managed state and IPC registration.
- Create `apps/tauri-ui/src/api/connections.ts` for typed connection commands.
- Update `scripts/tree_descriptions.json` for new tracked source files and regenerate indices with `python scripts/update_tree.py`.

### Task 1: Define provider and credential contracts

**Files:**

- Create: `apps/tauri-ui/src-tauri/src/connectors/mod.rs`
- Create: `apps/tauri-ui/src-tauri/src/connectors/model.rs`
- Create: `apps/tauri-ui/src-tauri/src/connectors/tests.rs`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`

**Interfaces:**

- `ProviderId`: `WechatPay`, `Alipay`, `Binance`, `Okx`, `Ifind`, serialized as `wechat_pay`, `alipay`, `binance`, `okx`, `ifind`.
- `CredentialInput`: tagged input variants with these fields: WeChat (`merchant_id`, `certificate_serial`, `private_key_pem`); Alipay (`app_id`, `app_private_key_pem`, `alipay_public_key_pem`); Binance (`api_key`, `secret_key`); OKX (`api_key`, `secret_key`, `passphrase`); iFinD (`refresh_token`).
- `ConnectionStatus { provider: ProviderId, configured: bool }` contains no secret or account balance.
- `SensitiveString` owns a `Zeroizing<String>` and implements input deserialization without implementing a returned DTO path.
- `ConnectionTestRequest { provider, bill_date?, symbol? }` and `ConnectionTestResult { provider, outcome, summary?, error_code? }` are shared by all adapters. `ConnectionTestSummary` is a tagged union with `BillRead`, `BinanceBalances`, `OkxBalances`, and `IfindQuote`; `AssetBalance` and `IfindQuote` contain display-safe strings only.
- `connections_test(request)` is the single Tauri command for all provider test calls. The payment plan registers payment cases first; the market plan extends the same dispatch with market cases.

- [ ] **Step 1: Write contract tests** for all provider IDs, required credential fields, provider/input mismatch rejection, status serialization that contains no sample secret, and serialization of each shared test-result variant without credentials or raw payloads.

```rust
#[test]
fn public_status_never_contains_provider_secrets() {
    let status = ConnectionStatus { provider: ProviderId::Binance, configured: true };
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, r#"{"provider":"binance","configured":true}"#);
    assert!(!json.contains("sample-secret"));
}
```

- [ ] **Step 2: Run the focused tests and confirm they fail** because the contract types and validation functions do not exist.

Run: `cargo test -p ledger-tauri-ui connectors::tests`

Expected: compilation fails on the missing `connectors::model` types.

- [ ] **Step 3: Implement the provider enums, `SensitiveString`, tagged `CredentialInput`, and required-field validation.** Add `mod connectors;` and `#[cfg(test)]` test wiring without changing existing modules.

- [ ] **Step 4: Run the focused tests and confirm they pass.**

Run: `cargo test -p ledger-tauri-ui connectors::tests`

Expected: all provider contract tests pass and serialization of `ConnectionStatus` contains only provider ID and configured state.

### Task 2: Implement native credential storage

**Files:**

- Create: `apps/tauri-ui/src-tauri/src/connectors/credential_store.rs`
- Create or extend: `apps/tauri-ui/src-tauri/src/connectors/tests.rs`
- Modify: `apps/tauri-ui/src-tauri/Cargo.toml`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`

**Interfaces:**

- `CredentialField`: a closed enum for each provider-specific field listed in `CredentialInput`.
- `CredentialStore: Send + Sync`: `contains(provider, field)`, `get(provider, field) -> Result<Option<Zeroizing<String>>>`, `set(provider, field, value: &SensitiveString)`, and `delete(provider, field)`.
- `NativeCredentialStore` uses `keyring-core` with Android Keystore-backed storage on Android, Windows Credential Manager on Windows, Apple Keychain on macOS, and Secret Service on Linux. Backend initialization failure returns an error and never selects a file-based fallback.
- `MemoryCredentialStore` is test-only and injected into service tests.

- [ ] **Step 1: Write failing service tests** for provider/field isolation, overwrite, deletion, absent entries, and rollback when a multi-field save fails midway.

```rust
#[test]
fn credentials_are_isolated_and_remove_clears_every_field() {
    let store = MemoryCredentialStore::default();
    store.set(ProviderId::Binance, CredentialField::SecretKey, secret("binance-secret")).unwrap();
    assert!(store.get(ProviderId::Binance, CredentialField::SecretKey).unwrap().is_some());
    assert!(store.get(ProviderId::Okx, CredentialField::SecretKey).unwrap().is_none());
    remove_provider(&store, ProviderId::Binance).unwrap();
    assert!(store.get(ProviderId::Binance, CredentialField::SecretKey).unwrap().is_none());
}
```

- [ ] **Step 2: Run the focused tests and confirm the credential service is missing.**

Run: `cargo test -p ledger-tauri-ui connectors::credential_store`

Expected: test compilation fails because `CredentialStore` and `MemoryCredentialStore` are not defined.

- [ ] **Step 3: Add the keyring ecosystem dependencies by target and implement `NativeCredentialStore`.** Store each UTF-8 secret field under service `com.example.localledger.providers` and a stable provider/field account name. Use `Zeroizing` for loaded values and serialization buffers. On Linux, an unavailable Secret Service is an explicit `secure_storage_unavailable` error.

- [ ] **Step 4: Implement grouped save with rollback.** Read prior values into zeroizing buffers, write required fields in order, and restore prior values (or delete newly created ones) if any write fails. Deleting a provider removes all of its field entries.

- [ ] **Step 5: Run storage tests and compile the desktop target.**

Run: `cargo test -p ledger-tauri-ui connectors::credential_store`

Run: `cargo check -p ledger-tauri-ui`

Expected: memory-backed tests pass; the application compiles without initializing or accessing a user's OS keyring during tests.

### Task 3: Expose guarded credential management commands

**Files:**

- Create or modify: `apps/tauri-ui/src-tauri/src/connectors/credentials.rs`
- Modify: `apps/tauri-ui/src-tauri/src/connectors/mod.rs`
- Modify: `apps/tauri-ui/src-tauri/src/lib.rs`
- Create: `apps/tauri-ui/src/api/connections.ts`
- Modify: `apps/tauri-ui/src/api/tree`

**Interfaces:**

- `ConnectorState { credentials: Arc<dyn CredentialStore> }` is managed during Tauri setup.
- Tauri commands: `provider_connections() -> Result<Vec<ConnectionStatus>>`, `save_provider_credentials(input: CredentialInput) -> Result<ConnectionStatus>`, and `remove_provider_credentials(provider: ProviderId) -> Result<ConnectionStatus>`.
- `connectionsApi.status()`, `.save(input)`, `.remove(provider)`, and `.test(request)` are typed frontend wrappers. `.test` serializes the shared DTO shape `ConnectionTestRequest { provider, billDate?, symbol? }` and returns `ConnectionTestResult { provider, outcome, summary?, errorCode? }`.

- [ ] **Step 1: Add service tests** proving status is based on the full required-field set, incomplete saves fail, removal clears state, and provider credential inputs never serialize as command results.

- [ ] **Step 2: Run the tests and confirm the command service is missing.**

Run: `cargo test -p ledger-tauri-ui connectors::credentials`

Expected: missing command/service symbols fail compilation.

- [ ] **Step 3: Implement credential command handlers.** Call the existing ledger `access` guard before credential operations, release the ledger mutex before accessing the keyring, return only `ConnectionStatus`, and add status/save/remove to `tauri::generate_handler!`. Register `connections_test` in the payment plan so there remains one shared dispatcher.

- [ ] **Step 4: Add `connections.ts` wrappers** with the provider input union and safe connection result DTOs. Do not export a function that reads a credential back.

- [ ] **Step 5: Run focused Rust and TypeScript validation.**

Run: `cargo test -p ledger-tauri-ui connectors::credentials`

Run: `npm --prefix apps/tauri-ui run build`

Expected: all command service tests pass and TypeScript accepts the typed wrapper without adding any UI network request.

- [ ] **Step 6: Update generated tree descriptions and check indices.**

Run: `python scripts/update_tree.py`

Run: `python scripts/update_tree.py --check`

Expected: both commands exit 0; only tree files required for the new source files change.

## Foundation Acceptance

- `cargo test -p ledger-tauri-ui connectors` passes with memory-backed storage.
- The app refuses to store credentials when the native secure store is unavailable.
- Provider status DTOs contain no credentials, and all commands require the existing app-unlocked guard.
- `npm --prefix apps/tauri-ui run build` and `python scripts/update_tree.py --check` pass.
