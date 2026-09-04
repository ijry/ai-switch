---
title: 发布流程
description: AI Switch 的自动化发布流程：版本 tag 触发 GitHub Actions，经过版本一致性校验、三平台构建矩阵、Tauri 更新器签名与 latest.json 清单生成，最终发布到 GitHub Releases，再由独立工作流推送到 Homebrew 与 WinGet。
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
5. **计算平台变量** —— 从 `rustc -vV` 取 host 三元组，推导出更新器平台标识（`windows-x86_64` / `darwin-aarch64` / `linux-x86_64` 等）、`APP_VERSION`，以及 sidecar 与服务器二进制路径。
6. **前端检查** —— `pnpm typecheck`、`pnpm test:run`、`pnpm release:manifest:test`。
7. **构建前端** —— `pnpm build`。
8. **sidecar 测试与构建** —— `go test ./...`，然后 `go build -trimpath -ldflags="-s -w"` 输出到 `src-tauri/binaries/ai-switch-tsnet-<三元组><后缀>`，并校验文件确实存在。
9. **Rust 检查** —— `pnpm rust:check`、`pnpm rust:test`。
10. **打包 Tauri** —— `pnpm tauri build --ci --bundles <该平台的格式>`。
11. **构建独立服务器** —— `pnpm server:build:release`。
12. **归集产物** —— 运行 `scripts/stage-release-assets.mjs`，把 `src-tauri/target/release/bundle` 下的安装包重命名为 `ai-switch-<版本>-<平台>`（`.exe` 统一带 `-setup` 后缀），只有更新器会下载的 `.app.tar.gz` / `.nsis.zip` 命名为 `ai-switch-updater-<版本>-<平台>`，`.sig` 跟随它签名的那个文件。deb 的中间产物（`control.tar.gz`、`data.tar.gz`、`debian-binary`）与 `.app` 目录内部一律跳过。没有安装包或没有 `.sig` 就直接报错。另外把服务器与 sidecar 二进制分别压成 `ai-switch-server_<tag>_<平台>.zip` 与 `ai-switch-tsnet_<tag>_<平台>.zip`。

    命名不是随手取的：GitHub Release 页面按文件名排序，只展开前几个，其余折进「Show all N assets」。版本号紧跟 `ai-switch-` 能让安装包排在 `ai-switch-server_`、`ai-switch-tsnet_` 之前，用户要的 Windows `.exe` 和 macOS `.dmg` 才不会被折起来。`<平台>` 沿用更新器平台标识而不是更友好的 `windows-x64`，因为包管理器上架靠这个 token 挑安装包（见下文）。
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
2. **还原发布说明** —— 把 `prepare` 作业的 `release_notes` 输出写回 `release-notes.md`。为空就报错退出。
3. **生成更新器清单** —— 运行 `scripts/create-updater-manifest.mjs`，按平台子目录名识别目标平台，从预设的偏好顺序里挑出对应的更新器资源（Windows 优先 `.exe` 再 `.msi`，macOS 优先 `.tar.gz` 再 `.dmg`，Linux 优先 `.AppImage` 再 `.deb`），产出 `release-assets/latest.json`：

   ```bash
   node scripts/create-updater-manifest.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-assets/latest.json \
     --notes-file release-notes.md
   ```

4. **校验签名密钥一致性** —— 运行 `scripts/verify-updater-signatures.mjs`，从清单里每个平台的签名载荷解出签名者 key ID，与 `tauri.conf.json` 里 `pubkey` 解出的 key ID 逐一比对：

   ```bash
   node scripts/verify-updater-signatures.mjs \
     --manifest release-assets/latest.json \
     --tauri-config src-tauri/tauri.conf.json
   ```

   这道校验的意义是：如果签名密钥被轮换而配置里的公钥没跟着更新，已安装的客户端会验签失败、更新链路静默断裂。在发布前拦下来远比事后补救便宜。

5. **删除 `.sig`** —— 签名此时已经内联进 `latest.json`，客户端只读清单、不会去取同名 `.sig`，所以 `find release-assets -name '*.sig' -delete`，不再占用 Release 页面本来就不多的可见资产位。
6. **生成 Release 正文** —— 运行 `scripts/create-release-body.mjs`，扫描各平台子目录里的安装包，在 tag 提交信息之前拼一张中英双语下载表（Windows / macOS / Linux 各一行，独立服务器另起一行），写入 `release-body.md`：

   ```bash
   node scripts/create-release-body.mjs \
     --assets-dir release-assets \
     --tag "<tag>" \
     --repo "<owner/repo>" \
     --output release-body.md \
     --notes-file release-notes.md
   ```

   下载表只进 GitHub Release 正文。更新清单的 `notes` 仍然是原样的 tag 提交信息——桌面端按 29 个连字符的分隔线切分中英文，多一张表格会串进更新日志里。

