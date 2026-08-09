# 发布说明自动化设计

日期：2026-07-26
状态：已批准

## 目标

在标签自动发布工作流中生成 GitHub Release 发布说明，并将同一份内容写入 Tauri 更新清单，使桌面端更新窗口可以显示更新日志。

## 方案

- 保留现有 `ncipollo/release-action`、多平台构建和发布资源流程。
- 发布资源生成前，先使用 `ncipollo/release-action` 创建或更新一个草稿 Release，并启用 `generateReleaseNotes: true`。
- 通过 GitHub API 分页读取 Release 列表，并按 tag 筛选草稿 Release 的正文，保存为临时的 `release-notes.md` 文件。不能使用 `releases/tags/{tag}` 接口，因为该接口对草稿 Release 返回 404。
- 扩展 `scripts/create-updater-manifest.mjs`，支持可选的 `--notes-file` 参数，并将文件内容写入 `latest.json` 的 `notes` 字段。
- 校验清单和签名后，再次执行 `ncipollo/release-action` 上传资源，并将草稿 Release 自动发布。
- 不新增提交到仓库的 `CHANGELOG.md`，也不维护自定义 commit 解析脚本。

## 兼容性

- 未传入 `--notes-file` 时，manifest 的 `notes` 保持为空，兼容原有脚本调用方式。
- 更新界面已经读取 `update.body`，无需修改前端展示逻辑。

## 验证

- 运行 `pnpm release:manifest:test`，确认发布说明能写入 `latest.json`。
- 运行 `pnpm typecheck`、`pnpm test:run` 和 Rust 检查，确认发布流程依赖的项目检查仍然通过。
- 在 GitHub Actions 中确认草稿 Release 正文与 `latest.json.notes` 内容一致。
