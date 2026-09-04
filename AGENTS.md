# Repository Rules

- Work directly on `main` by default. Do not create or switch to feature branches/worktrees unless the user explicitly asks for a separate branch, worktree, or isolation.
- `docs/superpowers/specs` 和 `docs/superpowers/plans` 中的文档默认使用中文撰写，除非用户明确要求其他语言。

## 构建目录约束

- 仓库内只允许使用两个 Rust/Cargo 构建目录：`src-tauri/target/` 仅用于本地 dev 运行（包括 `pnpm tauri:dev`），`src-tauri/target-codex/` 仅用于 AI 调试、测试和验证。
- AI 调试或验证命令必须复用 `src-tauri/target-codex/`；在 `src-tauri` 工作目录执行时使用 `CARGO_TARGET_DIR=target-codex`。禁止创建根目录 `target/`、`target-*`、`src-tauri/target-codex-test/`、`src-tauri/target-codex-pairing/` 或 `.codex-run/**/target` 等其他 target 目录。
- 不得为并行任务或临时检查新建第三个 target。任务结束后清理临时构建产物；若既有 target 被进程占用，先停止对应的 dev/调试进程，再复用或清理该目录。

## 发布新版规则

当用户说“发布新版”时，按以下顺序执行：

1. 先检查 GitHub Actions 中 `main` 分支最近一次 CI 状态；如果 CI 失败或仍在运行，先报告状态并停止发布。若没有可用的 CI 记录，至少运行仓库发布流程中的本地检查：`pnpm typecheck`、`pnpm test:run`、`pnpm release:manifest:test`、`pnpm rust:check`、`pnpm rust:test` 和 `go test ./...`（工作目录为 `sidecar/ai-switch-tsnet`）。任一检查失败时不得提交或打 tag。
2. 找出最新的语义化版本 tag，分析该 tag 到当前最新代码（包括待提交工作区改动）的变化，生成条目化更新日志。更新日志必须通俗易懂，分别提供中文和英文版本，按功能、修复、改进等用户可理解的内容描述，不直接堆砌 commit hash 或内部实现细节。
3. 使用同一份双语更新日志作为 Git 提交信息，提交标题简洁明确，正文保留完整的中文和英文条目；不要另外创建持久化的 CHANGELOG 文件，除非用户明确要求。提交正文必须按「中文条目 → 空行 → 独占一行的 29 个连字符（`-----------------------------`）→ 空行 → 英文条目」的顺序排列：发布流水线把这条提交信息原样写入更新清单的 `notes`（GitHub Release 正文则是在它前面再拼一张下载表），客户端据此按界面语言只显示对应语种的那一半。除该分隔线外，正文其余位置不要出现 12 个以上连续的 `-`、`*` 或 `_`。
4. 根据变更类型自动递增版本号：破坏性变更递增 major，新增用户功能递增 minor，修复和一般改进递增 patch。若用户明确指定版本号，使用用户指定的版本。同步更新 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`，并在需要时更新 `src-tauri/Cargo.lock` 中项目自身的版本条目，保持所有应用版本一致。
5. 提交版本和代码改动后创建对应的 annotated tag，例如 `git tag -a v0.4.2 -m "Release v0.4.2"`。tag 名称必须与应用版本一致；预发布版本使用 `-rc.N`、`-beta.N` 或 `-alpha.N` 后缀，并同步写入所有版本配置。
6. 默认只提交代码并创建本地 tag，不自动推送；只有用户明确要求“推送”或“提交推送”时，才执行 `git push origin main` 和对应的 `git push origin <tag>`。推送前再次确认工作区干净、提交和 tag 指向正确。
