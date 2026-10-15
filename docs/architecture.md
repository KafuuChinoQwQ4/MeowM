# 架构边界

- `ledger-domain` 定义金额、账户、交易及不变量，不依赖平台。
- `ledger-app` 编排账户、交易、分类和报表用例，通过 ports 访问持久层。
- `ledger-storage` 将账本保存在本机 SQLite。
- `ledger-import` 处理微信/支付宝账单预览、标准化和去重。
- `ledger-notifications` 解析 Android 适配层送来的通知事件。
- `ledger-backup` 管理本地加密备份和恢复。
- `apps/tauri-ui` 使用 React + TypeScript，通过 `src/api/` 中按业务划分的接口调用 Tauri Rust 命令。
- 前端 `src/app/` 管理导航、当前平台和日期；`src/features/` 按平台、账本、交易、待确认和导入拆分完整流程；`src/components/` 复用弹窗和表单，`src/shared/` 复用金额、日期和请求生命周期，`src/styles/` 按界面职责拆分样式。组件不直接调用 invoke。
- `src-tauri/src/ledger/` 按账户、每日汇总、查询、导入预览、入箱和待确认操作拆分命令实现；`transactions.rs` 编排交易详情、编辑和撤销，测试独立放在子模块。
- `ledger-storage/src/sqlite/` 按连接、迁移、账户、账务仓储、分类、查询、导入、撤销、写入和行映射拆分。SQLite 连接保持私有，原子操作仍在同一数据库事务中完成；本次重构不改变 IPC 名称、公开仓储接口或数据库版本。
- `apps/tauri-ui/src-tauri/gen/android` 提供 Tauri Android WebView 壳，入口为 `MainActivity`，沿用应用 ID `com.example.localledger`。旧 Slint 界面和 Android JNI 壳已移除。

每个目录使用自己的 `tree` 说明直接文件。运行 `python3 scripts/update_tree.py` 更新，运行 `python3 scripts/update_tree.py --check` 检查。
