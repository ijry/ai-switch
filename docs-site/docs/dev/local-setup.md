---
title: 本地开发
description: 在本地搭建 AI Switch 开发环境：工具链版本要求、依赖安装、前端与 Rust 检查命令、sidecar 测试、桌面应用开发模式与构建，以及这个文档站自身的开发方式。
---

# 本地开发

本页给出从零开始在本地跑起 AI Switch 的完整命令。所有命令都在仓库根目录执行，除非特别说明工作目录。

## 工具链要求

版本以 `.github/workflows/release.yml` 中 CI 实际使用的为准——本地与 CI 保持一致能避免"我这里能过"的问题。

| 工具 | 版本 | 说明 |
| --- | --- | --- |
| Node.js | 22 | CI 用 `actions/setup-node` 固定为 22 |
| pnpm | 10.12.4 | 与根 `package.json` 的 `packageManager` 字段一致，CI 用 `pnpm/action-setup` 固定 |
| Rust | stable | CI 用 `dtolnay/rust-toolchain@stable` |
| Go | stable | 仅构建/测试 sidecar 时需要 |

Windows 与 macOS 上 Tauri 2 的系统依赖由各自的开发者工具链提供（Windows 需要 MSVC 生成工具与 WebView2，macOS 需要 Xcode 命令行工具）。

Linux 需要额外安装系统包，与 CI 中的 `apt-get` 步骤一致：

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  libgtk-3-dev
```

## 安装依赖

pnpm 通过 corepack 分发，先启用它再安装：

```powershell
corepack enable
pnpm install
```

Rust 依赖不需要单独安装，首次执行 `cargo` 相关命令时会自动拉取。

## 前端检查

```powershell
pnpm typecheck
pnpm test:run
```

- `pnpm typecheck` 是 `tsc --noEmit`，只做类型检查不产出文件。
- `pnpm test:run` 是 `vitest run`，单次执行不进入 watch。开发时想要 watch 模式用 `pnpm test`。

## Rust 检查

```powershell
pnpm rust:check
pnpm rust:test
pnpm server:check
```

这三条都是在 `src-tauri` 目录下调用 cargo 的薄封装：

- `pnpm rust:check` → `cargo check`
- `pnpm rust:test` → `cargo test`
- `pnpm server:check` → `cargo check --bin ai-switch-server`，单独校验独立服务器目标能编译

::: tip
`cargo check` 在 Windows 上偶发在 `tauri-build` 阶段报 `PermissionDenied` 而 panic，这通常是构建产物目录的临时文件锁，直接重跑即可。
:::

## sidecar 测试

Tailscale sidecar 是独立的 Go 模块，测试需要切换工作目录：

```powershell
cd sidecar/ai-switch-tsnet
go test ./...
```

如果要单独构建 sidecar 产物（`pnpm tauri:build` 打包桌面应用时需要它就位）：

```powershell
cd sidecar/ai-switch-tsnet
go build -o ../../src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe .
```

文件名后缀必须是当前 Rust 目标三元组，Tauri 的 `externalBin` 机制按三元组查找二进制。用 `rustc -vV` 可以看到本机的 `host:` 三元组。

## 更新器清单测试

发布流程会生成并校验 `latest.json`，对应脚本自带 Node 原生测试：

```powershell
pnpm release:manifest:test
```

它跑的是 `scripts/create-updater-manifest.test.mjs` 与 `scripts/verify-updater-signatures.test.mjs`。CI 在每个平台的构建作业里都会执行这一步，所以改动 `scripts/` 下的发布脚本后本地务必先跑一次。

## 运行桌面应用

```powershell
pnpm tauri:dev
```

它会先按 `tauri.conf.json` 的 `beforeDevCommand` 启动 Vite（`http://127.0.0.1:1420`），再编译并启动 Rust 侧。首次编译耗时较长，之后增量编译会快很多。

::: tip 开发数据与正式数据是分开的
debug 构建使用 `~/.ai-switch/ai-switch-dev.db`，release 构建使用 `~/.ai-switch/ai-switch.db`。两者共用同一个数据目录但数据库文件独立，所以 `pnpm tauri:dev` 不会动到你已安装版本里的账号数据。
:::

如果只想在浏览器里调前端，可以单独跑 `pnpm dev`（Vite dev server 监听 `127.0.0.1`），此时前端会走 Web transport，需要有一个已启动的 Web 服务或独立服务器提供 `/api`。

