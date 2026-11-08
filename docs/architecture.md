# 架构边界

- `ledger-domain` 定义带币种指数的最小单位金额、固定 11 种币种目录、账户、交易及不变量，不依赖平台。外币到人民币的精确换算使用整数最小单位和十进制定点汇率，不经过 JavaScript 浮点数。
- `ledger-app` 编排账户、交易、分类、报表、预算、周期和提醒用例，通过 ports 访问持久层。
- `ledger-storage` 将账本保存在本机 SQLite。
- `ledger-storage/src/sqlite/exchange_rates.rs` 由 schema 13 精确保存每日 CNY 参考汇率及同步状态，重叠日期的后续来源修订可更新旧观察值；汇率历史包含在可信快照、加密备份与恢复中。
- 账户的币种代码和精度由既有账户列保存；schema 14 新增 `transaction_fx_snapshots`，为已确认外币交易保存独立汇率快照。交易分录保留账户原币，报表金额单独以 CNY 存储；交易详情、备份恢复和快照校验都会读取并验证汇率来源、有效日期及原币种。
- `apps/tauri-ui/src-tauri/src/exchange_rates/` 集中实现 Frankfurter v2 请求、来源校验、限时限量读取、同步状态和汇率 IPC。桌面只在应用进程启动或用户手动刷新时检查；Android `ExchangeRateWorkScheduler` 注册联网的 WorkManager 周期任务和启动任务，`ExchangeRateWorker` 通过 JNI 调用同一 Rust 服务与本机 SQLite。Worker 成功不依赖 Activity/WebView 存活，Android 仍可按网络和省电状态延迟执行。
- Tauri `transactions.rs` 使用交易日期前最新且带有效来源信息的本地汇率预览并写入外币收入、支出和退款；若没有可用历史汇率，可手动提供每 1 CNY 可兑换的外币数值。修改已确认交易的说明等元数据会沿用原汇率快照。退款必须与原消费币种一致，转账只允许两端币种相同；账单、通知、待确认、预算和周期账单仍使用人民币。
- React `features/exchangeRates/` 从本机 CNY 报价历史离线构造任意源币/目标币货币对，按共同有效日期绘制 SVG 曲线并用整数最小单位换算金额；曲线不补造周末或单边缺失的数据点，点选目标保持键盘可达。账户和交易表单使用账户原币精度及 Rust 汇率预览，报表收支总额保持人民币，平台余额和日常参考余额按账户币种显示。
- `ledger-import` 处理微信、支付宝和银行卡 CSV/XLS/XLSX 账单，以及中国银行交易流水明细 PDF 的预览、标准化和去重。Excel 使用 Calamine 离线读取单元格；带密码的银行 PDF 在内存中解锁并提取文字及坐标，密码不进入导入会话，只缓存标准化流水。中行 PDF 按表格列解析、保留页面顺序、校验相邻余额并复用相同的候选预览/查重流程；其他 PDF 版式和扫描件明确拒绝，不伪造 OCR 结果。
- 账单最多 10,000 行、5 MiB，CSV 严格识别 UTF-8、GBK/GB18030。支付宝记账导出的退款保留为退款，提现、充值、还款等内部资金流保留为转账；中行 PDF 用金额符号确定相对流水账户的转入转出方向，并仅在对方户名唯一匹配现有平台时预填另一端，无法唯一判断时由用户补选。花呗、余额和余额宝可统一归属支付宝，同平台内部划转保存两条等额反向分录，平台总余额不变；跨平台转账分别调整两端余额。原账户名仅作核对，转账不计收支。
- 去重优先使用同支付渠道的交易单号；商户单号独立保存为辅助证据，不覆盖相冲突的渠道交易号。无单号的原样重导按来源摘要及已有记录次数核对，同文件无单号的相同行仅标疑似。其他候选按记账平台、金额、类型、实际店名或商品描述中的店名前缀和相近时间提示，疑似项默认不勾选。手动记账要求消费商户，备注、时间及两类单号可选，手动来源摘要与账单摘要隔离。迁移 12 保留旧证据并新增商户单号列和索引，快照、加密备份同步支持版本 12。
- `ledger-notifications` 将受支持包名的内存事件解析为候选，经 `NotificationRepository` 原子写入 Pending、来源证据和去重收据；原始正文和通知 key 不持久化。
- `ledger-backup` 管理本地加密备份和恢复。
- `ledger-app/src/privacy.rs` 维护进程内应用锁、Argon2id 密码验证、尝试限速与自动锁定；`ledger-storage/src/sqlite/privacy.rs` 用立即事务按来源或全量清理，预览指纹防止账本变化后误删。锁验证值仅存在本机设置，不进入加密备份。
- `apps/tauri-ui` 使用 React + TypeScript，通过 `src/api/` 中按业务划分的接口调用 Tauri Rust 命令。
- Tauri 账本命令共用 Rust 锁状态检查；React 启动先查询锁状态，解锁后才挂载账本页面。`features/privacy/` 负责锁定、金额遮盖和删除确认；Android Activity 设置 `FLAG_SECURE` 防止应用窗口截图。通知后台采集使用独立服务，不依赖已解锁的 WebView。
- 前端 `src/app/` 管理导航、当前平台和日期；`src/features/` 按平台、账本、交易、待确认、导入、报表、预算和周期拆分完整流程；`src/components/` 复用弹窗和表单，`src/shared/` 复用金额、日期和请求生命周期，`src/styles/` 按界面职责拆分样式。组件不直接调用 invoke。
- `src-tauri/src/ledger/` 按账户、每日汇总、查询、导入预览、入箱和待确认操作拆分命令实现；`transactions.rs` 编排交易详情、编辑和撤销，测试独立放在子模块。
- `src-tauri/src/analytics/` 按报表查询、预算管理和命令注册拆分，React 通过独立的 reports/budgets API 模块调用。`ledger-app` 负责本地日期边界、聚合与预算进度，SQLite 负责唯一范围及分类有效性的原子校验。提醒由预算状态派生稳定 ID，界面展示时不产生交易。
- `ledger-storage/src/sqlite/` 按连接、迁移、账户、账务仓储、分类、查询、导入、撤销、写入和行映射拆分。SQLite 连接保持私有，原子操作仍在同一数据库事务中完成；通知设置和收据由独立 `notifications.rs` 管理，迁移 6 新增去重收据与最近结果表。
- `apps/tauri-ui/src-tauri/gen/android` 提供 Tauri Android WebView 壳，入口为 `MainActivity`，沿用应用 ID `com.example.localledger`。旧 Slint 界面和 Android JNI 壳已移除。`NotificationCaptureService` 在有界后台队列中调用独立 JNI 边界，使用与 Tauri 相同的 `Context.dataDir/ledger.sqlite3`，不依赖 Activity 或 WebView 已启动。通知访问插件只处理系统授权状态及设置跳转，React 设置页、API 和样式分别独立成模块。