7. **转为正式发布** —— 再次调用 `ncipollo/release-action`，`draft: false`、`bodyFile: release-body.md`、`replacesArtifacts: true`、`artifactErrorsFailBuild: true`，上传 `release-assets/**/*.*` 下的全部文件（含 `latest.json`）。

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

一次成功的发布会在 GitHub Release 上产出（按资产列表里的先后顺序）：

- **Windows**：`ai-switch-<版本>-windows-x86_64-setup.exe`
- **macOS**：`ai-switch-<版本>-darwin-aarch64.dmg`
- **Linux**：`ai-switch-<版本>-linux-x86_64.AppImage` 与 `ai-switch-<版本>-linux-x86_64.deb`
- **每个平台**：`ai-switch-server_<tag>_<平台>.zip`（独立服务器）
- **每个平台**：`ai-switch-tsnet_<tag>_<平台>.zip`（Tailscale sidecar）
- **macOS**：`ai-switch-updater-<版本>-darwin-aarch64.app.tar.gz`（只有自动更新会下载）
- **`latest.json`**：Tauri 更新器清单，桌面端自动更新的数据源

`.sig` 不作为独立资产发布，签名内联在 `latest.json` 里。Release 正文顶部另有一张下载表，直接指向上面前三类文件。

用户如何拿到这些产物见[安装](/guide/installation)，独立服务器的用法见[独立服务器](/deploy/standalone-server)。

## 发布到包管理器（Homebrew / WinGet）

`.github/workflows/package-managers.yml` 把**已经发布**的 Release 推给包管理器。它和 `release.yml` 故意分成两个工作流：winget 的提交要过 Microsoft 审核，Homebrew 推送可能因为 token 过期失败，这些都不应该把发版本身拖住或标成失败；反过来，补发一个几个月前的 tag 也不需要重新构建。

### 触发方式

| 触发 | 行为 |
| --- | --- |
| `release.yml` 发布完成 | 自动运行：publish 作业最后一步按刚发布的 tag 派发本工作流 |
| 有人手工 publish 一个 Release | 自动运行，tag 取自事件 |
| 手动 `workflow_dispatch` | 用 `tag` 输入补发任意历史版本；留空则取最新一个 Release |

`release: published` 事件对流水线发布的 Release **不会**触发。GitHub 不会用自己的 `GITHUB_TOKEN` 产生的事件去启动新的工作流运行，而这个 Release 正是 release-action 用那个 token 发布的——v0.8.1 因此完全没跑过包管理器。`workflow_dispatch` 是这条规则的两个例外之一（另一个是 `repository_dispatch`），所以 `release.yml` 在发布之后显式派发一次，并且用 tag 而不是分支作为 ref，让清单由产出这批安装包的那个提交来渲染。派发本身失败只留一条 warning：已经发布的 Release 不该因为包管理器被标成失败，缺的那一次可以事后用同一个 tag 手动补跑。

手动运行另外有三个开关：`homebrew` 和 `winget` 可以分别关掉，`dry_run` 会渲染并校验清单、但不碰任何外部仓库。

草稿和预发布（`-rc` / `-beta` / `-alpha`）一律跳过，并在日志里写明原因而不是报错：草稿的资产还没有公开下载地址，而预发布一旦进了 tap 就会推给所有执行 `brew upgrade` 的人。

### 需要配置什么

两条链路都要往**别人的仓库**里写东西，所以都需要一个仓库 secret。**缺 secret 不会让工作流失败**，只会在日志里留一条 warning 并跳过对应的那一条链路，另一条照常走。

| Secret / Variable | 类型 | 用途 |
| --- | --- | --- |
| `HOMEBREW_TAP_TOKEN` | secret | 对 tap 仓库有 `contents: write` 的 PAT |
| `HOMEBREW_TAP_REPO` | variable，可选 | tap 仓库名，默认 `ijry/homebrew-ai-switch` |
| `WINGET_TOKEN` | secret | **classic** PAT，只需 `public_repo` scope |
| `WINGET_FORK_USER` | variable，可选 | winget-pkgs fork 所在账号，默认仓库 owner |

::: warning WinGet 的 token 必须是 classic PAT
提交 PR 的实际工具是 Komac，它走 GitHub 的 GraphQL API，而 fine-grained token 只能对「资源 owner 与 token owner 一致」的资源调用 GraphQL。目标是 `microsoft/winget-pkgs`，所以 fine-grained token 和 GitHub App 在这里都用不了。
:::

### Homebrew：一次性准备

