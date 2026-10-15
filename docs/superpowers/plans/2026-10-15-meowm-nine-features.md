# MeowM 九项账本能力实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有离线优先账本上完成待确认、交易管理、报表预算、加密备份、通知自动采集、周期账单、搜索规则、账户对账和隐私控制九项能力，并让每项能力通过同一套 Rust 领域与 SQLite 约束接入 Tauri/Android 界面。

**Architecture:** `ledger-domain` 继续定义金额、账户、交易状态和不变量；`ledger-app` 编排待确认、交易、报表、周期、规则和对账用例；`ledger-storage` 负责版本化 SQLite 迁移和原子事务。Tauri TypeScript 只负责展示和用户操作，Android 原生层只负责通知权限、通知事件和文件选择，所有解析、去重、确认、余额和备份校验都回到 Rust。

**Tech Stack:** Rust 2024 workspace、SQLite/rusqlite、chrono、uuid、serde、Tauri 2、TypeScript、Vite、Android Kotlin/NotificationListenerService、现有整数最小货币单位模型。

**Spec:** `docs/superpowers/specs/2026-10-07-local-expense-manager-design.md`，以及现有阶段计划 `docs/superpowers/plans/2026-10-07-local-expense-manager-next-phases.md`。

## Global Constraints

- 第一版继续本地优先，不引入用户账号、服务端或云同步。
- 所有金额使用整数最小单位或定点十进制，禁止二进制浮点数。
- 自动采集和文件导入先进入 `Pending`；只有用户确认后才改变正式余额和报表。
- `confirm`、`ignore`、批量确认、恢复和重复通知处理必须幂等；重复操作不能产生重复交易。
- 通知正文和原始账单行不写入普通日志；持久化来源证据只保留必要的标准化字段摘要和摘要哈希。
- 所有批量写入使用 SQLite 事务；任意校验失败都回滚整批操作。
- 余额、报表、预算和对账只计算 `Confirmed` 交易，`Pending` 与 `Ignored` 必须明确显示状态。
- 本地日期边界统一由 Rust 根据设备时区转换为半开 UTC 区间；跨月份、夏令时和重启后结果必须一致。
- 继续保留现有数据库和平台 ID；新增迁移必须兼容 schema version 3，不能清空现有流水。
- 新增或删除文件后运行 `python3 scripts/update_tree.py` 和 `python3 scripts/update_tree.py --check`；不得把构建产物、真实账单、通知正文、Token 或密码写进仓库。
- 每个任务先写失败测试，再写最小实现；每个任务完成后运行目标测试并提交独立 commit。

## Current Baseline and Dependency Order

现有 `ledger-domain`、`ledger-app`、`ledger-storage` 已有账户、交易、转账、退款、分类、规则和报表基础；`ledger-import`、`ledger-notifications`、`ledger-backup` 已有职责骨架；`apps/tauri-ui` 已接通每日手动收支和平台管理。现行界面入口是 `apps/tauri-ui/src/api.ts`、`apps/tauri-ui/src/main.ts` 和 `apps/tauri-ui/src-tauri/src/ledger.rs`，不能按旧的 Slint 或独立 Android 项目路径实施。

按以下顺序执行：

1. 任务 0：统一数据模型、迁移和错误/IPC 契约。
2. 任务 1：待确认收件箱和导入预览。
3. 任务 2：交易详情、编辑、撤销、退款和转账关联。
4. 任务 3：月度报表、预算和提醒。
5. 任务 4：Android 通知自动采集。
6. 任务 5：周期性账单和到期提醒。
7. 任务 6：搜索、商户归一化和分类规则界面。
8. 任务 7：账户对账。
9. 任务 8：加密备份、恢复和数据导出。
10. 任务 9：隐私控制和数据生命周期。
11. 任务 10：端到端验收、迁移回放、Android 真机验证和发布文档。

任务 1 至 9 之间的 UI 可以并行设计，但写入数据库的迁移按任务 0 一次合并，避免多个迁移编号冲突。

---

### Task 0: 统一数据模型、迁移和跨层契约

**Files:**

