# 本地优先个人收支管理

安卓优先、离线可用的个人账本。Rust 工作区承载领域模型、账务用例、导入解析、通知解析和备份逻辑；Tauri + TypeScript 提供界面，Android 使用 Tauri WebView 壳。

## 目录约定

每个目录的 `tree` 只描述该目录的直接文件。新增或删除文件时，先更新 `scripts/tree_descriptions.json`，再运行 `python3 scripts/update_tree.py`；用 `python3 scripts/update_tree.py --check` 检查索引。生成目录 `gen`、构建产物及已退役界面目录不生成索引，避免将 `tree` 当成 Android 资源编译。

账本核心和每日收支界面已接通 SQLite。导入、通知解析和备份模块尚未接入新界面。

## 收支与可选参考余额

新平台默认不记录初始金额，日末余额显示“尚未设置”，收入、支出和净额仍按已确认流水统计。设置参考余额时，金额代表所选本地日期的日末余额。例如 4 号日末为 100 元，5 号净收入 20 元，则 5 号日末为 120 元；若 4 号净收入 15 元，则 3 号日末为 85 元。

每个平台独立保存一个参考金额和时间，再次设置会替换参考点；清除参考不会删除流水。参考金额本身不产生流水，查询根据最新的已确认流水推算。余额支持零和负数，金额使用整数分计算。数据库自动升级到版本 3，保留旧账户金额、时间和流水，不会自动清理旧账本。

## 运行界面

```bash
cd apps/tauri-ui
npm ci
npm run tauri -- dev
```

桌面需要 Tauri 的系统构建依赖。数据库位于 Tauri 应用数据目录下的 `ledger.sqlite3`，Android 使用应用私有存储。首次启动创建微信、支付宝、现金三个无参考余额的平台。「新建平台」可添加其他平台，顶部选择平台和日期，「记一笔」保存所选日期的收支（本地中午），「设置参考余额」保存所选日期的日末参考金额。当前手动录入使用人民币。

## Android 打包

需要 JDK 17、Android SDK 35 和 36（Tauri Android 库需要 36）、NDK（已验证 26.1.10909125）和 Rust Android target。Gradle 由 Android 工程的 wrapper 提供。

```bash
export ANDROID_HOME=/path/to/android-sdk
export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk/26.1.10909125"
export JAVA_HOME=/path/to/jdk17
export PATH="$JAVA_HOME/bin:$PATH"
rustup target add aarch64-linux-android x86_64-linux-android
bash scripts/build_android.sh arm64-v8a
# 模拟器
bash scripts/build_android.sh x86_64
```

调试 APK 位于 `apps/tauri-ui/src-tauri/gen/android/app/build/outputs/apk/` 下的架构目录，安装时指定对应设备与实际文件路径：

```bash
adb -s DEVICE_SERIAL install -r /path/to/app-debug.apk
adb -s DEVICE_SERIAL shell am start -n com.example.localledger/.MainActivity
```

应用 ID 沿用 `com.example.localledger`。覆盖安装保留数据，旧账本清理需单独执行且只针对指定设备；启动逻辑不删除数据。

## 验证

```bash
cd apps/tauri-ui
npm run build
node --experimental-strip-types --test tests/format.test.ts
cd ../..
cargo test -p ledger-tauri-ui --lib
python3 scripts/update_tree.py --check
```

Rust 接口测试使用真实 SQLite，覆盖默认无余额、日末参考金额、双向推算和重启保存；Android 仍需安装 APK 验证实际 WebView 到 Rust 的调用。

## 平台管理

左下角打开导航，进入「平台管理」可新建平台、重命名、修改分类及切换到账本。支持支付钱包、银行账户、证券账户、虚拟币交易所、现金、其他六类；列表支持分类筛选和分页。分类存入 SQLite 的 `platform_metadata` 表，重命名保留原账户 ID 和全部流水。旧微信/支付宝默认为支付钱包，现金默认为现金，其他旧平台默认为其他。当前仍按人民币记账，分类不包含多币种换算、持仓或行情同步。
