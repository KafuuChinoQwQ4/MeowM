# MeowM 本地优先个人账本

MeowM 是一款以 Android 为主、离线可用的个人收支管理应用。账本数据保存在本机 SQLite 中，界面使用 Tauri、React 和 TypeScript 构建，账务逻辑由 Rust 处理。目前以人民币记账，金额按整数分计算。

## 功能

- **账本与平台：** 按日期查看收支和余额，管理钱包、银行卡、现金等平台，记录收入、支出、转账和退款。
- **流水与待确认：** 查看、修改流水；导入的账单和采集到的交易先进入待确认，核对后再计入账本。
- **账单导入：** 支持微信、支付宝和银行卡的 CSV、XLS、XLSX 文件，预览记录并识别重复项。
- **财务管理：** 查看收支报表、管理预算和周期账单，搜索流水、整理分类规则，并按平台余额和流水进行对账。
- **Android 采集：** 经用户授权后，可读取支持的支付通知或识别支付页面；识别结果仍需在待确认中核对。
- **安全与数据：** 支持应用锁、自动锁定、隐藏金额、加密备份与恢复，以及 CSV/JSON 导出和数据清理。

## 使用

启动应用后，可从底部导航进入「账本」「明细」「待确认」「财务管理」或「设置」。先选择或创建平台，再记账或导入账单；在待确认中核对并确认记录后，流水才会影响余额和报表。通知采集和支付页面识别可在「设置 → 平台与采集」中配置，使用时需要授予相应的 Android 系统权限。

桌面开发需要 Rust、Node.js/npm 和 Tauri 所需的系统依赖：

```bash
cd apps/tauri-ui
npm ci
npm run tauri -- dev
```

构建 Android 调试 APK 需要 JDK 17、Android SDK/NDK 及对应的 Rust target。配置 `ANDROID_HOME` 和 `ANDROID_NDK_ROOT` 后运行：

```bash
rustup target add aarch64-linux-android
bash scripts/build_android.sh arm64-v8a
```

详细 Android 构建参数见 [`scripts/build_android.sh`](scripts/build_android.sh)，项目模块说明见 [`docs/architecture.md`](docs/architecture.md)。