- Modify: `crates/ledger-domain/src/lib.rs`, `transaction.rs`, `category.rs`, `account.rs`
- Modify: `crates/ledger-app/src/ports.rs`, `transactions.rs`, `reports.rs`, `classification.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Create: `crates/ledger-storage/migrations/0004_nine_features.sql`
- Modify: `apps/tauri-ui/src-tauri/src/ledger.rs`, `apps/tauri-ui/src-tauri/src/lib.rs`, `apps/tauri-ui/src/api.ts`
- Test: `crates/ledger-storage/tests/sqlite_repository.rs`, `crates/ledger-app/tests/ledger_workflows.rs`

**Interfaces:**

- `TransactionStatus::{Pending, Confirmed, Ignored}` 保持兼容；新增撤销关系、更新时间和编辑版本字段，撤销本身是新的 `BalanceAdjustment` 或反向交易，不能物理删除原记录。
- 新增 `PendingQuery`、`PendingCandidate`、`BulkConfirmation`、`TransactionEdit`、`Budget`、`RecurringTemplate`、`ReconciliationSession`、`PrivacySettings` 和 `AuditEvent` 的领域类型，所有 ID 使用现有 UUID 包装类型。
- `LedgerRepository` 增加按状态、日期、账户、分类、商户和关键词查询的接口，以及原子批量确认、原子撤销和导入/通知来源证据查询接口。
- Tauri 命令统一返回可序列化的 DTO 和用户可读错误；前端 API 不直接暴露 SQLite 行对象。

- [x] **Step 1: 写迁移失败测试**：从 schema version 3 打开数据库，断言新表和索引存在，旧账户、流水、分类和来源证据数量及 ID 不变；重复打开不重复迁移。
- [x] **Step 2: 写领域不变量测试**：Pending 不进入余额；撤销交易必须引用已确认原交易；预算金额非负；周期模板不能生成重复实例；对账差异使用整数分。
- [x] **Step 3: 实现迁移和基础类型**：新增 `pending_review` 查询索引、预算表、周期模板/实例表、对账表、通知设置表、应用设置表和审计事件表；为编辑/撤销、规则、来源和备份保留外键。
- [x] **Step 4: 扩展 repository/application ports**：为后续任务提供精确的查询、批量事务和设置读写接口；所有新写接口在同一 SQLite transaction 中完成校验和写入。
- [x] **Step 5: 暴露最小 Tauri DTO**：增加分页、过滤、状态和错误码字段，确保大账本不会把所有原始记录一次性传到 WebView。
- [x] **Step 6: 运行 `cargo test -p ledger-storage -p ledger-app`**，确认旧测试和迁移回放测试通过。
- [x] **Step 7: Commit**：`feat: define nine ledger feature contracts`。

**任务 0 完成记录：** 提交 `7bdf86d`、`1d0ea68`；schema 3/4 迁移、批量回滚、撤销抵消和分页验证通过，独立审查通过。

### Task 1: 待确认收件箱和账单导入预览

**Files:**

- Modify: `crates/ledger-import/src/{preview,dedupe,normalize,wechat,alipay,csv}.rs`
- Modify: `crates/ledger-app/src/transactions.rs`, `ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `apps/tauri-ui/src/main.ts`, `apps/tauri-ui/src/styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-import/tests/provider_parsers.rs`, `import_preview.rs`, `crates/ledger-app/tests/ledger_workflows.rs`

**Interfaces:**

- `parse_wechat_csv(bytes)`、`parse_alipay_csv(bytes)` 和 `parse_bank_csv(bytes)` 返回统一 `ParsedImport`。
- `ImportPreview::build(rows, existing)` 返回新增、精确重复、疑似重复、警告和待确认计数。
- `PendingService::list(query)`、`confirm(ids, edits)`、`ignore(ids)`、`undo_ignore(id)` 提供分页和幂等收件箱操作。

- [x] **Step 1: 写解析与预览失败测试**：覆盖 UTF-8/BOM、中文表头、消费、收入、退款、负号、缺失金额、非法日期、重复外部交易号和只有金额日期的疑似重复。
- [x] **Step 2: 实现统一标准化**：供应商适配器只产生 `NormalizedImportRow`，不访问 SQLite；未知表头返回明确错误；金额按整数分解析。
- [x] **Step 3: 实现去重与证据**：优先使用 `(source_kind, external_transaction_id)`，没有稳定 ID 时只标记候选，不自动合并；写入来源摘要哈希，不保存原始正文。
- [x] **Step 4: 实现预览确认事务**：在一笔 SQLite transaction 中写入导入批次、交易、分录和来源证据；任一行失败全部回滚；重复确认返回原收据。
- [x] **Step 5: 增加 Tauri 文件选择和预览命令**：文件进入内存解析，前端显示每行状态和警告，确认按钮提交候选 ID 与用户修订字段。
- [x] **Step 6: 增加“待确认”导航页**：按来源、状态、日期和账户筛选，支持单条编辑、批量确认、批量忽略和疑似重复展开。
- [x] **Step 7: 运行 `cargo test -p ledger-import -p ledger-app`、`npm run build`**，确认导入失败不会改变余额。
- [x] **Step 8: Commit**：`feat: add pending inbox and bill import preview`。