## 构建

### 桌面安装包

```powershell
pnpm build
pnpm tauri:build
```

- `pnpm build` 是 `tsc && vite build`，产出 `dist/`。
- `pnpm tauri:build` 打包安装包，产物在 `src-tauri/target/release/bundle/` 下。

打包前请确认 `src-tauri/binaries/` 里有对应三元组的 sidecar 二进制，否则 Tauri 会因找不到 `externalBin` 而失败。

### 独立服务器

```powershell
pnpm server:build          # debug，产物 src-tauri/target/debug/ai-switch-server
pnpm server:build:release   # release，产物 src-tauri/target/release/ai-switch-server
```

运行时通过环境变量配置（Windows PowerShell 示例）：

```powershell
$env:AI_SWITCH_HOST = "127.0.0.1"
$env:AI_SWITCH_PORT = "3090"
$env:AI_SWITCH_TOKEN = "replace-me"
$env:AI_SWITCH_STATIC_DIR = "$PWD\dist"
.\src-tauri\target\debug\ai-switch-server.exe
```

完整的环境变量清单与部署建议见[独立服务器](/deploy/standalone-server)。

## 一次跑完所有检查

发布前或提 PR 前建议按 CI 的顺序全部执行一遍：

```powershell
pnpm typecheck
pnpm test:run
pnpm release:manifest:test
pnpm rust:check
pnpm rust:test
cd sidecar/ai-switch-tsnet; go test ./...
```

这正是 `.github/workflows/release.yml` 在每个平台构建作业里跑的检查集合。任一项失败时不应打 tag，详见[发布流程](/dev/release)。

## 开发这个文档站

文档站是 `docs-site/` 下的独立 pnpm 项目，有自己的 `package.json` 与 lockfile，不属于根工作区。

```powershell
cd docs-site
pnpm install
pnpm docs:dev
```

`pnpm docs:dev` 启动 VitePress 开发服务器，支持热更新，改完 markdown 立即能看到效果。

构建与本地预览：

```powershell
pnpm docs:build
pnpm docs:preview
```

::: warning 必须用 preview 而不是直接打开 dist 里的 HTML
站点配置了 `base: "/ai-switch/"`（部署在 GitHub Pages 的子路径下）。直接用浏览器打开 `docs/.vitepress/dist/index.html` 会因为所有资源路径都带 `/ai-switch/` 前缀而全部 404，页面看起来完全没有样式。`pnpm docs:preview` 会以正确的 base 起一个静态服务器（默认端口 4173），这才是验证构建产物的正确方式。
:::

### 写文档时的注意事项

- **站点配了 `cleanUrls: false`**（GitHub Pages 无法把 `/foo` 映射到 `foo.html`），但页面之间互链时**仍然不要写 `.html` 后缀**——VitePress 会在构建时自动补全。中文页写 `/guide/accounts`，英文页写 `/en/guide/accounts`。
- **链接到不存在的页面会导致构建失败**。VitePress 的死链检查是硬失败，`pnpm docs:build` 会直接报错并列出坏链接。
- **正文里不要出现裸的双大括号插值**。VitePress 用 Vue 编译 markdown，一对左大括号会被当成插值表达式解析并导致构建失败。需要展示 GitHub Actions 表达式之类的内容时，一定放进代码块：

  ```yaml
  prerelease: ${{ contains(github.ref_name, '-rc') }}
  ```

  围栏代码块是安全的，正文和行内代码不是。
- 中英文页面一一对应。侧边栏在 `docs/.vitepress/config.mts` 里按语言分别配置，英文侧边栏的 key 必须带 `/en/` 前缀。
- 每个页面都要有 `title` 与 `description` frontmatter，`transformPageData` 会用它们生成 canonical 链接与 Open Graph 标签。

### 文档站的 CI

`.github/workflows/docs.yml` 在 `docs-site/**` 变更时触发：PR 上只构建（作为死链与语法的校验），推到 `main` 后构建并部署到 GitHub Pages。它用 `fetch-depth: 0` 检出完整历史，因为 `lastUpdated` 需要读每个页面的 git 提交时间。

## 相关阅读

- [架构总览](/dev/architecture)——各目录与模块的职责划分
- [发布流程](/dev/release)——tag、签名与三平台产物
- [桌面端部署](/deploy/desktop)
- [常见问题](/faq)
