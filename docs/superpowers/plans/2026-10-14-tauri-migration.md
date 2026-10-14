# Tauri TypeScript migration

User approved replacing all previous frontend code with Tauri/TypeScript and clearing old SQLite data.

Architecture: bundled TypeScript/Vite frontend uses typed Tauri invoke commands. Rust application/domain/storage crates remain authoritative. New Android shell uses Tauri WebView, same application ID and debug signing identity for replacement installation.

- [x] Build responsive TypeScript UI and typed IPC contract with explicit account/date per request; no mock runtime.
- [x] Implement Rust commands, preserve integer money arithmetic, test default absent balance and 4th/5th/3rd date projection plus persistence.
- [x] Generate Android Tauri shell, remove Slint and obsolete Android shell/build integration.
- [x] Build frontend, check Rust, build ARM64 APK, install on connected phone.
- [x] Clear only old app SQLite data once as explicitly requested; no automatic destructive startup logic. Verify new empty ledger and persistence after restart.
- [x] Update documentation and directory indexes; review migration and generated artifacts.

Validation: TypeScript build; command integration tests with real SQLite; targeted UI checks at mobile width; Android install/start and actual WebView invokes. Preserve unrelated work in current dirty checkout. No worktree copy of this entire home-directory git repository.

2026-10-14 validation: x86_64 and ARM64 debug APKs built; emulator real WebView UI verified absent balance, 100 reference, forward 120, backward 85, restart persistence and clearing reference without deleting entries. Phone updated to 0.2.0 (versionCode 2); old files/ledger.sqlite3 backed up and removed. New ledger.sqlite3 contains three default accounts with NULL references and zero transactions. Rust interface tests 2/2 and TypeScript format tests 3/3 passed.
