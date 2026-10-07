# 发布前验证记录

验证日期：2026-10-07。验证分支：`feat/local-expense-next-phases`。功能代码对应 `7bbab0e`，构建资源配置对应 `62a6b9f`。所有测试使用脱敏或合成数据；设备文件导入、备份与恢复流程关闭网络。

## 自动化验证

- `cargo test --workspace --locked`：153 个测试通过，其中 Slint 19 个。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`：通过。
- `cargo fmt --all -- --check`、`python3 scripts/update_tree.py --check`、`git diff --check`：通过。
- Android Gradle：`assembleDebug assembleRelease testDebugUnitTest assembleDebugAndroidTest` 通过，采用单 worker、无 daemon、Kotlin 进程内编译和 Rust 单任务。Kotlin 单元测试 7 个通过。
- debug APK SHA-256：`3cbccd23a2816d187e839870e9e2dcae53ada6b3d3a41030b23432a5e3cbe3d6`。
- unsigned release APK SHA-256：`ca58a246519e33cef82b77345a524718cf233a3333029502c4f470118134f991`。两个 APK 都含 `arm64-v8a` 和 `x86_64` 的 Rust bridge/UI 库；产物扫描未发现 fixture、账单样本、SQLite 文件、通知测试正文或已知合成密钥标记。

## API 35 设备流程

在 x86_64 API 35 模拟器上安装上述 debug APK，7 个 instrumentation 测试全部通过。实际 UI 还验证了：Pending 编辑并确认后余额和报表更新；通过系统 SAF 加密导出、取消选择器、错误密码拒绝且账本不变、正确密码恢复；系统凭证应用锁解锁和取消后的遮挡页面；WeChat CSV 通过 SAF 首次导入和同文件重复去重；明确确认全量删除后余额、报表归零且 Pending 清空。

实际观察值如下，全部使用测试账户 Wallet：

| 流程 | 观察结果 |
| --- | --- |
| Pending 修改金额后明确确认 | 支出 12.00，余额 -12.00；未确认行不计入报表 |
| SAF 加密导出后新增支出 5.00 | 导出文件 2125 字节；余额变为 -17.00 |
| 取消恢复文件选择、错误密码恢复 | 两次均保持 -17.00；错误密码显示拒绝提示 |
| 正确密码并明确确认恢复 | 余额恢复 -12.00，备份后的新增交易消失 |
| 系统 PIN 应用锁 | 正确凭证可进入；取消只显示不透明锁定页面 |
| SAF 导入合成 CSV 支出 3.00 | 支出从 12.00 变为 15.00 |
| 重复选择同一 CSV | 新增 0、重复 1，确认按钮禁用；支出仍为 15.00 |
| 明确确认全部删除 | 收入、支出、净支出为 0，Pending 为空；外部导出文件保留 |

仪器测试覆盖 JNI 回调与重放、ContentResolver 文件读写与超限保护、Keystore/JNI 往返/重启/篡改/删除，以及应用锁、自动备份禁用和凭据/缓存/采集设置清理。临时系统 PIN 已移除。删除后的主数据库保留为空账本，外部导出文件需用户另行删除。

本机完整设备记录位于工作树根目录 `.superpowers/sdd/2026-10-07-local-expense-manager-next-phases/device-acceptance.md`，未作为发布资源打包。仪器日志位于 `/root/.cache/meowm-tools/final-instrumentation.log`，构建日志位于 `/root/.cache/meowm-tools/final-reviewed-android-build.log`。

## 验证边界

本次没有实体 ARM64 手机、Windows 原生运行、release APK 安装或真实微信/支付宝通知及平台 API。支付平台适配器使用离线合成 HTTP fixtures；通知输入使用生产 JNI 边界注入的合成事件。银行通知和通用银行 CSV 仍是 README 中声明的边界。固定设备 UTC offset 的月份计算不等同于历史 IANA 夏令时规则引擎。

退款、跨来源合并、转账等边界依赖 Rust 回归测试，本次未逐项进行设备 UI 操作。320×640 窄屏存在较多滚动及局部横向溢出。通知标识升级不会重写旧开发数据库中的历史哈希，旧事件重放可能产生待人工核对的 Pending；不会宣称已恢复此前被旧去重规则丢弃的事件。发布包检查覆盖文件类型、已知测试标记及生产日志调用，不构成对任意未知密钥的绝对证明。

## WSL 资源控制

本次重型构建均由本机 `run-memory-guard.py` 运行，`CARGO_BUILD_JOBS=1`，Gradle 最大 worker 为 1，构建与模拟器没有并发。保护器在可用内存低于 768 MiB 或 swap 使用超过 2 GiB 时停止本次任务，启动前要求至少 1.5 GiB 可用内存。模拟器曾被保护器按阈值停止，WSL 未再次中断；之后单独重启完成剩余流程并关闭。最终可用内存约 4.4 GiB，无 qemu、Java、Cargo 或 rustc 重进程。源代码和编译缓存移到持久目录，避免依赖重启会清空的 `/tmp`。
