# Linux 一键安装发现与默认值实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让官网、GitHub Release 正文和 Linux 一键安装体验完整呈现安装命令、端口选择、明文 HTTP 默认值与启动结果。

**Architecture:** 保持独立服务器全局安全默认值不变，只由 `scripts/install-server.sh` 为新安装写入 `AI_SWITCH_ALLOW_INSECURE_HTTP=1`。安装器继续用 `AI_SWITCH_PORT` 指定端口，安装后启用并启动 systemd 服务，同时输出访问地址、令牌读取方式和服务状态。发布正文生成器和双语文档补充一键命令。

**Tech Stack:** Bash、systemd、Node.js 内置测试、VitePress Markdown。

**Spec:** 用户请求：官网和 Release 显示 Linux 一键安装命令；一键安装默认开启 `AI_SWITCH_ALLOW_INSECURE_HTTP`；支持安装时指定默认端口；安装后自动启动并输出相关信息；在 WSL 中验证。

## Global Constraints

- 只修改任务分支，不合并、不 rebase、不推送。
- AI 验证使用 `src-tauri/target-codex/`，禁止创建其他 target 目录。
- 文档默认中文。
- 服务端源码的非环回明文 HTTP 默认拒绝逻辑不变。

---

### Task 1: 安装器行为

**Files:**
- Modify: `scripts/install-server.sh`
- Test: `scripts/install-server.test.mjs`

**Interfaces:**
- Consumes: `AI_SWITCH_PORT`、`AI_SWITCH_HOST`、`AI_SWITCH_ALLOW_INSECURE_HTTP` 环境变量。
- Produces: 新安装的 `/etc/ai-switch/server.env` 包含 `AI_SWITCH_ALLOW_INSECURE_HTTP=1`；端口输入无效时报错；安装后输出访问地址、令牌读取方式和服务状态。

- [x] 先在 `scripts/install-server.test.mjs` 增加失败断言。
- [x] 运行 `node --test scripts/install-server.test.mjs` 确认失败。
- [x] 修改安装器，保持重复安装保留既有环境文件。
- [x] 运行同一测试确认通过。

### Task 2: 发布正文

**Files:**
- Modify: `scripts/create-release-body.mjs`
- Test: `scripts/create-release-body.test.mjs`

**Interfaces:**
- Consumes: Release 资产目录、tag、repo。
- Produces: 存在 Linux 服务器归档时，正文包含带端口示例的 Linux 一键安装命令。

- [x] 先在发布正文测试中增加失败断言。
- [x] 运行 `node --test scripts/create-release-body.test.mjs` 确认失败。
- [x] 修改生成器。
- [x] 运行同一测试确认通过。

### Task 3: 官网文档

**Files:**
- Modify: `docs-site/docs/index.md`
- Modify: `docs-site/docs/en/index.md`
- Modify: `docs-site/docs/guide/installation.md`
- Modify: `docs-site/docs/en/guide/installation.md`
- Modify: `docs-site/docs/deploy/standalone-server.md`
- Modify: `docs-site/docs/en/deploy/standalone-server.md`

**Interfaces:**
- Produces: 首页与安装页展示 Linux 一键安装命令；部署文档解释端口、明文 HTTP 与安全边界。

- [x] 补充中英文首页、安装页和独立服务器文档。
- [x] 构建 docs 站点验证 Markdown 和链接。

### Task 4: WSL 验证与整体检查

**Files:**
- 无新增生产文件。

**Interfaces:**
- Produces: WSL 中安装命令参数解析与安装流程验证结果。

- [x] 在 WSL 中用本地安装脚本和最新 Release 归档执行安装。
- [x] 验证端口配置、服务启动状态、访问地址和令牌文件。
- [x] 运行 `pnpm release:manifest:test` 和 docs 构建。
- [x] 提交任务分支。
