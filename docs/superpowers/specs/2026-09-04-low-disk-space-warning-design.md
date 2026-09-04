# 磁盘剩余空间不足告警设计

日期：2026-09-04

状态：已确认设计，随实现一起提交

## 目标

磁盘写满时 AI Switch 会以各种难以归因的方式出错：SQLite 写入失败、配置快照落不下去、迁移冲突时的数据库备份复制不动。这些错误现在只在具体操作失败时才暴露，用户看到的是「保存失败」而不是「磁盘满了」。

本次增加主动检测：应用运行期间定期检查关键卷的剩余空间，低于 1 GiB 时在界面上给出告警，让用户在真正写坏之前先腾空间。

## 范围

- 新增后端只读命令 `get_disk_space_status`，桌面端与独立服务器都可调用。
- 新增前端全局告警条，挂在 `src/App.tsx` 与 `AutoUpdatePrompt` 同级。
- 不改变任何写入路径的行为：不因空间不足阻止写入、不预留空间、不自动清理备份。
- 不新增设置项：阈值是常量。
- 不新增后台 Rust 定时任务。

## 监控哪个卷

用户的原话是「系统盘」。实现上监控两个候选路径所在的卷，按卷去重：

1. **系统盘根目录** —— Windows 取 `%SystemDrive%\`（拿不到时退回 `C:\`），Unix 取 `/`。
2. **应用数据目录** `~/.ai-switch`（`AppPaths::data_dir`）—— 数据库、配置快照、备份、日志都在这里，它满了才是真正让软件出错的那一个。

两者在 Windows/macOS 的标准安装下是同一个卷，去重后只剩一条。但 Linux 上 `/home` 常常是独立分区，只查 `/` 会漏掉真正装着 `~/.ai-switch` 的那个卷，所以两个都查。

去重键按平台取：Windows 用路径的盘符前缀（大写，如 `C:`）；Unix 用 `std::fs::metadata().dev()`，即内核认定的设备号，比字符串前缀准确。

探测前先把路径回退到**最近一个已存在的祖先目录**：首次启动时 `~/.ai-switch` 可能还没建出来，直接探测不存在的目录在两个平台上都会失败。

## 实现方式

### 后端

新增 `src-tauri/src/models/disk_space.rs`：

```rust
pub struct DiskVolumeSpace {
    pub label: String,           // C:  或 Unix 挂载点
    pub path: String,            // 实际探测的路径
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub low: bool,
}

pub struct DiskSpaceStatus {
    pub threshold_bytes: u64,
    pub low: bool,               // 任一卷 low 即为 true
    pub volumes: Vec<DiskVolumeSpace>,
}
```

字段用 serde 默认的 snake_case，与仓库里多数输出模型一致。

新增 `src-tauri/src/services/disk_space_service.rs`，阈值为常量 `LOW_DISK_SPACE_THRESHOLD_BYTES = 1 GiB`。取剩余空间不引入任何新的第三方 crate：

- Windows：`GetDiskFreeSpaceExW`。`windows-sys` 已经是本项目的直接依赖，且已启用正好包含这个函数的 `Win32_Storage_FileSystem` feature；宽字符串与错误处理照 `src-tauri/src/config_writer/platform.rs` 现成的写法。
- Unix：`libc::statvfs`，可用空间取 `f_bavail * f_frsize`（`f_frsize` 为 0 时退回 `f_bsize`）。`libc` 已在依赖树里，只需在 `[target.'cfg(unix)'.dependencies]` 声明。
- 其他平台：返回不支持错误，该候选被跳过。

用 `f_bavail`（非特权用户可用）而不是 `f_bfree`（含保留块），因为前者才是应用真正能写进去的量。

单个卷探测失败不算错误，只跳过并打 `eprintln!`；命令本身不返回 `Result`，任何环境下都不会因为拿不到磁盘信息而让界面报错。

命令注册三处：`commands/disk_space_commands.rs`、`lib.rs` 的 `use` 与 `generate_handler!`、`web/handlers/mod.rs` 的 `dispatch_command` 分支（独立服务器和 Web 端靠它，缺了会被 `tests/transport/command-contract.test.ts` 拦下）。

### 前端

- `src/lib/api/types.ts` 增加两个类型，`src/lib/api/client.ts` 增加 `getDiskSpaceStatus()`。
- `src/lib/query/diskSpace.ts`：`useDiskSpaceStatus()`，`refetchInterval` 为 5 分钟。选 React Query 而不是自己写 `setInterval`，因为它自带去重、窗口聚焦刷新和卸载清理。
- `src/components/system/LowDiskSpaceBanner.tsx`：`status.low` 为真时渲染固定定位的红色告警条，列出每个告警卷的剩余量与总量，带关闭按钮。
- `src/lib/byteSize.ts`：新增 `formatByteSize()`，到 GB 量级。现有 `AutoUpdatePrompt`/`UpdatesScreen` 里各有一份只到 MB 的私有实现，那是给下载进度用的，本次不动。

告警条用固定定位覆盖而非插入布局流，避免撑动侧栏和内容区。

### 关闭与重新弹出

关闭只写组件内 state，不做持久化：磁盘满是必须处理的问题，不该被一次点击永久静音。重新弹出的条件是 `low` 由 false 变为 true —— 即用户腾出空间后又掉回阈值以下，或者重启应用。同一次持续告警不会反复弹。

## 行为

- 剩余 ≥ 1 GiB：无告警条，无任何提示。
- 任一监控卷剩余 < 1 GiB：显示告警条，文案列出盘符/挂载点与剩余量。
- 多个卷同时告警：一条告警条内列出多行。
- 探测失败（权限、平台不支持）：静默跳过，不显示错误。
- 非桌面（Web）环境：同样生效，报告的是服务器主机的磁盘。

## 测试

Rust（`disk_space_service.rs` 内联 `#[cfg(test)]`）：

- 阈值给 0 时不告警，且返回的卷总容量大于 0、可用空间不大于总容量。
- 阈值给 `u64::MAX` 时每个卷都告警，`status.low` 为真。
- 返回的卷按 label 去重，没有重复项。
- 数据目录尚不存在（临时目录下的深层子路径）时仍能探到所在卷。

前端（`tests/LowDiskSpaceBanner.test.tsx`）：

- `low` 为真时渲染告警条并显示剩余量。
- `low` 为假时不渲染任何东西。
- 点关闭后消失。
- 恢复到充足后再次不足时重新出现。

## 验收标准

- 系统盘或数据目录所在卷剩余空间低于 1 GiB 时，应用界面出现告警。
- 应用持续运行时每 5 分钟复检一次。
- 关闭告警后不会在同一次告警里反复弹出。
- 独立服务器 / Web 端调用同一命令能拿到结果。
- `pnpm typecheck`、`pnpm test:run`、`cargo test`、`cargo fmt --check` 通过。

## 非目标

- 不在空间不足时阻止或改写任何写入操作。
- 不提供阈值配置项。
- 不做系统级通知（应用未打开时不告警）。
- 不自动清理 `backups/` 或日志。