**任务 1 完成记录：** 提交 `aca8b1c`、`aaf9850`；导入预览、待确认编辑/确认/忽略/恢复、跨来源疑似查重已接入。65 项相关 Rust 测试、3 项浏览器模拟 IPC 测试及构建通过，独立复审通过。原生文件选择器及 Android 实机流程保留到任务 10 验证；当前支持 UTF-8 CSV，银行使用统一表头。

### Task 2: 交易详情、编辑、撤销和关联记录

**Files:**

- Modify: `crates/ledger-domain/src/transaction.rs`, `validation.rs`
- Modify: `crates/ledger-app/src/transactions.rs`, `ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/ledger.rs`, `lib.rs`
- Test: `crates/ledger-app/tests/ledger_workflows.rs`, `crates/ledger-storage/tests/sqlite_repository.rs`

**Interfaces:**

- `TransactionService::edit(id, patch)` 只允许编辑可变字段，并重新执行账户、币种、金额和退款父交易校验。
- `TransactionService::reverse(id, reason)` 创建可追溯反向交易，原交易保留并标记 `Reversed` 或关联 `reversal_transaction_id`。
- `TransactionQuery` 返回完整详情、来源证据、分类、商户、关联原交易/退款/转账和审计事件。

- [x] **Step 1: 写失败测试**：编辑已确认收入、编辑金额导致报表变化、撤销消费、撤销退款、不能撤销 Pending、不能把退款父项改成非消费、转账两端金额守恒。
- [x] **Step 2: 实现可变字段补丁**：金额、日期、账户、分类、商户和备注变更使用乐观版本检查；来源导入字段只读，保留用户修订覆盖层。
- [x] **Step 3: 实现撤销而非删除**：撤销写入反向分录和审计事件，余额与报表通过统一 `report_effect` 计算；重复撤销返回原撤销 ID。
- [x] **Step 4: 为当前明细列表增加详情抽屉**：展示来源、状态、关联记录和变更历史；保存前显示余额/报表影响，危险操作要求二次确认。
- [x] **Step 5: 更新手动记账表单**：支持分类、商户、备注和“转账/退款”入口；保持现有人民币和平台选择约束。
- [x] **Step 6: 运行 Rust 流程测试、TypeScript 构建和真实 SQLite 重启测试**。
- [x] **Step 7: Commit**：`feat: add editable auditable transactions`。

**任务 2 完成记录：** 交易详情展示来源摘要、关联记录与审计事件；确认后的可变字段按版本编辑，撤销写入反向分录和原因。97 项 Rust 测试（含真实 SQLite 重启与撤销报表抵消）、4 项浏览器模拟 IPC 测试、3 项前端格式测试、前端构建、目录索引与格式检查通过；应用层和存储层严格 Clippy 通过。Android 实机尚未验证。原工作区未提交的后续任务草稿不属于本阶段提交。

### Task 3: 月度报表、预算和提醒

**Files:**

