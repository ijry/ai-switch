# 会话系统终端恢复实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标：** 在会话详情页提供播放按钮，在当前项目目录打开系统终端并执行会话恢复命令。

**架构：** 前端通过 API 客户端调用新的 `open_session_terminal` 命令。该命令只注册在 Tauri 桌面端，由 Rust 按操作系统启动系统终端并把工作目录设置为会话项目目录；WebTransport 将通过 `desktopOnlyCommands` 在网络运行时拒绝调用。会话页负责展示按钮的可用状态、启动中状态和启动错误。

**技术栈：** React、TypeScript、TanStack Query、Lucide、Tauri 2、Rust `std::process::Command`、Vitest、Rust 单元测试。

## 全局约束

- 不复用应用内 `create_terminal_session` PTY；恢复必须打开用户系统终端。
- 只有同时存在 `projectDir` 和 `resumeCommand` 时按钮可用。
- Web 运行时不得向后端执行系统终端命令；命令列入 `desktopOnlyCommands`。
- Rust 必须校验项目目录存在且为目录，启动失败返回结构化 API 错误。
- 保持现有会话详情布局：更新时间、播放按钮、三点会话菜单均位于标题右侧。
- `docs/superpowers/plans` 文档使用中文，不提交代码或创建分支。

---

### 任务 1：定义 API 与跨运行时命令契约

**文件：**
- 修改：`src/lib/api/commandSupport.ts`
- 修改：`src/lib/api/client.ts`
- 修改：`tests/transport/command-contract.test.ts`

**接口：**
- 新增 `openSessionTerminal(input: { cwd: string; command: string }): Promise<void>`。
- `open_session_terminal` 加入 `desktopOnlyCommands`，因此 WebTransport 在发起 HTTP 请求前返回 `transport.desktop_only`。

- [x] 在客户端添加 `openSessionTerminal`，把 `{ cwd, command }` 作为命令参数发送。
- [x] 在契约测试中断言客户端命令已注册到 Tauri、明确列入桌面专用列表、且不要求 Web handler 注册。

### 任务 2：实现 Rust 系统终端启动命令

**文件：**
- 修改：`src-tauri/src/commands/session_commands.rs`
- 修改：`src-tauri/src/lib.rs`
- 修改：`tests/transport/command-contract.test.ts`

**接口：**
- 新增 `OpenSessionTerminalInput { cwd: String, command: String }`。
- 新增 Tauri 命令 `open_session_terminal(input) -> Result<(), ApiError>`。

- [x] 校验 `cwd` 和 `command` 非空、`cwd` 存在且为目录，并拒绝包含 NUL 的输入。
- [x] Windows 使用 `cmd.exe /K <command>`，macOS 使用 Terminal + `osascript`，Linux/Unix 使用可用终端模拟器执行 `sh -lc <command>`；所有启动进程继承会话项目目录。
- [x] 使用现有 `AppError::Validation` 和 `AppError::Filesystem` 返回可展示的错误码及细节。
- [x] 从 `lib.rs` 导入并加入 `tauri::generate_handler!`；不要加入 Web handler。
- [x] 为输入校验和终端命令选择添加 Rust 单元测试，避免测试真正打开系统终端。

### 任务 3：接入会话详情播放按钮

**文件：**
- 修改：`src/screens/SessionsScreen.tsx`
- 修改：`src/lib/i18n.tsx`

**接口：**
- 播放按钮调用 `openSessionTerminal({ cwd: selectedSession.projectDir, command: selectedSession.resumeCommand })`。

- [x] 在更新时间右侧、三点菜单左侧放置 Lucide `Play` 按钮。
- [x] 没有项目目录、恢复命令、非桌面运行时或正在启动时禁用按钮，并提供无障碍名称和标题。
- [x] 启动期间显示禁用状态；命令失败时在详情头部显示错误；成功后关闭旧错误。
- [x] 保持移动端返回、复制菜单和消息区域布局不变。
- [x] 添加中英文的打开终端、不可用、启动中和启动失败文案。

### 任务 4：补充前端行为测试并验证

**文件：**
- 修改：`tests/SessionsScreen.test.tsx`

- [x] mock `openSessionTerminal`，断言完整会话点击播放时传入准确的 `cwd` 与 `resumeCommand`。
- [x] 断言缺少恢复信息时播放按钮禁用，并断言 Web 运行时按钮不可用或不会执行命令。
- [x] 运行 `pnpm exec vitest run tests/SessionsScreen.test.tsx tests/transport/command-contract.test.ts`。
- [x] 运行 `pnpm typecheck`、`cargo fmt -- --check`、`pnpm rust:check` 及相关 `cargo test`。
