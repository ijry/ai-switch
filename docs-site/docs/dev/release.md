---
title: 发布流程
description: AI Switch 的自动化发布流程：版本 tag 触发 GitHub Actions，经过版本一致性校验、三平台构建矩阵、Tauri 更新器签名与 latest.json 清单生成，最终发布到 GitHub Releases。
---

# 发布流程

AI Switch 的发布完全由 GitHub Actions 驱动。推送一个符合规范的版本 tag，`.github/workflows/release.yml` 会自动完成校验、三平台构建、签名与发布。

## 触发条件

工作流只监听 tag 推送，且 tag 必须匹配 `v*.*.*`：

```yaml
on:
  push:
    tags:
      - "v*.*.*"
```

具体的格式校验比这个通配符更严格，实际正则要求 `v<major>.<minor>.<patch>`，可选带 `-rc` / `-beta` / `-alpha` 预发布后缀（后缀可再带 `.<数字>`）。合法示例：

```text
v0.6.7
v0.6.7-rc.1
v0.7.0-beta
v1.0.0-alpha.2
```

并发组按 `github.ref` 划分且 `cancel-in-progress: false`——同一个 tag 的发布不会被后续推送打断。

## 三个作业

| 作业 | 运行环境 | 职责 |
| --- | --- | --- |
| `prepare` | `ubuntu-latest` | 校验 tag 与版本一致性、校验 tag 归属分支、创建草稿 Release 并生成发布说明 |
| `build` | 矩阵：`windows-latest` / `macos-latest` / `ubuntu-latest` | 跑全量检查、构建 sidecar、打包 Tauri 安装包与独立服务器、上传产物 |
| `publish` | `ubuntu-latest` | 汇总产物、生成并校验 `latest.json`、把草稿 Release 转为正式发布 |

依赖关系是 `prepare` → `build`（矩阵并行）→ `publish`。`build` 设置了 `fail-fast: false`，某个平台失败时其余平台仍会跑完，方便一次看清所有问题。

## prepare：发布前的两道闸门

### 1. 版本必须三处完全一致

tag 去掉 `v` 前缀后，必须与以下两个文件里的版本**完全相同，包括预发布后缀**：

- `package.json` 的 `version`
- `src-tauri/tauri.conf.json` 的 `version`

校验逻辑分两步：先确认 `package.json` 与 `tauri.conf.json` 互相一致，再确认 tag 与它们一致。任何一步不匹配就直接 `exit 1`。

也就是说，`v0.6.7` 这个 tag 要求两个文件里都写着 `0.6.7`；`v0.7.0-rc.1` 要求两个文件里都写着 `0.7.0-rc.1`。

::: warning 常见踩坑
只改了 `package.json` 忘了改 `tauri.conf.json`（或反过来），是最容易触发这道闸门的原因。打 tag 前先确认两处都改了。另外 `src-tauri/Cargo.toml` 与 `Cargo.lock` 中本项目自身的版本也应同步更新——虽然 CI 不校验它，但版本不一致会让构建产物的元数据错乱。
:::

### 2. tag 必须基于默认分支

```bash
git fetch origin "$DEFAULT_BRANCH"
tag_commit="$(git rev-list -n 1 "$GITHUB_REF_NAME")"
if ! git merge-base --is-ancestor "$tag_commit" "origin/$DEFAULT_BRANCH"; then
  echo "Tag ${GITHUB_REF_NAME} is not based on ${DEFAULT_BRANCH}."
  exit 1
fi
```

tag 指向的提交必须是默认分支的祖先。在功能分支上打 tag 并推送会被拒绝——这道检查防止未合并的代码被误发布。

### 3. 创建草稿 Release

通过上述校验后，用 `ncipollo/release-action` 创建**草稿**（`draft: true`）Release 并自动生成发布说明（`generateReleaseNotes: true`）。是否标记为预发布由 tag 名判定：

```yaml
prerelease: ${{ contains(github.ref_name, '-rc') || contains(github.ref_name, '-beta') || contains(github.ref_name, '-alpha') }}
```

草稿状态很关键：构建期间用户不会看到一个残缺的 Release，只有 `publish` 作业成功后才会翻正。

## build：三平台矩阵

| 标签 | Runner | Bundle 格式 |
| --- | --- | --- |
| Windows | `windows-latest` | `nsis` |
| macOS | `macos-latest` | `app`、`dmg` |
| Linux | `ubuntu-latest` | `deb`、`appimage` |

每个平台的步骤顺序：

1. **校验签名密钥** —— 如果 `TAURI_SIGNING_PRIVATE_KEY` 为空直接抛错，绝不产出未签名的更新器资源。
2. **Linux 系统依赖** —— 仅 Linux 执行 `apt-get install`（`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf`、`libgtk-3-dev`）。
3. **工具链** —— pnpm 10.12.4、Node 22（带 pnpm 缓存）、stable Rust、stable Go（按 `sidecar/ai-switch-tsnet/go.sum` 缓存）。
4. **安装依赖** —— `pnpm install --frozen-lockfile`。
5. **计算平台变量** —— 从 `rustc -vV` 取 host 三元组，推导出更新器平台标识（`windows-x86_64` / `darwin-aarch64` / `linux-x86_64` 等）、sidecar 与服务器二进制路径。
6. **前端检查** —— `pnpm typecheck`、`pnpm test:run`、`pnpm release:manifest:test`。
7. **构建前端** —— `pnpm build`。
8. **sidecar 测试与构建** —— `go test ./...`，然后 `go build -trimpath -ldflags="-s -w"` 输出到 `src-tauri/binaries/ai-switch-tsnet-<三元组><后缀>`，并校验文件确实存在。
9. **Rust 检查** —— `pnpm rust:check`、`pnpm rust:test`。
10. **打包 Tauri** —— `pnpm tauri build --ci --bundles <该平台的格式>`。
11. **构建独立服务器** —— `pnpm server:build:release`。
12. **归集产物** —— 从 `src-tauri/target/release/bundle` 递归收集 `.exe`/`.msi`/`.dmg`/`.deb`/`.AppImage`/`.zip`/`.tar.gz`/`.sig`，统一重命名为 `ai-switch_<tag>_<平台>_<原名>`；另外把服务器与 sidecar 二进制分别压成 `ai-switch-server_<tag>_<平台>.zip` 与 `ai-switch-tsnet_<tag>_<平台>.zip`。这一步还会断言**至少存在一个 `.sig` 文件**，否则报错。
13. **上传产物** —— 以更新器平台标识为 artifact 名上传，`if-no-files-found: error`。