- Modify: `crates/ledger-app/src/reports.rs`, `ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Create: `crates/ledger-app/src/budgets.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/ledger.rs`, `lib.rs`
- Test: `crates/ledger-app/tests/classification_and_reports.rs`, `crates/ledger-app/tests/budget_workflows.rs`

**Interfaces:**

- `ReportService::summarize(query)` 扩展为账户、分类、商户和月份维度，返回趋势、占比、排行、退款抵减和余额快照。
- `BudgetService::{create,update,archive,list,progress}` 管理按月份/分类/账户的整数分预算。
- `ReminderService::due(now)` 只生成稳定的提醒事件，不直接写交易。

- [x] **Step 1: 写失败测试**：月份边界、时区/DST、退款抵减、转账不计收入支出、Pending 不计入、分类预算进度和超支边界。
- [x] **Step 2: 扩展报表查询**：保持半开 UTC 区间，加入分类名称、商户聚合和分页排行；禁止把原始通知正文放入报表。
- [x] **Step 3: 实现预算持久化和进度**：预算周期支持自然月和自定义周期；停用预算不删除历史进度；超支百分比避免整数溢出。
- [x] **Step 4: 实现 Tauri 报表命令和首页**：提供本月总览、趋势、分类占比、预算条、商户排行和日期范围切换；无数据时显示空状态。
- [x] **Step 5: 增加预算设置页**：新增、编辑、停用预算，不能选择已停用分类作为新预算目标。
- [x] **Step 6: 运行 `cargo test -p ledger-app` 和 `npm run build`**，用固定时区样本核对金额。
- [x] **Step 7: Commit**：`feat: add reports budgets and reminders`。

**任务 3 完成记录：** 沿用 React 模块边界，新增 `features/reports/`、`features/budgets/` 和 Rust `analytics/` 适配模块。`summarize_scoped` 在保留原有报表接口的基础上支持平台、分类、商户筛选、月份趋势、分类占比、分页商户排行及期末余额；SQLite 在 UTC 半开区间内读取已确认交易，IPC 不返回原始来源正文。预算复用版本 5 的预算表，支持自然月及自定义周期、唯一范围、编辑和停用保留历史；退款及其撤销继承原消费分类，预算计算不依赖收入或余额聚合。提醒按预算、周期和阈值生成稳定 ID，当前仅在报表页展示，不产生交易或系统通知。

验证：111 项 Rust 测试、11 项浏览器模拟 IPC 测试、3 项前端格式工具测试、生产构建、应用层与存储层严格 Clippy、Prettier、Rust 格式及目录索引检查通过。固定 UTC+8 和纽约 DST 月份样本覆盖半开边界；真实 SQLite 覆盖预算重启持久化、重复范围/停用分类校验、退款撤销、80%/用尽/超支及大整数百分比。Tauri 报表和预算命令另以 `TZ=Asia/Shanghai` 验证。浏览器交互使用模拟 IPC，Android 实机仍留待任务 10；系统通知及自动采集不属于本阶段。完成提交与 GitHub 上传后停止，未推进任务 4。

### Task 4: Android 通知自动采集

**Files:**

- Modify: `crates/ledger-notifications/src/{event,parse,providers,lib}.rs`
- Modify: `crates/ledger-app/src/transactions.rs`, `ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Create/Modify: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/NotificationCaptureService.kt`
- Create/Modify: `apps/tauri-ui/src-tauri/gen/android/app/src/main/java/com/example/localledger/RustBridge.kt`
- Modify: `apps/tauri-ui/src-tauri/gen/android/app/src/main/AndroidManifest.xml`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`, `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-notifications/tests/notification_parsers.rs`, Android adapter tests, Rust JNI boundary tests

**Interfaces:**

- `parse_notification(event, settings) -> ParseOutcome::{Candidate,NeedsConfirmation,Ignored,Unsupported}`。
- Android 只传 `package_name`、标题、正文、通知 key 和发布时间；Rust 返回 `ignored/pending/error` 结果。
- `NotificationSettings` 按包名控制来源，默认关闭；通知来源证据使用通知 key/摘要去重。

- [ ] **Step 1: 写通知解析失败测试**：微信/支付宝消费、收款、退款、转账、重复回调、负号、中文金额、缺少商户和不支持包名。
- [ ] **Step 2: 实现来源解析器**：公共逻辑只做金额、日期、方向和置信度；不确定字段生成 Pending，不得直接 Confirmed；日志只输出来源和结果摘要。
- [ ] **Step 3: 实现 Rust 入口**：将解析候选写入待确认表和来源证据；重复通知返回已有候选；服务重启后继续幂等。
- [ ] **Step 4: 实现 Android 通知服务**：声明通知监听服务，读取系统授予状态，过滤启用来源，捕获异常后不阻塞系统回调；原始正文只在内存中传递。
- [ ] **Step 5: 增加设置页和权限引导**：首次启用说明用途，跳转系统通知访问设置，逐来源开关，并显示最近一次解析结果而非正文。
- [ ] **Step 6: 验证 Tauri Android 生成/安装流程**：在模拟器或真机上测试授予权限、进程重启、重复通知、关闭来源和 WebView 刷新。
- [ ] **Step 7: 运行 Rust 通知测试、`npm run build` 和 Android instrumentation/最小真机流程**。
- [ ] **Step 8: Commit**：`feat: connect Android payment notifications`。

