# AI Switch 应用自启动设计

## 背景与目标

AI Switch 当前可以在应用进程已经启动后自动恢复 Web 服务和本地路由代理，但没有“随操作系统登录启动应用”的能力。用户需要在应用设置中开启一个开关，让桌面端在 Windows、macOS 或 Linux 用户登录后自动启动，并在这种启动方式下保持主窗口隐藏、继续驻留托盘。

目标如下：

- 通过系统原生登录启动项注册 AI Switch。
- 在应用设置中读取真实的系统注册状态，并提供启用、禁用操作。
- 自启动触发时只隐藏主窗口，不影响托盘、后台服务恢复和单实例行为。
- 浏览器 Web 运行时不显示桌面自启动控制，也不调用桌面插件。
- 启用或禁用失败时保持界面状态准确，并给出可理解的错误提示。

## 范围与非目标

本次变更包含：

- 接入官方 Tauri 2 `tauri-plugin-autostart` Rust 插件和 JavaScript 客户端包。
- 为自启动注册项附加稳定参数 `--autostart`。
- 在设置页增加桌面专属复选框及中英文文案。
- 增加启动参数识别、前端交互和插件配置的测试。
- 更新桌面部署文档。

本次变更不包含：

- 不把自启动状态写入 `settings.json`、SQLite 或其他应用配置；系统注册项是唯一事实来源。
- 不增加 Web API 或 Tauri 自定义命令；自启动插件命令只在桌面端可用。
- 不改变普通手动启动时的窗口显示行为。
- 不增加“启动后最小化/启动后显示”等额外用户选项。
- 不支持 Android 或 iOS；当前桌面构建目标为 Windows、macOS 和 Linux。

## 方案选择

采用官方 `tauri-plugin-autostart`，而不是在 Rust 中分别实现注册表、LaunchAgent 和 `.desktop` 文件。插件已经封装三种桌面操作系统的注册方式，并提供统一的 `isEnabled`、`enable` 和 `disable` API。这样可以让系统注册状态直接作为来源，避免应用配置与系统状态发生漂移。

插件注册使用：

- 应用名：`AI Switch`。
- 自启动参数：`--autostart`。
- Tauri 能力权限：`autostart:default`，其中包含检查、启用和禁用权限。

## 组件与文件职责

### Rust 桌面外壳

在 `src-tauri/src/lib.rs` 注册 `tauri_plugin_autostart::Builder`。插件构建器传入 `--autostart` 参数和应用名。`setup` 回调在启动后台恢复任务前检查命令行参数；若存在该参数，获取 `main` Webview 窗口并调用 `hide`。隐藏失败只记录错误，不阻止应用继续启动。

自启动参数判断提取为纯函数，以便在不创建 Tauri 应用的情况下测试：含有精确参数 `--autostart` 时返回真，其他参数（包括相似前缀）返回假。

### 前端自启动适配

新增前端小模块（建议为 `src/lib/autostart.ts`），只负责重新导出或封装 `@tauri-apps/plugin-autostart` 的三个操作。该模块不保存状态，也不在模块加载时调用插件。

`SettingsScreen` 使用 `isDesktop()` 判断运行时。桌面端创建 React Query 查询 `['autostart']`，查询函数调用 `isEnabled()`；非桌面端通过 `enabled: false` 禁止查询并完全隐藏复选框。变更 mutation 根据目标值调用 `enable()` 或 `disable()`，成功后把目标布尔值写入查询缓存。

### 设置界面

在现有“应用设置”区域加入一个复选框，和语言、主题等应用级偏好并列。控件在初始状态查询中、启用/禁用 mutation 中均禁用。桌面插件错误显示专用的双语错误文案，不复用保存 `AppSettings` 的成功提示。Web 端不显示占位或不可用控件。

### Tauri 能力与依赖

- `src-tauri/Cargo.toml` 增加 `tauri-plugin-autostart = "2"`。
- `package.json` 增加 `@tauri-apps/plugin-autostart`，并更新 `pnpm-lock.yaml`。
- `src-tauri/capabilities/default.json` 增加 `autostart:default`。
- `src-tauri/src/lib.rs` 注册插件并保留现有 `generate_handler!` 命令列表不变。

## 运行时流程

### 开启自启动

1. 桌面设置页调用 `isEnabled()`，显示系统当前状态。
2. 用户勾选复选框。
3. 前端调用插件 `enable()`；插件按平台写入登录启动项，并记录启动命令及 `--autostart` 参数。
4. 调用成功后查询缓存变为 `true`，控件保持勾选。
5. 下一次用户登录时，系统启动 AI Switch；Tauri `setup` 识别 `--autostart`，隐藏 `main` 窗口，托盘仍可用，现有 Web 服务和路由代理恢复任务照常执行。

### 关闭自启动

1. 用户取消复选框。
2. 前端调用插件 `disable()`。
3. 调用成功后查询缓存变为 `false`，系统登录项被移除。

### 普通启动和单实例

手动从开始菜单、应用目录或命令行启动时不带 `--autostart`，主窗口照常显示。若自启动实例已经运行，现有单实例插件接收后续启动请求并聚焦主窗口；该行为不因新增开关改变。

## 错误处理

- `isEnabled()` 失败：复选框保持禁用，显示加载/读取错误，不猜测系统状态。
- `enable()` 或 `disable()` 失败：不更新查询缓存；React 控件回到 mutation 前的实际状态，并显示“无法更新自启动设置”的错误。
- `hide()` 失败：输出 Rust 错误日志，但继续初始化托盘和后台任务，避免自启动导致应用不可用。
- 插件只在桌面运行时调用。浏览器调用路径不应产生 `plugin:autostart|...` 请求。

## 测试策略

### 前端

在 `SettingsScreen` 测试中模拟插件 API 和桌面运行时，覆盖：

- 系统状态为关闭时正确渲染未勾选控件。
- 勾选调用 `enable()` 并更新为开启。
- 取消勾选调用 `disable()` 并更新为关闭。
- 插件拒绝时保留原状态并显示错误。
- Web 运行时不显示自启动控件、不调用插件。

### Rust

在 `src-tauri/src/lib.rs` 的测试模块中覆盖启动参数纯函数的正负样例。增加静态配置契约测试，检查插件注册、`--autostart` 参数和 `autostart:default` 权限存在。

### 回归检查

实现完成后运行：

- `pnpm typecheck`
- 相关 Vitest 测试以及 `pnpm test:run`
- `pnpm rust:check`
- `pnpm rust:test`

## 验收标准

- 在三种桌面平台的设置页开启后，系统登录会启动 AI Switch 且主窗口初始不可见、托盘可操作。
- 关闭开关并重新登录后，应用不会因该设置自动启动。
- 普通手动启动仍显示主窗口。
- Web 浏览器设置页不出现桌面自启动控件。
- 系统注册操作失败时用户能看到错误，且不会出现与真实状态相反的勾选结果。
- 新增测试和现有测试全部通过。
