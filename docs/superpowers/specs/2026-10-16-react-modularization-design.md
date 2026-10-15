# React 迁移与模块解耦设计（已确认）

## 目标与范围

在九项功能计划的任务 2 完成版本 `3024328` 上，将现有 Tauri 前端迁移到 React + TypeScript，并按业务职责拆分前端、Tauri 适配层和 SQLite 实现。完成本次重构、验证并推送 GitHub 后停止，任务 3 另行开始。后续任务草稿继续保留在本地 stash，本次不恢复或发布。

## 当前问题

- `src/main.ts` 446 行集中维护导航、全局状态、账本、明细、平台、待确认和导入，以及 DOM 创建与事件注册。
- `src/transactions.ts` 仅 48 行，但长行压缩了整个交易弹窗、表单、验证和影响预览，实际职责过多。
- `src/api.ts` 将 DTO、IPC 调用和响应转换集中在一个文件。
- `src-tauri/src/ledger.rs` 1220 行混合账户、日账单、分页、导入会话、待确认与测试。
- `ledger-storage/src/sqlite.rs` 1626 行混合连接、迁移、行映射、不同 repository 接口、撤销与审计。

## 方案选择

采用完整 React 页面迁移，并对两个 Rust 大文件做模块化拆分。只加 React 容器、内部继续调用旧 DOM 渲染器，不能解决状态和生命周期耦合；同时引入路由框架或全局状态库会扩大迁移面，现有四个导航视图由应用壳管理即可。

使用现有 Vite/Tauri 工程，安装 `react`、`react-dom`、对应 TypeScript 类型和匹配当前 Vite 的 React 插件，更新 lockfile。由 `createRoot` 挂载应用，业务界面全部使用 JSX、受控表单和 React 生命周期。

## 前端边界

- `src/main.tsx`：加载样式、挂载 React，不保存业务状态。
- `src/app/App.tsx`：应用壳，组织当前导航、平台和日期，协调刷新及弹窗入口；导航和工具栏拆成独立组件。
- `src/components/`：共用 Dialog、表单字段、分页、加载与错误状态组件。Dialog 统一处理焦点、Escape、浏览器返回和保存时关闭保护，清理监听器。
- `src/features/accounts/`：平台列表、分类筛选、平台编辑表单和账户加载逻辑。
- `src/features/ledger/`：日账本、收入/支出/净额卡片、参考余额表单和按平台/日期读取的 hook。
- `src/features/transactions/`：明细列表、详情、来源/关联/审计、编辑表单、影响预览和撤销确认。金额计算继续使用整数分或 BigInt。
- `src/features/pending/`：筛选、分页、选择、批量处理、候选编辑和疑似重复展开。状态仅存在所属页面和 hook 中。
- `src/features/import/`：文件选择、预览、逐行修订和确认加入待确认。
- `src/api/`：各业务的 DTO 和调用模块；集中处理 Tauri invoke，组件不直接调用 invoke。删除重复的分类调用封装，公开调用保持类型完整，避免 `Record<string, unknown>` 写入接口。
- `src/styles/`：将样式按全局基础、应用壳、账本、表单弹窗和待确认等职责拆分，保留现有视觉与窄屏布局。
- `src/shared/`：纯格式化、错误信息和业务标签；避免将页面状态或各业务数据请求放进万能工具文件。

共享只限于真实共用组件；业务表单不为减少代码行而强行统一。文件按正常排版组织，通常控制在约 100–300 行；超过约 350 行时优先检查是否混入另一职责，不使用压缩长行规避拆分。

## Rust 边界

保留 `Ledger` 与 `SqliteLedgerRepository` 的公开入口和已有命令名称、参数、DTO 字段，调用方不需要同时改协议。

- `src-tauri/src/ledger/mod.rs`：账本上下文、打开数据库、子模块组织。
- `ledger/accounts.rs`、`day.rs`、`query.rs`、`imports.rs`、`pending.rs`：各自 DTO 与适配逻辑；金额/日期/ID 解析放入明确的解析模块。
- `ledger/tests/`：按账户/日账单和导入/待确认拆出原测试。
- 交易适配层保留独立职责，将测试从生产实现分离。
- `ledger-storage/src/sqlite/mod.rs`：连接与模块组织，继续导出原 repository 类型。
- `sqlite/migrations.rs`、`accounts.rs`、`transactions.rs`、`classification.rs`、`pending.rs`、`imports.rs`、`reversals.rs`、`mapping.rs`：分别负责迁移、平台/账户、交易持久化、分类规则、待确认事务、导入收据、撤销审计和数据库行映射。

拆分使用子模块和受限可见性；不能为了迁移便利把内部数据库连接和解析函数全部公开。所有 SQL 写入事务、乐观锁、来源证据保护和撤销幂等性沿用原实现。

## 行为与兼容性

- 数据库 schema 保持 version 5，不增加迁移，不改变现有数据和 ID。
- 自动/导入记录先进入 Pending；确认后才影响余额与报表。
- 保留四个导航入口、账户/日期切换、分页、平台创建编辑、参考余额、账单预览与修订、候选确认/忽略/恢复、交易编辑与撤销。
- 异步请求按参数和生命周期防止旧响应覆盖新页面；重复提交受保护。
- 保存错误显示在当前表单且保留输入；分类读取失败可重试；日期金额校验沿用现有约束。
- 可见文本通过 React 文本节点展示，不将账单、备注等外部文本作为 HTML 插入。

## 验证和交付

先在旧版本运行现有 Rust 与浏览器流程建立基线；迁移后复用并扩展行为验证，重点覆盖平台和参考余额、导入预览修订、候选确认刷新账本、交易编辑版本与预览、撤销与关联跳转，避免只测组件是否存在。

执行 Rust workspace 测试、TypeScript 检查与生产构建、浏览器模拟 IPC 测试、金额日期测试、Rust 格式检查、针对改动模块的 Clippy、目录索引检查和 diff 检查。浏览器 IPC 模拟与 SQLite 测试分别报告；Android 真机验证不在本环境声称通过。

更新架构文档与目录说明。上传前扫描上传文件、暂存内容和提交历史，推送后核对 GitHub `main` SHA。本次只由主代理完成，交付后停止。