每个目录使用自己的 `tree` 说明直接文件。运行 `python3 scripts/update_tree.py` 更新，运行 `python3 scripts/update_tree.py --check` 检查。

- `ledger-domain/src/recurring.rs` 定义模板、日历周期、实例和提醒。`ledger-app/src/recurring/` 使用本地 `NaiveDate` 和原始锚点计算日历，不依赖 SQLite 或 WebView。
- `ledger-storage/src/sqlite/recurring/` 按行映射、模板写入与跳过、候选生成拆分。生成使用 SQLite immediate 事务，同时写入 Pending、`recurring` 来源证据、唯一实例收据及新游标；跳过也原子写收据与游标。模板版本避免过期编辑，实例确认状态直接关联交易状态，避免两套状态不一致。迁移 7 保留已有 ID，扩充来源类型并允许不关联交易的跳过收据。
- `src-tauri/src/recurring/` 按输入验证、DTO 映射和命令注册拆分；React `features/recurring/` 按资源刷新、表单、卡片、实例历史和首页提醒拆分。刷新生成后的通知只更新账本/收件箱查询，不触发无限补生成。提醒金额使用已生成候选快照，未来尚未生成的提醒使用当前模板。

- `ledger-storage/src/sqlite/search.rs` 复用账本分页查询并对日期、金额、账户、状态、来源、分类及关键词使用绑定参数；排序以微秒时间和交易 ID 打破并列。`search_rules.rs` 管理商户别名、规则命中预览及当前 Pending 建议。迁移 8 添加规范化商户列、别名表和搜索索引，原商户字段不改。
- `ledger-app/src/classification.rs` 统一商户规范化、规则排序和可解释建议。`src-tauri/src/search_rules/` 提供分类、规则预览与别名 IPC；React `features/search/` 拆分筛选、结果、分类管理、规则表单及 CSV 导出。规则建议读取时派生，只有用户在待确认编辑器中选择采用才成为分类写入。
- Android 搜索 CSV 导出由 `src-tauri/src/search_export/` 注册 Tauri 插件，生成的 Kotlin `SearchExportPlugin` 通过系统 `ACTION_CREATE_DOCUMENT` 让用户选位置，再写入选中的 URI；桌面保留浏览器下载。