### Task 5: 周期性账单和到期提醒

**Files:**

- Create: `crates/ledger-app/src/recurring.rs`
- Modify: `crates/ledger-domain/src/transaction.rs`, `crates/ledger-app/src/ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-app/tests/recurring_workflows.rs`, `crates/ledger-storage/tests/sqlite_repository.rs`

**Interfaces:**

- `RecurringTemplate::{id, account_id, kind, amount, category_id, merchant, description, cadence, next_due_date, end_date, enabled}`。
- `RecurringService::{create,update,pause,resume,generate_due,skip_instance,list}`；`generate_due` 返回候选而非直接确认。

- [ ] **Step 1: 写失败测试**：月末日期、每周/每月/每年、结束日期、暂停/恢复、单次跳过、重启后不重复生成和金额修改只影响未来实例。
- [ ] **Step 2: 实现周期计算**：使用 `NaiveDate` 和明确的本地日历策略，月末模板在短月份落到该月最后一天；生成实例使用唯一键 `(template_id, due_date)`。
- [ ] **Step 3: 实现候选生成**：到期实例进入 Pending，携带模板来源证据；用户确认后创建正式交易并标记实例已确认。
- [ ] **Step 4: 增加周期账单管理页**：列表显示下一次到期、金额、平台、分类、启用状态；支持暂停、恢复、跳过和编辑未来实例。
- [ ] **Step 5: 增加首页到期提醒**：只显示未来 7 天和逾期未处理项目，点击进入待确认收件箱。
- [ ] **Step 6: 运行 `cargo test -p ledger-app`、TypeScript 构建和重启幂等测试**。
- [ ] **Step 7: Commit**：`feat: add recurring ledger entries`。

### Task 6: 搜索、商户归一化和分类规则

**Files:**

- Modify: `crates/ledger-app/src/classification.rs`, `ports.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-app/tests/classification_and_reports.rs`, `crates/ledger-storage/tests/sqlite_repository.rs`, `apps/tauri-ui/tests/search.test.ts`

**Interfaces:**

- `TransactionSearch { query, account_id, category_id, status, source, from, until, min_amount, max_amount, page, page_size }`。
- `ClassificationRule { priority, merchant_contains, description_contains, category_id, enabled }` 沿用现有类型，新增规则试运行和命中说明。
- `ClassificationService::suggest(transaction)` 返回分类、命中规则 ID 和置信说明，不直接覆盖用户确认过的分类。

- [ ] **Step 1: 写失败测试**：大小写/空白归一化、中文商户别名、金额范围、分页稳定排序、多个规则优先级、停用分类和手工覆盖。
- [ ] **Step 2: 实现 SQLite 搜索索引和稳定分页**：使用参数化查询；关键词匹配商户、描述和来源摘要可搜索字段，禁止拼接 SQL。
- [ ] **Step 3: 实现商户归一化**：保留原始商户显示值，另存规范化值和本地别名映射；规则只作用于规范化值。
- [ ] **Step 4: 实现规则预览**：创建/编辑规则前显示命中数量和受影响的 Pending 候选；用户确认后只更新候选建议，不批量改动已确认交易。
- [ ] **Step 5: 增加明细搜索页**：组合筛选、清除筛选、分页、详情跳转和导出当前结果。
- [ ] **Step 6: 运行 Rust 搜索/规则测试、SQL 注入回归测试、`npm run build`**。
- [ ] **Step 7: Commit**：`feat: add ledger search and classification rules`。

### Task 7: 账户对账

**Files:**

- Create: `crates/ledger-app/src/reconciliation.rs`
- Modify: `crates/ledger-app/src/ports.rs`, `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-app/tests/reconciliation_workflows.rs`, `crates/ledger-storage/tests/sqlite_repository.rs`

**Interfaces:**

- `ReconciliationService::start(account_id, as_of, actual_balance)` 生成不可变对账快照。
- `ReconciliationService::summary(session_id)` 返回账本余额、实际余额、差异、未确认候选和期间交易。
- `ReconciliationService::{mark_reviewed,add_adjustment,close,reopen}` 管理对账状态；调整必须产生可审计的余额调整交易。

