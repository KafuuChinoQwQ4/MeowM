# 构建产物与磁盘空间管理

## 保留规则

- 保留主工作树的 Cargo `target` 依赖、增量编译和构建脚本缓存，以及 Cargo 下载缓存、npm 缓存、Gradle 缓存、Android SDK/NDK/JDK 和模拟器镜像。
- 每个架构只保留一份最近验证的 APK，统一放在忽略目录 `build/latest/`；新的验证版本取代旧版本，不按阶段无限归档 APK。
- 阶段提交进入主分支并完成验证后，检查阶段工作树无未提交修改，再移除该阶段重复的 `target`、Android `app/build` 和独立 `node_modules`。源码、Git 分支、测试证据、数据库和备份不属于构建清理范围。
- 优先在主工作树复用依赖缓存。临时工作树的构建结束后及时清理重复产物；如果共享 `CARGO_TARGET_DIR`，需注意 Tauri Android 构建脚本会生成与工作树路径相关的 Kotlin 文件，切换工作树后必须验证这些文件可用。
- Rust 测试可执行文件允许清理同名旧哈希副本，保留最近验证的一份；不要把 `.rlib`、`.rmeta`、动态库、`incremental`、`.fingerprint` 或构建脚本目录当成旧测试文件删除。
- 清理前确认没有进行中的构建，检查明确路径下没有 Git 跟踪文件。日常维护不运行全量 `cargo clean`，也不清空 SDK 或全局缓存。

## 本次清理（2026-10-16）

已删除第八、九阶段独立 Rust 目标目录，第七至九阶段 Android 构建输出、退役 Android 壳构建目录、历史 APK，以及 186 个旧 Rust 测试/可执行文件副本。主工作树 Cargo 缓存仍约 28 GiB，全局依赖与工具链保留。WSL 文件系统占用减少约 21.7 GiB；Git 源码和现有分支未改动。

最新保留包为 `build/latest/meowm-stage9-x86_64-debug.apk`，SHA-256 为 `8f03bdb275662138008eb01ec4973d997214ec653956ba0523fce7f5fa35c09f`。该包来自第九阶段最终构建。

## 将 WSL 空闲空间返还 Windows

当前 WSL 磁盘位于 `D:\WSL\archlinux\ext4.vhdx`。删除 Linux 文件后，动态 VHDX 不一定自动缩小。已经执行 `fstrim -v /`；当时 D 盘仍剩约 4.7 GiB，后续还需要离线压缩。

先保存工作并关闭 WSL 终端、编辑器远程连接及模拟器等相关任务。在 **Windows 管理员 PowerShell** 中执行：

```powershell
wsl --shutdown
diskpart
```

进入 `DISKPART>` 后执行：

```text
select vdisk file="D:\WSL\archlinux\ext4.vhdx"
compact vdisk
exit
```

压缩仅针对已分离或只读连接的动态虚拟磁盘；如果提示占用，关闭重新启动 WSL 的程序后再试。完成后检查 D 盘可用空间，再启动 WSL。不要删除 VHDX，也不要用调整分区大小代替压缩。本次没有执行关闭 WSL 或离线压缩，因为这会终止当前开发会话及其他 WSL 任务。

参考：[Microsoft compact vdisk](https://learn.microsoft.com/windows-server/administration/windows-commands/compact-vdisk)、[Microsoft WSL 磁盘说明](https://learn.microsoft.com/windows/wsl/wsl2-mount-disk)。
