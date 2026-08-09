# 标签自动发布实施计划

> **面向智能体执行者：** 实施时必须使用 `superpowers:executing-plans`，按任务逐项执行本计划。

**目标：** 将 AI Switch 发布流程改为推送符合版本格式的 Git 标签后自动构建并发布 GitHub Release。

**架构：** GitHub Actions 监听 `v*.*.*` 标签推送，使用 `github.ref_name` 作为唯一版本来源。工作流先创建草稿 Release，构建并上传所有平台资源和更新清单，成功后自动将草稿 Release 发布；带有 `-rc`、`-beta` 或 `-alpha` 后缀的标签自动标记为预发布版本。

**技术栈：** GitHub Actions、`ncipollo/release-action@v1`、Node.js、pnpm、Tauri 2、Bash、PowerShell。

## 全局约束

- 继续直接修改 `main`，不创建分支或 worktree。
- `docs/superpowers/specs` 和 `docs/superpowers/plans` 使用中文。
- 正式版本标签格式为 `v*.*.*`，例如 `v0.4.2`。
- 预发布标签格式可以包含 `-rc`、`-beta` 或 `-alpha`，例如 `v0.4.2-rc.1`。
- 发布工作流不再依赖 `workflow_dispatch` 输入参数。
- 标签去掉 `v` 前缀后的完整版本必须与 `package.json` 和 `src-tauri/tauri.conf.json` 完全一致，包括 prerelease 后缀。
- 标签对应的提交必须属于仓库默认分支。

---

### 任务 1：创建发布准备阶段

**文件：**
- 修改：`.github/workflows/release.yml:3-48`

**接口：**
- 消费：GitHub `push` 事件的 `github.ref_name`。
- 产出：`prepare` job 校验标签并创建草稿 Release，后续 job 使用同一个自动解析的版本标签。

- [x] **步骤 1：替换工作流触发器和输入定义**

将 `workflow_dispatch` 及其 `tag`、`release_name`、`draft`、`prerelease` 输入替换为：

```yaml
on:
  push:
    tags:
      - "v*.*.*"
```

- [x] **步骤 2：替换构建 job 中的版本引用**

将资源暂存步骤中的 `${{ inputs.tag }}` 全部替换为 `${{ github.ref_name }}`，确保压缩包名称、资源目录和 manifest 下载 URL 使用被推送的标签。

- [x] **步骤 3：增加版本一致性校验**

在 `prepare` job 中配置 Node，并用 Bash 拒绝不符合 `vX.Y.Z` 或 `vX.Y.Z-(rc|beta|alpha).N` 的标签；再读取 `package.json` 和 `src-tauri/tauri.conf.json` 的 `version`，要求两者与标签完整版本完全一致。

```bash
if [[ ! "$GITHUB_REF_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-(rc|beta|alpha)(\.[0-9]+)?)?$ ]]; then
  echo "Unsupported release tag format: ${GITHUB_REF_NAME}"
  exit 1
fi
package_version="$(node -p "require('./package.json').version")"
tauri_version="$(node -p "require('./src-tauri/tauri.conf.json').version")"
tag_version="${GITHUB_REF_NAME#v}"
if [[ "$package_version" != "$tauri_version" ]]; then
  echo "package.json version ${package_version} does not match Tauri version ${tauri_version}."
  exit 1
fi
if [[ "$tag_version" != "$tauri_version" ]]; then
  echo "Release tag ${GITHUB_REF_NAME} does not match app version v${tauri_version}."
  exit 1
fi
```

- [x] **步骤 4：校验标签提交属于默认分支**

Checkout 使用 `fetch-depth: 0`，通过 `git rev-list` 解析标签提交，再使用 `git merge-base --is-ancestor` 验证该提交属于仓库默认分支。

### 任务 2：串联构建与自动发布

**文件：**
- 修改：`.github/workflows/release.yml:build/publish jobs`

**接口：**
- 消费：`${{ github.ref_name }}`。
- 产出：`build` 依赖 `prepare`，`publish` 依赖 `prepare` 和 `build`，成功后自动发布 Release。

- [x] **步骤 1：定义预发布表达式**

将 `-rc`、`-beta`、`-alpha` 任一后缀作为预发布版本，使用 GitHub expression：

```yaml
${{ contains(github.ref_name, '-rc') || contains(github.ref_name, '-beta') || contains(github.ref_name, '-alpha') }}
```

- [x] **步骤 2：配置构建依赖**

将 `build.needs` 设置为 `prepare`，保证草稿 Release 创建成功后才开始平台构建。

- [x] **步骤 3：修改生成日志和 manifest 步骤**

读取 Release 正文时使用 `${{ github.ref_name }}`，生成 manifest 时的 `--tag` 也使用 `${{ github.ref_name }}`。

- [x] **步骤 4：修改最终发布步骤**

将 `publish.needs` 设置为 `prepare` 和 `build`。最终 Release 使用 `${{ github.ref_name }}`，`draft` 固定为 `false`，`prerelease` 使用同一表达式，确保所有构建成功后自动正式发布。

### 任务 3：同步发布文档

**文件：**
- 修改：`README.md:Release Automation`
- 修改：`docs/superpowers/specs/2026-07-17-github-actions-release-design.md:Goal/Trigger`
- 修改：`docs/superpowers/specs/2026-07-26-release-notes-automation-design.md:方案`

**接口：**
- 消费：新的标签触发规则和预发布规则。
- 产出：用户可直接照文档完成版本发布。

- [x] **步骤 1：更新 README 发布说明**

说明发布由推送 `v*.*.*` 标签触发，删除手动输入参数说明，并加入以下命令：

```bash
git tag v0.4.2
git push origin v0.4.2
```

同时说明 `v0.4.2-rc.1`、`v0.4.2-beta.1` 和 `v0.4.2-alpha.1` 会生成预发布版本。

- [x] **步骤 2：更新中文设计文档**

将 2026-07-17 设计文档中的手动触发描述改为标签触发，并将 2026-07-26 文档补充为“草稿 Release → 构建 → 清单 → 自动发布”的完整流程。

### 任务 4：验证自动发布配置

**文件：**
- 检查：`.github/workflows/release.yml`
- 检查：`docs/superpowers/plans/2026-08-09-tag-release-automation.md`

- [x] **步骤 1：检查 YAML 和残留输入引用**

确认工作流包含 `push.tags`，且不再包含 `inputs.tag`、`inputs.release_name`、`inputs.draft` 或 `inputs.prerelease`。

- [x] **步骤 2：运行发布脚本测试**

运行：

```bash
pnpm release:manifest:test
```

预期：所有测试通过。

- [x] **步骤 3：检查 diff 和工作区**

运行 `git diff --check`，确认没有空白错误；运行 `git status --short`，确认只包含本次发布自动化相关文件。