1. 建一个**公开**仓库 `ijry/homebrew-ai-switch`（名字必须以 `homebrew-` 开头，`brew tap ijry/ai-switch` 才能解析到它）。
2. 生成一个对该仓库有 `contents: write` 的 PAT，存成 `HOMEBREW_TAP_TOKEN`。

之后每次发布，工作流在 `macos-latest` 上渲染 cask、放进一个本地 tap、**真的 `brew install --cask` 装一遍**，确认 `/Applications/AI Switch.app` 存在且隔离属性已被清掉，然后才推到 tap 仓库。对一个 ad-hoc 签名的包来说这道安装验证是必要的：cask 语法正确但装完打不开，是这类应用最容易出的问题。

::: tip cask 帮用户跳过了 Gatekeeper 那一套
macOS 包是 ad-hoc 签名、没有公证的（见[安装](/guide/installation)里那一节），所以 cask 里带了一个 `postflight`，在 Homebrew 刚放好的那份 app 上执行 `xattr -dr com.apple.quarantine`。它只作用于那一个路径，不改任何系统设置，但用户不用再手动走「系统设置 → 仍要打开」。

cask 还声明了 `depends_on arch: :arm64`（CI 只构建 Apple Silicon）和 `auto_updates true`（应用自带更新器会自己换掉 app，Homebrew 记录的版本本来就会过期）。
:::

### WinGet：第一个版本必须手工提交

自动化只能做**版本递增**，第一次上架不行。`winget-releaser` 的第一步就是检查 `microsoft/winget-pkgs` 里有没有这个包，没有就直接报错：

```text
::error::Package ijry.AISwitch does not exist in the winget-pkgs repository.
Please add atleast one version of the package before using this action.
```

所以一次性准备是三步：

1. 把 `microsoft/winget-pkgs` fork 到 `ijry` 名下（工具不会替你建 fork）。
2. 用 [Komac](https://github.com/russellbanks/Komac) 或 [wingetcreate](https://github.com/microsoft/winget-create) 手工提交 `ijry.AISwitch` 的第一个版本，等 winget 的维护者合并。包标识符大小写敏感，且必须和目录路径完全一致（`manifests/i/ijry/AISwitch/<版本>/`）。
3. 生成 classic PAT（`public_repo`），存成 `WINGET_TOKEN`。

之后每次发布，工作流会开一个 PR 到 `microsoft/winget-pkgs`。**PR 需要 Microsoft 的维护者合并，版本才会真正对用户可见**，这一步不在我们控制范围内。

::: tip 安装包本身不需要代码签名
winget-pkgs 的策略文档里没有把代码签名列为要求，`SignatureSha256` 只对 MSIX/APPX 有意义。它的校验流水线关心的是别的东西：多引擎杀毒扫描、以非管理员身份静默安装、以及安装后的注册表项要和清单里的 `Publisher` / `PackageName` 对得上。

Tauri 的 NSIS 安装包默认是 `currentUser` 模式，不需要提权；`InstallerType: nullsoft` 也不用自己写静默参数——winget 客户端认出 nullsoft 后会自动用 `/S`。
:::

### 用户侧的安装命令

上面两处准备做完、并且各自的第一个版本已经落地之后，用户就可以这样装：

```bash
# macOS（Apple Silicon）
brew tap ijry/ai-switch
brew install --cask ai-switch
```

```powershell
# Windows
winget install ijry.AISwitch
```

在那之前这两条命令都会找不到包，所以[安装](/guide/installation)那一页仍然只写从 Releases 下载。

### 清单是怎么生成的

`scripts/create-package-manifests.mjs` 从 Release 的 API 响应出发，做三件事：

1. **挑安装包**。按更新器平台标识（`darwin-aarch64`、`windows-x86_64`）加扩展名匹配，排除 `.sig`、`.app.tar.gz`、`.nsis.zip` 这些只有更新器会下载的资源。仓库先后用过两套资产命名，两套都能匹配；匹配到 0 个或多于 1 个都直接报错，不猜。
2. **取 sha256**。Release API 现在对每个资产直接给出 `digest`，所以正常情况下不用把 31 MB 的 dmg 拉下来算哈希；老版本的资产没有这个字段，会退回下载并流式计算。
3. **渲染 cask，并把 winget 需要的输入写进 `summary.json`**。给 `winget-releaser` 的 `installers-regex` 是用第 1 步已经确定的文件名生成的锚定正则，这样它自己再匹配一次的结果不可能和第 1 步不一致。

这个脚本由 `pnpm release:manifest:test` 覆盖（`release.yml` 的每个平台都会跑），用例里钉着 v0.8.0 真实的资产列表。

## 相关阅读

- [本地开发](/dev/local-setup)——发布前应当在本地跑通的检查
- [架构总览](/dev/architecture)——被打包的各部分分别是什么
- [安装](/guide/installation)
- [常见问题](/faq)
