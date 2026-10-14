# 架构边界

- `ledger-domain` 定义金额、账户、交易及不变量，不依赖平台。
- `ledger-app` 编排账户、交易、分类和报表用例，通过 ports 访问持久层。
- `ledger-storage` 将账本保存在本机 SQLite。
- `ledger-import` 处理微信/支付宝账单预览、标准化和去重。
- `ledger-notifications` 解析 Android 适配层送来的通知事件。
- `ledger-backup` 管理本地加密备份和恢复。
- `apps/tauri-ui` 提供 TypeScript 界面，通过 Tauri invoke 调用 Rust 命令；`src-tauri/src/ledger.rs` 将命令接到真实 SQLite 和账务用例。
- `apps/tauri-ui/src-tauri/gen/android` 提供 Tauri Android WebView 壳，入口为 `MainActivity`，沿用应用 ID `com.example.localledger`。旧 Slint 界面和 Android JNI 壳已移除。

每个目录使用自己的 `tree` 说明直接文件。运行 `python3 scripts/update_tree.py` 更新，运行 `python3 scripts/update_tree.py --check` 检查。