注意 CI 会在每个平台上重跑全部检查。这意味着一次发布相当于跑三遍完整测试套件，任何平台相关的问题都会在这里暴露。

## 更新器签名

Tauri 的更新器要求安装包附带 minisign 签名，签名密钥通过仓库 secret 注入：

| Secret | 是否必需 | 说明 |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | **必需** | minisign 私钥。缺失时 `build` 作业第一步就抛错 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 可选 | 私钥的口令，仅当私钥加了口令时才需要 |

对应的公钥硬编码在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`，更新端点指向：

```text
https://github.com/ijry/ai-switch/releases/latest/download/latest.json
```

`bundle.createUpdaterArtifacts: true` 让 Tauri 在打包时额外产出更新器资源与 `.sig` 签名文件。

## publish：清单生成与签名校验

1. **下载全部产物** —— `actions/download-artifact` 把三个平台的 artifact 取到 `release-assets/` 下，每个平台一个子目录。
2. **读取发布说明** —— 用 `gh api` 分页查询 Release 列表，取出 `prepare` 阶段生成的 body 写入 `release-notes.md`。取不到就报错退出。
3. **生成更新器清单** —— 运行 `scripts/create-updater-manifest.mjs`，按平台子目录名识别目标平台，从预设的偏好顺序里挑出对应的更新器资源（Windows 优先 `.exe` 再 `.msi`，macOS 优先 `.tar.gz` 再 `.dmg`，Linux 优先 `.AppImage` 再 `.deb`），产出 `release-assets/latest.json`：

   ```bash
   node scripts/create-updater-manifest.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-assets/latest.json \
     --notes-file release-notes.md
   ```

4. **校验签名密钥一致性** —— 运行 `scripts/verify-updater-signatures.mjs`，从每个 `.sig` 的 minisign 载荷里解出签名者 key ID，与 `tauri.conf.json` 里 `pubkey` 解出的 key ID 逐一比对：

   ```bash
   node scripts/verify-updater-signatures.mjs \
     --manifest release-assets/latest.json \
     --tauri-config src-tauri/tauri.conf.json
   ```

   这道校验的意义是：如果签名密钥被轮换而配置里的公钥没跟着更新，已安装的客户端会验签失败、更新链路静默断裂。在发布前拦下来远比事后补救便宜。

5. **转为正式发布** —— 再次调用 `ncipollo/release-action`，`draft: false`、`replacesArtifacts: true`、`artifactErrorsFailBuild: true`，上传 `release-assets/**/*.*` 下的全部文件（含 `latest.json`）。

## 发布操作示例

### 正式版

确认工作区干净、版本已同步、CI 在 `main` 上是绿的，然后：

```bash
git tag -a v0.6.8 -m "Release v0.6.8"
git push origin main
git push origin v0.6.8
```

### 预发布版

tag 名带 `-rc` / `-beta` / `-alpha` 即自动标记为 prerelease：

```bash
git tag -a v0.7.0-rc.1 -m "Release v0.7.0-rc.1"
git push origin v0.7.0-rc.1
```

记得 `package.json` 与 `src-tauri/tauri.conf.json` 里也要写完整的 `0.7.0-rc.1`。

### 打错了 tag 怎么办

如果 tag 还没推送，直接删掉重打：

```bash
git tag -d v0.6.8
```

如果已经推送且工作流已失败，先删远端 tag 再重来。注意 GitHub 上可能已经留下一个草稿 Release，需要手动删除，否则 `allowUpdates: true` 会让下次运行复用它：

```bash
git push origin :refs/tags/v0.6.8
git tag -d v0.6.8
```

推荐做法是**在打 tag 前先本地跑一遍完整检查**，见[本地开发](/dev/local-setup)的"一次跑完所有检查"。CI 跑三个平台一次发布耗时不短，把可以本地发现的问题留到 CI 里发现很浪费时间。

## 发布产物一览

一次成功的发布会在 GitHub Release 上产出：

- **Windows**：NSIS 安装器（`.exe`）及其 `.sig`
- **macOS**：`.app` 归档与 `.dmg`，及其 `.sig`
- **Linux**：`.deb` 与 `.AppImage`，及其 `.sig`
- **每个平台**：`ai-switch-server_<tag>_<平台>.zip`（独立服务器）
- **每个平台**：`ai-switch-tsnet_<tag>_<平台>.zip`（Tailscale sidecar）
- **`latest.json`**：Tauri 更新器清单，桌面端自动更新的数据源

用户如何拿到这些产物见[安装](/guide/installation)，独立服务器的用法见[独立服务器](/deploy/standalone-server)。

## 相关阅读

- [本地开发](/dev/local-setup)——发布前应当在本地跑通的检查
- [架构总览](/dev/architecture)——被打包的各部分分别是什么
- [安装](/guide/installation)
- [常见问题](/faq)
