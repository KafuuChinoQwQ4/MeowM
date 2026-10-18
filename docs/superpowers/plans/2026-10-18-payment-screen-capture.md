# 支付成功页面识别实施记录

依据：[支付成功页面识别设计](../specs/2026-10-18-payment-screen-capture-design.md)。用户批准在 `main` 实现并推送 GitHub，且先合并清理分支。

- [x] 确认所有功能分支均已包含于 `main`，删除干净的多余 worktree 和已合并分支；远端仅保留 `main`。
- [x] 新增 `screen_capture` 来源和有界 OCR 事件合同，解析成功标记、人民币金额及商户，生成 Pending。
- [x] migration `0010_screen_capture.sql` 与 schema 10：独立来源设置/状态/收据、原子候选写入、快照和数据清理。
- [x] 精确事件 ID 去重；跨通知与截图的同额短时匹配仅提示疑似重复，保持不同支付可审查。
- [x] MediaProjection 授权、通知权限、使用情况访问检查、本地中文 OCR、受限帧缓存、单个投影 display、旋转和停止清理。
- [x] Tauri 会话连接现有隐私锁；锁定、停止、删除、恢复使会话失效。
- [x] 现有通知采集页增加来源开关、默认平台、授权、启动/停止与结果提示；轮询保留未保存草稿和操作错误。
- [x] Rust、Kotlin、浏览器回归测试与 Android ARM64 调试包构建。
- [x] 最终目录索引、格式检查和源码快照敏感信息扫描。

提交时再次扫描暂存内容与完整 Git 历史，推送后核对远端 `main` 与本地提交一致。

实际检查命令：`cargo test --workspace`、`cargo fmt --all -- --check`、`npm run build`、`npm run test:ui`、`npm run format:check`、Android `testUniversalDebugUnitTest` 与 Tauri `android build --debug --apk --split-per-abi --target aarch64 --ci`。

真实微信/支付宝支付、厂商长期后台策略和正式签名发布包不在本轮自动化验证结果内。
