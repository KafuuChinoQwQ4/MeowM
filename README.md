# 本地优先个人收支管理

安卓优先、离线可用的个人账本。Rust 负责记账、SQLite、通知/CSV 解析、去重、分类、报表与加密备份；Slint 提供账户、交易、待确认、报表和设置界面，Android 接入通知权限、系统文件选择、Keystore 与可选系统凭证应用锁。无需账号、服务端或 Token。

首次使用在设置创建账户，填写期初余额和基准日期，再手工记账或确认通知。通知默认关闭、默认 Pending，未确认和忽略记录不改变余额/报表。微信与支付宝 UTF-8 CSV 经系统文件选择、预览、处理退款与疑似重复后明确确认导入。疑似重复可人工关联已有交易保留多份来源证据，或声明独立交易；重复文件/通知/确认可安全重放。退款保留原支出并抵减净支出，转账与信用卡还款不重复计入消费。外币账户显示原币余额，报表使用保存的 CNY 估值。

设置支持分类与商户/关键词规则、来源开关、默认账户、加密备份/恢复和删除。删除单笔、批次、全部本地数据均需明确确认。备份是完整加密导出，恢复完整覆盖；错误密码不会改账本。参见 [架构](docs/architecture.md)、[隐私、权限、删除及日志策略](docs/privacy-and-deletion.md) 与 [发布前验证记录](docs/validation.md)。

## 构建运行

需要 Rust stable、桌面 Linux 图形/字体依赖；Android 需要 JDK 17、Gradle 8.7、Android SDK 35、NDK 与 Rust Android targets：

```sh
cargo run -p ledger-slint-ui
# 默认当前目录 ledger.sqlite3；指定数据库：
MEOWM_DATABASE=/path/to/ledger.sqlite3 cargo run -p ledger-slint-ui
rustup target add aarch64-linux-android x86_64-linux-android
# 配置 JAVA_HOME、ANDROID_SDK_ROOT；可选 ANDROID_NDK_ROOT、CARGO_TARGET_DIR
cd apps/android
gradle assembleDebug testDebugUnitTest
gradle assembleRelease                 # 未签名 release APK
adb install -r app/build/outputs/apk/debug/app-debug.apk
gradle connectedDebugAndroidTest       # 需要在线 emulator/device
```

Gradle 为 arm64-v8a 与 x86_64 编译 Rust release 库并封装。release 尚需自己的签名配置才能发行；不要把签名密钥/密码放进仓库。Windows NDK host 配置已支持，但 native Windows 编译未经本次环境验证。

内存有限的 WSL 环境请设置 `CARGO_BUILD_JOBS=1`，按顺序运行构建和测试。Android 构建默认单 worker、单个 Rust 编译任务，并在 Gradle 进程内编译 Kotlin，避免多个常驻 JVM。先完成 APK 构建，再启动模拟器验证，不同时运行两者；将工作树和 `CARGO_TARGET_DIR` 放在持久磁盘，避免 `/tmp` 被重启清空。运行期间关注可用内存和 swap，资源不足时停止当前构建后重试。

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
python3 scripts/update_tree.py --check
```

新增/删除文件时更新 `scripts/tree_descriptions.json` 并运行 `python3 scripts/update_tree.py`。每个目录的 `tree` 只列直接文件。

## 当前边界

- 微信/支付宝真实通知格式及厂商后台限制需要各自设备验证；不能保证每笔支付都捕获。银行通知及通用银行 CSV 解析器尚未提供。
- API 仅有独立接口和离线合成 fixtures，无真实平台连接、OAuth、网络请求或后台同步；开关只记录未来意向。
- CSV 转账尚无双账户映射、缺少 CNY 估值的外币行需排除后手工补录，不会半批写入。报告按设备当时 UTC offset 计算月份，不是历史 IANA 夏令时规则引擎。
- Android 系统自动云备份/设备迁移被禁用，手动加密备份由用户自行保管。应用锁只保护 UI，SQLite 依赖设备私有存储与系统磁盘加密。
- KDF/大账本恢复在 UI 回调执行，可能短暂阻塞界面；第三方 SAF 提供器不保证原子文件发布。
