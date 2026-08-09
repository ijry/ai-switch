# GitHub Actions 跨平台发布设计

## 目标

通过推送版本标签，自动构建并发布 `main` 上的 AI Switch 跨平台二进制文件。

工作流会创建一个 GitHub Release，并上传 Windows、macOS 和 Linux 的桌面安装包、更新资源、独立服务器二进制文件和 Tailscale sidecar 二进制文件。

## 触发方式

发布工作流监听符合 `v*.*.*` 的标签推送：

```yaml
on:
  push:
    tags:
      - "v*.*.*"
```

正式版本示例：

```bash
git tag v0.4.2
git push origin v0.4.2
```

包含 `-rc`、`-beta` 或 `-alpha` 的标签会自动标记为预发布版本，例如 `v0.4.2-rc.1`。标签去掉 `v` 前缀后的完整版本必须与 `package.json` 和 `src-tauri/tauri.conf.json` 中的版本完全一致，包括 prerelease 后缀；标签对应的提交必须属于仓库默认分支。

标签推送后，工作流先创建草稿 Release，再构建和上传全部资源；所有构建成功后自动将草稿 Release 发布。

## 发布任务

工作流包含发布准备、平台构建和最终发布三个阶段：

- `prepare`：校验标签格式、版本和默认分支归属，创建草稿 Release 并生成发布说明。
- `build`：依赖 `prepare`，使用平台矩阵构建并上传工作流资源。
- `publish`：依赖 `build`，生成 `latest.json`、校验签名并将草稿 Release 正式发布。

平台构建使用以下矩阵：

- `windows-latest`
- `macos-latest`
- `ubuntu-latest`

每个平台 job 安装 Node、pnpm、Rust、Go 和平台专用的 Tauri 依赖，然后在打包前运行以下检查：

- `pnpm typecheck`
- `pnpm test:run`
- `pnpm rust:check`
- `pnpm rust:test`
- `sidecar/ai-switch-tsnet` 中的 `go test ./...`

检查通过后，每个平台 job 构建：

- 当前平台的 Tauri desktop bundle。
- 当前平台的 `ai-switch-server` release 二进制文件。
- 当前平台的 `ai-switch-tsnet` sidecar 二进制文件。

sidecar 二进制文件会在执行 `pnpm tauri:build` 前写入 `src-tauri/tauri.conf.json` 期望的 Tauri `externalBin` 路径。

## 桌面 Bundle 目标

`src-tauri/tauri.conf.json` 当前仅配置 Windows `nsis`。发布实现应允许跨平台 bundle：

- Windows: `nsis`
- macOS: `dmg`
- Linux：`deb` 和 `appimage`

更新接口保持不变：

```text
https://github.com/ijry/ai-switch/releases/latest/download/latest.json
```

## 签名

工作流需要以下 Tauri updater 签名密钥：

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 可选，取决于私钥是否设置密码

如果缺少私钥，工作流必须在发布资源前失败；发布未签名的 updater 资源会导致应用内更新不可用。

## Release 创建与上传

工作流先为推送的标签创建草稿 Release，平台构建 job 将资源上传为工作流 artifact；所有平台构建成功后，`publish` job 生成 `latest.json` 并将全部资源上传到 Release，最后取消草稿状态。

资源名称包含应用版本、操作系统和架构，避免资源重名。工作流应上传：

- Tauri 生成的平台桌面安装包和 updater 资源。
- `latest.json` updater 清单。
- 每个平台的独立 `ai-switch-server` 压缩包。
- 每个平台的独立 `ai-switch-tsnet` 压缩包。

## 失败行为

工作流会因签名密钥缺失、测试失败或预期资源缺失而失败；只有所有必需 job 成功后才会创建公开 Release。

如果同一标签已有草稿 Release，工作流允许更新并复用该 Release，不会强制改写 Git 历史。

## 不在范围内

- 商店专用签名、公证或应用商店分发。
- 自动修改版本号或创建 Git 标签。
- 修改 updater URL 或增加发布渠道。
- 删除现有本地构建命令。

## 验收标准

- 推送符合格式的版本标签后，工作流自动启动。
- 工作流构建 Windows、macOS 和 Linux 桌面安装包。
- 工作流上传桌面、独立服务器和 sidecar 资源。
- 未配置 updater 签名时工作流失败。
- 现有本地开发命令继续可用。
