# 架构边界

- `ledger-domain` 定义金额、账户、交易及不变量，不依赖平台。
- `ledger-app` 编排账户、交易、分类、报表、预算、周期和提醒用例，通过 ports 访问持久层。
- `ledger-storage` 将账本保存在本机 SQLite。
- `ledger-import` 处理微信/支付宝账单预览、标准化和去重。
- `ledger-notifications` 将受支持包名的内存事件解析为候选，经 `NotificationRepository` 原子写入 Pending、来源证据和去重收据；原始正文和通知 key 不持久化。
- `ledger-backup` 管理本地加密备份和恢复。
- `apps/tauri-ui` 使用 React + TypeScript，通过 `src/api/` 中按业务划分的接口调用 Tauri Rust 命令。
- 前端 `src/app/` 管理导航、当前平台和日期；`src/features/` 按平台、账本、交易、待确认、导入、报表、预算和周期拆分完整流程；`src/components/` 复用弹窗和表单，`src/shared/` 复用金额、日期和请求生命周期，`src/styles/` 按界面职责拆分样式。组件不直接调用 invoke。
- `src-tauri/src/ledger/` 按账户、每日汇总、查询、导入预览、入箱和待确认操作拆分命令实现；`transactions.rs` 编排交易详情、编辑和撤销，测试独立放在子模块。
- `src-tauri/src/analytics/` 按报表查询、预算管理和命令注册拆分，React 通过独立的 reports/budgets API 模块调用。`ledger-app` 负责本地日期边界、聚合与预算进度，SQLite 负责唯一范围及分类有效性的原子校验。提醒由预算状态派生稳定 ID，界面展示时不产生交易。
- `ledger-storage/src/sqlite/` 按连接、迁移、账户、账务仓储、分类、查询、导入、撤销、写入和行映射拆分。SQLite 连接保持私有，原子操作仍在同一数据库事务中完成；通知设置和收据由独立 `notifications.rs` 管理，迁移 6 新增去重收据与最近结果表。
- `apps/tauri-ui/src-tauri/gen/android` 提供 Tauri Android WebView 壳，入口为 `MainActivity`，沿用应用 ID `com.example.localledger`。旧 Slint 界面和 Android JNI 壳已移除。`NotificationCaptureService` 在有界后台队列中调用独立 JNI 边界，使用与 Tauri 相同的 `Context.dataDir/ledger.sqlite3`，不依赖 Activity 或 WebView 已启动。通知访问插件只处理系统授权状态及设置跳转，React 设置页、API 和样式分别独立成模块。

每个目录使用自己的 `tree` 说明直接文件。运行 `python3 scripts/update_tree.py` 更新，运行 `python3 scripts/update_tree.py --check` 检查。

- `ledger-domain/src/recurring.rs` 定义模板、日历周期、实例和提醒。`ledger-app/src/recurring/` 使用本地 `NaiveDate` 和原始锚点计算日历，不依赖 SQLite 或 WebView。
- `ledger-storage/src/sqlite/recurring/` 按行映射、模板写入与跳过、候选生成拆分。生成使用 SQLite immediate 事务，同时写入 Pending、`recurring` 来源证据、唯一实例收据及新游标；跳过也原子写收据与游标。模板版本避免过期编辑，实例确认状态直接关联交易状态，避免两套状态不一致。迁移 7 保留已有 ID，扩充来源类型并允许不关联交易的跳过收据。
- `src-tauri/src/recurring/` 按输入验证、DTO 映射和命令注册拆分；React `features/recurring/` 按资源刷新、表单、卡片、实例历史和首页提醒拆分。刷新生成后的通知只更新账本/收件箱查询，不触发无限补生成。提醒金额使用已生成候选快照，未来尚未生成的提醒使用当前模板。
