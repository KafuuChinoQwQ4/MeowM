# 本地收支管理系统后续阶段实施计划

> **For agentic workers:** 每个任务都必须使用测试先行，并在任务完成后运行 `python3 scripts/update_tree.py --check`。任务之间按顺序执行；每个任务完成后进行独立代码审查。

**目标：** 在现有 Rust 本地账本核心之上，完成微信/支付宝账单导入、Android 通知自动采集、移动端界面、本地备份，以及后续可选的支付平台 API 接入。

**当前基线：** `ledger-domain`、`ledger-app`、`ledger-storage` 已完成账户、交易、退款、转账、分类、报表和 SQLite 持久化。`ledger-import`、`ledger-notifications`、`ledger-backup`、Android 和 Slint UI 目前是职责骨架。

**架构：** Android 只负责系统权限、通知监听、文件选择和 JNI 边界；Rust 负责解析、标准化、去重、待确认交易、账本写入、分类、报表和备份。所有数据默认保存在本机，不引入账号、服务端或云同步。

**技术栈：** Rust workspace、SQLite、Android Kotlin、JNI/UniFFI 边界、Slint UI、整数最小货币单位、版本化迁移。

**相关设计：** `docs/superpowers/specs/2026-10-07-local-expense-manager-design.md`

## 全局约束

- 第一版不需要用户账号、服务端或云同步。
- 所有金额继续使用最小货币单位整数或定点十进制，禁止二进制浮点数。
- 自动采集得到的交易默认进入 `Pending`，不能直接改变正式余额和报表。
- 只有用户确认后才进入 `Confirmed`；重复确认必须幂等。
- 通知正文默认不长期保存，不写入普通日志；只保留来源和标准化字段摘要。
- 导入必须先预览，用户确认后才批量写入；批量写入失败必须整体回滚。
- 微信、支付宝、银行卡和后续 API 都必须进入同一个标准化、去重和分类流程。
- 用户可在设置中分别启用或关闭通知采集、账单导入来源和未来的 API 来源。
- 多币种总览继续使用交易保存的 CNY 报表金额；账户详情使用账户原币余额。
- 每次新增或删除文件都更新 `scripts/tree_descriptions.json`，运行 `python3 scripts/update_tree.py`，并用 `--check` 验证。

## 阶段 1：完成账单文件导入

### 任务 1：定义导入领域类型与 CSV 解码

**文件：**

- 修改：`crates/ledger-import/src/lib.rs`
- 修改：`crates/ledger-import/src/csv.rs`
- 修改：`crates/ledger-import/src/normalize.rs`
- 修改：`crates/ledger-import/src/wechat.rs`
- 修改：`crates/ledger-import/src/alipay.rs`
- 测试：新增 `crates/ledger-import/tests/provider_parsers.rs`

**产出接口：**

- `ImportSource::{WeChat, Alipay, BankCsv}`
- `RawImportRow`：原始行号、时间、金额文本、币种文本、收支方向、商户、外部交易号和原始字段摘要。
- `NormalizedImportRow`：标准化时间、`Money`、交易类型、商户、账户线索、CNY 报表金额和解析警告。
- `parse_wechat_csv(bytes) -> Result<ParsedImport, ImportError>`
- `parse_alipay_csv(bytes) -> Result<ParsedImport, ImportError>`

**步骤：**

- [ ] 先写微信、支付宝表头、编码、金额和退款行的失败测试；测试数据使用脱敏固定样本。
- [ ] 支持 UTF-8、UTF-8 BOM 和常见中文表头；未知表头必须返回可读错误。
- [ ] 金额解析使用整数最小单位，禁止 `f32`/`f64`；无法确定方向或金额的行进入警告列表。
- [ ] 将供应商行转换成统一结构，不在解析器内直接写 SQLite。
- [ ] 测试重复外部交易号、退款关联号、缺失金额、非法日期和跨币种金额。
- [ ] 提交 `feat: parse local payment bill files`。

### 任务 2：导入预览、去重和原子确认

**文件：**

- 修改：`crates/ledger-import/src/dedupe.rs`
- 修改：`crates/ledger-import/src/preview.rs`
- 修改：`crates/ledger-app/src/ports.rs`
- 测试：新增 `crates/ledger-import/tests/import_preview.rs`