- [ ] **Step 1: 写失败测试**：无参考余额、负余额、截止时刻边界、Pending/忽略交易、已关闭对账不可静默修改、重复关闭幂等。
- [ ] **Step 2: 实现快照和差异计算**：按账户和本地截止时刻计算 Confirmed 余额，保存当时的账本金额和输入实际金额；不把差异直接写成隐藏字段。
- [ ] **Step 3: 实现对账调整**：用户明确确认后生成 `BalanceAdjustment`，附原因和审计事件；调整不能伪装成收入或支出。
- [ ] **Step 4: 增加对账页面**：选择平台和日期、输入真实余额、展示差异和待处理项、确认调整、关闭/重新打开会话。
- [ ] **Step 5: 运行 Rust 对账测试和真实 SQLite 重启测试**，确认历史对账不随新增交易被篡改。
- [ ] **Step 6: Commit**：`feat: add account reconciliation`。

### Task 8: 加密备份、恢复和数据导出

**Files:**

- Modify: `crates/ledger-backup/src/{archive,crypto,restore,lib}.rs`
- Modify: `crates/ledger-storage/src/sqlite.rs`
- Create: `crates/ledger-export/src/lib.rs` and `crates/ledger-export/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Test: `crates/ledger-backup/tests/backup_restore.rs`, `crates/ledger-export/tests/export.rs`

**Interfaces:**

- 备份容器包含显式 schema 版本、账户、分类、规则、交易、分录、来源摘要、导入批次、预算、周期模板和对账记录，不包含 Token、通知正文和密码。
- `BackupService::{create,inspect,restore}` 使用带认证的加密容器；错误密码、篡改、未知版本和约束失败都返回明确错误。
- `ExportService::{csv,json}` 导出脱敏后的已确认交易或用户选择的筛选结果；导出不改变账本。

- [ ] **Step 1: 写失败测试**：空账本、大账本、错误密码、损坏密文、旧版本、重复恢复、恢复失败保留原库、导出金额和退款关联。
- [ ] **Step 2: 实现版本化归档**：先从 repository 读取一致性快照，写入 schema、checksum 和 authenticated encryption；来源内容只写摘要。
- [ ] **Step 3: 实现临时库恢复**：解密后导入临时 SQLite，执行迁移、外键、金额、来源去重和业务不变量校验，成功后原子替换或事务合并。
- [ ] **Step 4: 实现 CSV/JSON 导出**：稳定列名、UTF-8 BOM 可选、整数分转十进制字符串；默认导出 Confirmed，用户可显式包含 Pending 状态列。
- [ ] **Step 5: 增加设置入口**：文件选择器、密码输入、备份检查、恢复预览、导出范围和成功/失败结果；恢复操作必须显示将覆盖或合并的范围。
- [ ] **Step 6: 运行备份恢复、迁移回放、导出 round-trip、`cargo test --workspace` 和前端构建**。
- [ ] **Step 7: Commit**：`feat: add encrypted backup and data export`。

### Task 9: 隐私控制和数据生命周期

**Files:**

- Create: `crates/ledger-app/src/privacy.rs`
- Modify: `crates/ledger-app/src/ports.rs`, `crates/ledger-storage/src/sqlite.rs`
- Modify: `apps/tauri-ui/src/api.ts`, `main.ts`, `styles.css`
- Modify: `apps/tauri-ui/src-tauri/src/{ledger,lib}.rs`
- Modify: `apps/tauri-ui/src-tauri/tauri.conf.json` and Android manifest/settings integration as required by the Tauri mobile shell
- Test: `crates/ledger-app/tests/privacy_workflows.rs`, Android settings tests, `apps/tauri-ui/tests/privacy.test.ts`

**Interfaces:**

- `PrivacySettings { app_lock_enabled, hide_amounts, notification_sources, auto_lock_minutes }`；设置只保存非秘密偏好。
- `PrivacyService::{set_lock,unlock,lock_now,redact_view,delete_source_data,delete_all_data}`。
- 加密备份密码只进入内存中的加密 API；不写 SQLite、日志、前端 localStorage 或普通备份字段。

- [ ] **Step 1: 写失败测试**：锁定后不能访问交易 DTO，隐藏金额只影响展示，关闭通知来源停止新采集但不删除已有记录，删除来源数据范围可预览且可回滚，删除全部数据需要显式确认。
- [ ] **Step 2: 实现设置和锁状态**：锁状态在 Rust 进程内维护，应用重启默认锁定（若已启用）；前端只收到布尔状态和脱敏视图。
- [ ] **Step 3: 实现敏感数据清理**：按来源删除通知摘要/候选，保留用户确认交易的必要审计信息；全部删除先创建加密备份提示，再执行事务删除并重建默认平台。
- [ ] **Step 4: 实现 UI 隐私设置**：应用锁、自动锁定、隐藏金额、来源开关、删除数据预览；删除和关闭采集明确说明影响范围。
- [ ] **Step 5: 审查日志与 IPC**：搜索通知正文、Token、密码、完整账单行和金额泄漏；将错误映射为稳定用户提示。
- [ ] **Step 6: 运行 Rust/Android/TypeScript 隐私测试和一次发布包静态扫描**。
- [ ] **Step 7: Commit**：`feat: add privacy and data lifecycle controls`。

### Task 10: 端到端验收和发布加固

**Files:**

- Modify: `README.md`, `docs/architecture.md`
- Create: `docs/privacy.md`, `docs/user-workflows.md`
- Modify: all affected `tree` files through `python3 scripts/update_tree.py`
- Test: workspace tests, migration replay tests, frontend tests, Android instrumentation/minimal device flow

- [ ] **Step 1: 建立脱敏 fixture**：准备微信/支付宝 CSV、通知事件、退款、转账、周期账单、预算和对账样本；fixture 不含真实个人信息。
- [ ] **Step 2: 运行完整验证**：
  - `cargo fmt --all -- --check`
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cd apps/tauri-ui && npm run build && node --experimental-strip-types --test tests/*.test.ts`
  - `python3 scripts/update_tree.py --check`
  - `bash scripts/build_android.sh arm64-v8a` 和 `bash scripts/build_android.sh x86_64`
