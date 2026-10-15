# 第四阶段：Android 通知采集验收

验收日期：2026-10-16。范围对应九功能计划 Task 4，不包含下一阶段的周期账单。

## 实现范围

- 微信、支付宝来源默认关闭，开启时需明确同意用途；支持逐来源开关、默认人民币平台和系统设置跳转。
- 系统通知经 Kotlin 有界后台队列传入 JNI；解析、金额、方向、Pending 写入和来源摘要均由 Rust 处理。
- 收款、消费、退款及转账进入待确认；不确定或缺失金额不猜测，退款需关联原消费，转账需指定两个不同平台。
- SQLite 自动升级到版本 6，保存规范化结果及通知 key/内容的摘要。候选、证据、收据在同一事务提交；设置变化会在提交前复核。原始正文和通知 key 不保存或记录日志。
- 服务重连重放当前活动通知，通知 key、内容摘要及发布时间参与幂等识别；重复回调返回原候选。发布时间不同的事件不会仅因商户和金额相同而合并。
- React 设置、来源卡片、IPC、样式及 Rust 解析、持久层、JNI 分别独立模块。刷新不清空未保存的来源草稿，转账候选重新打开保留转入平台。

## 自动验证

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace` | 118 项通过 |
| `cargo fmt --all -- --check` | 通过 |
| 通知、应用、存储三 crate 的 Clippy，`--all-targets -- -D warnings` | 通过 |
| `cargo clippy -p ledger-tauri-ui --all-targets --no-deps -- -D warnings` | 通过 |
| `npm run build`、`npm run format:check` | 通过 |
| `npm run test:ui` | 12 项通过，浏览器使用真实 React，模拟 Tauri IPC |
| 金额与日期辅助测试 | 3 项通过 |
| Android instrumentation | API 35 x86_64 模拟器，3 项通过 |
| Tauri Android 调试构建 | ARM64、x86_64 两份 APK 成功 |

全工作区严格 Clippy 仍报告既有 `ledger-import` 中的参数数量、连续 replace、可合并 if 等告警；本次未扩展修改此前导入实现。生成的 Android 框架还存在原有 Gradle SDK 兼容性及弃用提示，未阻止构建。

Rust 测试使用真实 SQLite，覆盖默认关闭、缺失字段、负号/异常/中文金额、重启幂等、忽略后重复事件、关闭来源、未知包名、来源私密内容不落库、转账编辑重开及确认。Android 三项测试分别覆盖受限 JNI 输入与来源开关、真实 Kotlin 回调和服务对象重建、服务权限声明与共享数据库目录；使用隔离的临时数据库。

## Android 系统链路实测

使用 JDK 17、SDK 35/36、NDK 26.1.10909125，API 35 模拟器。覆盖安装保留原账本，验收前备份该模拟器账本，结束后恢复，不把测试候选留在原账本中。

1. 安装 x86_64 调试 APK。实际 WebView 调用 `notification_state` 返回两个来源默认关闭，未授权时 `notification_access` 返回 granted=false、connected=false。
2. 从实际 WebView 调用系统设置跳转成功。通过模拟器的 `cmd notification allow_listener` 授予精确服务组件后，实际 WebView 返回 granted=true、connected=true。此项通过 ADB 授权，未自动点击系统设置确认对话框。
3. 使用临时安装的合成通知 fixture（包名 `com.tencent.mm`，标注为测试应用；模拟器事先没有安装真实微信）经 Android NotificationManager 发布两次发布时间不同的“向模拟咖啡店付款12元”。系统监听服务自动生成两条 Pending，金额为 12.00，来源为 notification，未确认入账。
4. 强停并重启应用，当前活动通知被重新分发，最近结果变为 duplicate，候选数量仍为两条。实际待确认页面显示模拟商户；WebView reload 后候选数量不变。
5. 关闭微信来源，再发布第三条通知，候选数量仍为两条。撤销系统权限后实际 WebView 返回 granted=false、connected=false。
6. 卸载临时 fixture、撤销本次监听授权并恢复验收前账本。fixture 及临时签名密钥不进入仓库、APK 或 Git 历史。

Android instrumentation 可重复运行：

```bash
cd apps/tauri-ui
npm run tauri -- android build --debug --apk --target aarch64 x86_64 --split-per-abi --ci
cd src-tauri/gen/android
# 使用刚刚构建的 JNI 库；直接运行 Gradle 不会自动配置 Tauri 的 NDK linker 环境。
./gradlew :app:assembleUniversalDebugAndroidTest \
  -PabiList=x86_64 -ParchList=x86_64 -PtargetList=x86_64 \
  -x :app:rustBuildUniversalDebug -x :app:rustBuildX86_64Debug
adb install -r app/build/outputs/apk/x86_64/debug/app-x86_64-debug.apk
adb install -r app/build/outputs/apk/androidTest/universal/debug/app-universal-debug-androidTest.apk
adb shell am instrument -w -r \
  -e class com.example.localledger.NotificationCaptureTest \
  com.example.localledger.test/androidx.test.runner.AndroidJUnitRunner
```

## 验证边界

系统链路使用合成通知，未进行真实微信/支付宝支付，也未覆盖各厂商后台限制、锁屏隐藏正文或长期保活。ARM64 APK 已构建，未安装在 ARM64 真机上。通知只能作为辅助输入，来源格式变化或系统限制可能造成漏采，应使用账单导入补齐。后台队列最多等待 64 个事件，过载时允许丢弃；重连仅重放仍在通知栏中的通知。