**产出接口：**

- `ImportCandidate`：标准化交易、来源证据、匹配状态和警告。
- `DuplicateMatch::{ExactExternalId, StrongCandidate, NoMatch}`。
- `ImportPreview::build(rows, existing) -> Result<ImportPreview, ImportError>`。
- `ImportPreview::confirm(self, repository) -> Result<ImportReceipt, ImportError>`。

**规则：**

- 有稳定外部交易号时按来源和外部交易号去重。
- 没有稳定交易号时，使用金额、币种、时间、商户、账户和来源类型生成候选匹配，但不能只凭金额和日期自动合并。
- 预览中明确显示新增、重复、疑似重复、警告和待确认数量。
- 确认过程在一个 SQLite transaction 中写入交易、分录、来源证据和导入批次；任意失败全部回滚。
- 添加导入批次重放测试，确保同一文件重复导入不会产生重复正式交易。
- 提交 `feat: add import preview and deduplication`。

## 阶段 2：完成 Android 通知自动采集

### 任务 3：实现通知事件和来源解析器

**文件：**

- 修改：`crates/ledger-notifications/src/event.rs`
- 修改：`crates/ledger-notifications/src/parse.rs`
- 修改：`crates/ledger-notifications/src/providers.rs`
- 修改：`crates/ledger-notifications/src/lib.rs`
- 测试：新增 `crates/ledger-notifications/tests/notification_parsers.rs`

**产出接口：**

- `NotificationEvent`：来源包名、标题、正文、通知标识、发布时间和采集设置；不得包含 Android 对象引用。
- `ParsedNotification`：金额、币种、方向、商户、账户线索、外部交易号、置信度和解析警告。
- `parse_notification(event, settings) -> ParseOutcome`。
- `ParseOutcome::{Candidate, NeedsConfirmation, Ignored, Unsupported}`。

**规则：**

- 只支持明确配置的来源包名；其他包名直接忽略。
- 微信和支付宝解析器独立，公共逻辑只处理金额、日期、方向和置信度。
- 解析不确定或字段缺失时生成 `Pending` 候选，不能直接确认。
- 日志中只能出现来源类型、结果类型和稳定摘要，不能打印通知正文或 Token。
- 覆盖消费、收款、退款、转账提示、重复通知、负号和中文金额格式。
- 提交 `feat: parse payment notifications locally`。

### 任务 4：连接 Android 通知监听与 Rust

**文件：**

- 修改：`apps/android/app/src/main/kotlin/com/example/localledger/NotificationCaptureService.kt`
- 修改：`apps/android/app/src/main/kotlin/com/example/localledger/RustBridge.kt`
- 修改：`apps/android/app/src/main/AndroidManifest.xml`
- 修改：Android 设置页相关文件
- 新增：Rust JNI/UniFFI 入口文件
- 测试：Android 适配测试和 Rust 边界测试

**规则：**

- 首次使用时说明通知权限用途，并提供系统设置跳转。
- 设置中按来源启用/关闭采集；关闭后服务仍可运行，但不向 Rust 转发该来源。
- Android 只传递包名、标题、正文、通知标识和时间；解析、去重和账本写入由 Rust 完成。
- Rust 入口返回明确的 `ignored/pending/confirmed/error` 结果，Android 不自行改账。
- 服务重启、重复回调和 JNI 异常不能导致崩溃或重复入账。
- 在真实 Android 模拟器或测试设备上验证通知权限、进程重启和重复通知。
- 提交 `feat: connect android notification capture`。

## 阶段 3：完成本地移动端界面

### 任务 5：实现 Slint 账本界面

**文件：**

- 修改：`apps/slint-ui/ui/main.slint`
- 修改：`apps/slint-ui/src/main.rs`
- 新增：交易列表、待确认、报表和设置的 UI 模块
- 新增：UI 到 `ledger-app` 的适配层
- 测试：Rust UI model 测试和核心交互测试

**页面顺序：**