- [ ] **Step 3: 执行端到端回放**：导入/通知产生 Pending，修改后确认，确认后余额与报表变化；重复事件、重复导入、重复确认、重启和恢复均不重复入账。
- [ ] **Step 4: 执行真实 Android 最小流程**：安装 APK、授予通知访问、收到测试通知、关闭来源、杀进程重启、打开待确认页和恢复备份；记录无法在模拟器验证的限制。
- [ ] **Step 5: 做发布前隐私扫描**：扫描待上传文件、暂存内容和完整 Git 历史中的 Token、密码、私钥、通知正文和真实账单样本；发现内容先脱敏再发布。
- [ ] **Step 6: 更新 README**：写清安装、权限、导入、待确认、备份恢复、删除数据和 Android 验证步骤；说明当前仍不支持的多币种换算和在线平台同步。
- [ ] **Step 7: Commit**：`docs: document nine ledger features and release checks`。

## Cross-Feature Acceptance Criteria

- 导入文件和 Android 通知都进入同一个 Pending 收件箱、去重和确认流程。
- 任意 Pending、Ignored 或疑似重复记录都不会改变正式余额、预算进度和报表。
- 转账金额守恒；退款保留原消费并抵减支出；信用卡还款不被重复计为消费。
- 编辑、撤销、对账调整、恢复和删除均有可追溯审计事件；物理删除只发生在用户明确选择的来源数据或全部数据清理中。
- 本地月份、时区、负余额、零金额、超大金额和重启后的计算结果可复核。
- 重复通知、重复导入、重复确认、重复周期生成和重复恢复不会产生重复正式交易。
- 关闭某个采集来源后不再处理新数据，已有用户确认数据不被静默删除。
- 备份恢复失败不破坏现有账本；导出不携带 Token、密码、通知正文或未授权原始账单内容。
- Android、桌面 Tauri 和 Rust 端都使用同一套业务规则；UI 不直接访问 SQLite。

## Suggested Commit Sequence

1. `feat: define nine ledger feature contracts`
2. `feat: add pending inbox and bill import preview`
3. `feat: add editable auditable transactions`
4. `feat: add reports budgets and reminders`
5. `feat: connect Android payment notifications`
6. `feat: add recurring ledger entries`
7. `feat: add ledger search and classification rules`
8. `feat: add account reconciliation`
9. `feat: add encrypted backup and data export`
10. `feat: add privacy and data lifecycle controls`
11. `docs: document nine ledger features and release checks`