- 账户对账由 `ledger-app/src/reconciliation.rs` 定义独立仓储边界和状态服务，`ledger-storage/src/sqlite/reconciliation/` 按快照生成、操作、字段映射和事务入口拆分。迁移 9 保存不可变会话金额/参考点、流水快照、逐项核对状态和原因审计；旧占位会话保留但不允许补写为可信快照。写操作使用立即事务和版本校验，余额调整与关联收据、审计在同一事务提交。数据库触发器保护快照及对账调整，普通交易编辑/撤销不能绕过。
- `src-tauri/src/reconciliation/` 分离命令、返回类型和真实 SQLite 测试，将设备本地截止日转换为 UTC 日末；所有 IPC 金额仍为字符串。React 的 `features/reconciliation/` 分离页面、创建表单、会话操作、快照列表和展示工具，通过独立 API 调用；用户先逐项核对，再填写原因、确认调整及关闭。关闭/重开不修改原始快照。

- `ledger-storage/src/sqlite/snapshots/` 从同一 SQLite 事务读取白名单表，排除本机凭据和权限，按受信任迁移重建临时库并校验外键、金额及业务关系。恢复使用立即事务、预览指纹和完整替换，失败回滚。
- `ledger-backup` 将版本化快照和校验摘要用 Argon2id 派生密钥、XChaCha20-Poly1305 认证加密。`ledger-export` 在一致性快照上分页查询，用字符串金额生成 CSV/JSON，导出不写库。Tauri 的 `data_management.rs` 管理十分钟恢复预览，React `features/data/` 分开备份、恢复和筛选导出；Android 的 `DataFilesPlugin` 经系统 Storage Access Framework 打开和保存用户选择的文件。

- `ledger-domain/src/crypto.rs` 定义独立于法币 `Money` 的加密数量、双资产转换腿、来源记录和核对状态。数量保留十进制字符串，成交实际报价资产使用 tradeQuoteCcy 以兼容统一 USD/USDC 订单簿，报价腿由 checked i128 十进制乘法计算，溢出明确拒绝。
- Schema 15 的 `crypto_watchlist` 和 `crypto_records` 保存自选及带 OKX UID 的加密核对记录；7 天和归档账单共用来源 ID。重导保留人工状态，已确认记录不被远端覆盖，未完成充提不能确认；不写入法币 transactions/postings。新增表纳入可信快照、加密备份、恢复校验和全量清理，旧版本快照逐级迁移。
- `connectors/markets/okx/` 将公共行情、账户与估值、分页历史和合约分离，沿用有界 HTTP 客户端和完整查询串 HMAC-SHA256 签名。`src-tauri/src/crypto/commands.rs` 在后台请求前和提交结果时检查应用锁及 import_generation，会话被锁定、清空、恢复或更换 OKX 凭据后旧请求不可提交，历史分页绑定 UID，网络不持账本互斥锁，私有请求复用系统凭据存储。
- `features/markets/binance/` 和 `ifind/` 用类型适配器将现有官方 API 调度结果转换为行情、股票列表、余额与 K 线，`MarketWorkspace` 将用户页面与专家接口目录组合为标签页。`shared/MarketChart` 处理分页去重、图表和拖动选择，`useResource` 隔离旧参数及离开页面后的响应。iFinD A 股日期按 Asia/Shanghai 显示，K 线明确使用不复权口径。行情金额保留接口字符串；币安账户余额遵守全局金额隐藏状态。
- 非私密自选代码及显示名称以平台独立的 `meowm.market.<provider>.watchlist.v1` 键保存在设备 WebView localStorage，校验代码并限制 100 项，保存失败显示错误，不保存凭据或账户数据；此偏好不纳入账本备份。币安现货目录和 24h 行情的公共 GET 响应上限为 16 MiB，iFinD 智能选股上限为 8 MiB，其余接口保留原有 512 KiB 上限。
- `connectors/markets/okx/orders.rs` 只使用固定的订单、查询、历史和撤单端点；下单前按实时产品规则和账户持仓校验，模拟盘与实盘各自读取独立 Trade 凭据。未知下单结果保留同一 `clOrdId` 供查询，不自动重试，也不保存到账本。
- React `features/markets/MarketsPage.tsx` 按交易所凭据状态分层展示目录、API 设置和对应工作区；切换交易所时回到列表，当前工作区只挂载已选平台。`features/markets/okx/` 提供免密公共行情、自选和 SVG K 线、资产总览、独立加密流水、合约表现及逐笔确认的现货/合约交易面板。只读、模拟盘交易和实盘交易凭据分开保存；交易默认使用模拟盘，实盘有独立风险提示和二次确认。分页需显式继续读取；持仓净盈亏按结算币种精确相加，费用仅作为已经包含在净盈亏中的分项展示。行情采用手动 REST 刷新，不依赖 API Key；没有真实账户凭据的协议测试不能替代私人账户联调。