- 首页：本月收入、支出、净支出和账户余额。
- 交易：按日期、来源、账户、分类和状态筛选；支持编辑分类和备注。
- 待确认：展示通知/导入候选，支持确认、忽略、修改后确认。
- 报表：月度趋势、分类占比、商户排行、退款抵扣。
- 设置：来源开关、默认账户、分类管理、备份与恢复入口。

**规则：**

- 所有写操作通过 `ledger-app`，UI 不直接访问 SQLite。
- Pending 交易必须显示状态；图表只显示 Confirmed。
- 首次启用通知采集、导入来源和备份功能都需要明确的设置项。
- 测试状态切换、筛选、确认后刷新余额和报表。
- 提交 `feat: add local ledger mobile screens`。

## 阶段 4：本地备份与恢复

### 任务 6：实现版本化加密备份

**文件：**

- 修改：`crates/ledger-backup/src/archive.rs`
- 修改：`crates/ledger-backup/src/crypto.rs`
- 修改：`crates/ledger-backup/src/restore.rs`
- 修改：`crates/ledger-backup/src/lib.rs`
- 修改：Android/Slint 文件选择与设置入口
- 测试：新增 `crates/ledger-backup/tests/backup_restore.rs`

**规则：**

- 备份包含显式 schema 版本、账户、分类、交易、来源摘要、导入批次和分类规则。
- 使用带认证的加密容器；密码错误、篡改、版本不支持和数据校验失败必须拒绝恢复。
- 恢复先导入临时数据库，完成校验后原子替换；失败不能破坏现有账本。
- 恢复过程重新执行交易/分录/来源去重约束。
- 测试空账本、大账本、重复恢复、错误密码、损坏文件和旧版本升级。
- 提交 `feat: add encrypted local backups`。

## 阶段 5：可选支付平台 API 接入

### 任务 7：定义外部来源适配器边界

**文件：**

- 修改：`crates/ledger-app/src/ports.rs`
- 新增：`crates/ledger-import/src/providers.rs`
- 新增：各支付平台适配器模块
- 修改：设置和凭据存储模块
- 测试：使用脱敏 HTTP fixture 的适配器测试

**规则：**

- API 接入是可选设置，默认关闭；没有 API Token 时不影响本地导入和通知采集。
- 每个平台实现独立的授权、分页、时间范围同步和撤销接口。
- Token 只能保存在 Android Keystore 或等价的本地安全存储中，不能写入 SQLite 普通表、日志或备份明文。
- API 数据仍进入统一预览、去重、待确认和原子确认流程。
- 网络失败、限流、权限撤销和分页重复必须可恢复且不能重复入账。
- 首批只设计适配器边界和 mock 测试，真实平台接入逐个平台评估，不把平台限制写死在核心账本。
- 提交 `feat: add optional provider adapter boundary`。

## 阶段 6：发布前加固

### 任务 8：稳定性、隐私和发布验证

**文件：**

- 修改：`README.md`、`docs/architecture.md`
- 新增：数据删除、日志策略和权限说明文档
- 修改：所有受影响目录的 `tree`
- 测试：workspace、Android instrumentation、备份恢复和导入回放测试

**验收标准：**

- 通知重复、账单重复、Pending 重复确认都不会产生重复正式交易。
- 转账金额守恒；退款保留原交易并抵减净支出；信用卡还款不重复计入消费。
- 待确认和忽略交易不改变正式余额和图表。
- 本地月份边界、时区转换和多币种 CNY 报表金额可复核。
- 关闭某个来源后，不再采集该来源的新数据；已有数据不被删除。
- 导出、备份、恢复和删除操作都有明确的用户确认与失败回滚。
- 全部 Rust 测试、Clippy、格式检查、tree 检查、Android 构建和最小真机流程通过。
- 发布包不包含 Token、通知正文、个人账单样本或调试日志。

## 推荐执行顺序

按以下顺序逐个完成并审查：

1. 账单解析与标准化。
2. 导入预览、去重和原子确认。
3. 通知事件解析和 Pending 交易生成。
4. Android 通知监听与 Rust 边界。
5. Slint/Android 账本界面。
6. 加密备份与恢复。
7. 可选支付平台 API 适配器。
8. 稳定性、隐私和发布验证。

每个任务完成后都必须提交独立 commit、更新目录 `tree`、运行针对性测试，并在进入下一个任务前完成审查。
