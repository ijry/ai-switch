# Skills 设置移植实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 将 codeg 的 Skills 设置能力移植到 AI Switch，支持 11 个客户端的全局/项目 Skill 列表、预览、编辑、新建、保存和删除，并在左侧 MCP 入口下方增加独立 Skills 入口。

**架构：** Skills 以真实 Skill 文件为权威来源。后端拆分为模型、客户端目录规则、路径安全、front matter 解析、文件服务和 command；前端拆分为工具栏、列表、编辑器、预览和作用域选择器。项目目录由桌面目录选择器或 Web 文本路径提供，所有最终路径由后端重新校验。

**技术栈：** Rust 2021、Tauri 2、`serde`、`serde_yaml`、`serde_json`、React 18、TypeScript、TanStack Query、`@tauri-apps/plugin-dialog`、`lucide-react`、现有 transport 和 Vitest。

## 全局约束

- 支持 Codex、Claude Code、Gemini CLI、Grok、OpenCode、OpenClaw、Hermes、Cline、Cursor、Kimi Code、CodeBuddy 共 11 个客户端。
- Skill 文件和目录是权威来源，不使用 `prompt_assets` 表作为运行时唯一数据源。
- 左侧系统区顺序必须为 `MCP`、`Skills`、`Settings`；实现时保留已经存在的 MCP 入口。
- 作用域为 `global` 或 `project`；项目作用域必须绑定已存在的目录。
- Skill ID 禁止路径穿越和控制字符；内置目录中的 Skill 后端只读。
- Web 写命令 `skills_save`、`skills_delete` 必须受敏感命令门禁保护。
- 错误、日志和前端提示不得泄露 Skill 正文中的 token、密钥或环境变量值。
- 从 `codeg` 移植或改写的后端文件保留 Apache-2.0 来源和修改声明；复用 MCP 计划生成的许可证文件，不重复创建不同版本。
- 不覆盖工作区中已有未提交改动；每次提交只包含当前任务文件。

---

### 任务 1：建立 Skills 模型、客户端目录规格和路径安全接口

**文件：**
- 创建：`src-tauri/src/skills/mod.rs`
- 创建：`src-tauri/src/skills/model.rs`
- 创建：`src-tauri/src/skills/paths.rs`
- 创建：`src-tauri/src/skills/clients/mod.rs`
- 修改：`src-tauri/Cargo.toml`（仅在 MCP 计划尚未添加 `serde_yaml` 时添加）
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/commands/mod.rs`
- 测试：`src-tauri/src/skills/paths.rs` 内单元测试

**接口：**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillAgentType {
    ClaudeCode, Codex, Gemini, Grok, OpenClaw, OpenCode,
    Hermes, Cline, Cursor, KimiCode, CodeBuddy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope { Global, Project }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillLayout { MarkdownFile, SkillDirectory }

#[derive(Debug, Clone, Serialize)]
pub struct SkillItem {
    pub id: String,
    pub name: String,
    pub scope: SkillScope,
    pub layout: SkillLayout,
    pub path: String,
    pub description: Option<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillLocation { pub scope: SkillScope, pub path: String, pub exists: bool }

#[derive(Debug, Clone, Serialize)]
pub struct SkillsListResult {
    pub supported: bool,
    pub message: Option<String>,
    pub locations: Vec<SkillLocation>,
    pub skills: Vec<SkillItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillContent { pub skill: SkillItem, pub content: String }
```

每个客户端的目录规则必须由 `SkillStorageSpec` 返回：布局能力、全局目录、项目相对目录和只读根目录。

**目录规则：**

| 客户端 | 全局目录 | 项目相对目录 | 布局/只读规则 |
| --- | --- | --- | --- |
| Claude Code | `~/.claude/skills` | `.claude/skills` | 目录型 |
| Codex | `$CODEX_HOME/skills`、`$CODEX_HOME/skills/.system`、`~/.agents/skills` | `.codex/skills`、`.agents/skills` | 目录型和 `.md`；`.system` 只读 |
| Gemini CLI | `~/.gemini/skills`、`~/.agents/skills` | `.gemini/skills`、`.agents/skills` | 目录型 |
| Grok | `$GROK_HOME/skills` 或 `~/.grok/skills` | `.grok/skills` | 目录型 |
| OpenCode | `~/.config/opencode/skills`、`~/.agents/skills` | `.agents/skills`、`.opencode/skills` | 目录型 |
| OpenClaw | `~/.openclaw/skills` | `skills` | 目录型 |
| Hermes | `~/.hermes/skills` | 无 | 目录型 |
| Cline | `~/.agents/skills`、`~/.cline/skills` | `.agents/skills`、`.cline/skills`、`.clinerules/skills`、`.claude/skills` | 目录型 |
| Cursor | `~/.cursor/skills`、`~/.agents/skills`、`~/.cursor/skills-cursor` | `.cursor/skills`、`.agents/skills` | 目录型；`skills-cursor` 只读 |
| Kimi Code | `$KIMI_CODE_HOME/skills` 或 `~/.kimi-code/skills` | `.kimi-code/skills` | 目录型 |
| CodeBuddy | `~/.codebuddy/skills` | `.codebuddy/skills` | 目录型 |

- [ ] **步骤 1：写路径规格失败测试**

```rust
#[test]
fn codex_marks_system_directory_read_only() {
    let spec = skill_storage_spec(SkillAgentType::Codex);
    assert!(spec.is_read_only_path(Path::new("C:/home/.codex/skills/.system/imagegen")));
    assert!(!spec.is_read_only_path(Path::new("C:/home/.codex/skills/my-skill")));
}

#[test]
fn rejects_project_path_escape() {
    let root = PathBuf::from("C:/workspace");
    assert!(resolve_skill_path(&root, "../outside", SkillLayout::SkillDirectory).is_err());
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test skills::paths::tests`

预期：FAIL，提示规格和路径函数未定义。

- [ ] **步骤 3：实现模型和目录规格**

按表格实现 11 个客户端规格；环境变量 `CODEX_HOME`、`GROK_HOME`、`KIMI_CODE_HOME` 支持 `~` 展开和绝对路径；将所有输入路径 `canonicalize` 后比较前缀，拒绝非目录项目路径和 Skill ID 中的 `/`、`\`、`..`、控制字符。

- [ ] **步骤 4：运行测试确认通过**

运行：`cd src-tauri && cargo test skills::paths::tests`

预期：PASS，覆盖全部客户端、作用域、环境变量路径和越界拒绝。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/skills src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src-tauri/Cargo.toml
git commit -m "feat: scaffold Skills storage rules"
```

### 任务 2：实现 front matter 解析、Skill 扫描和读取

**文件：**
- 创建：`src-tauri/src/skills/frontmatter.rs`
- 创建：`src-tauri/src/skills/service.rs`
- 修改：`src-tauri/src/skills/mod.rs`
- 测试：`src-tauri/src/skills/frontmatter.rs` 内单元测试
- 测试：`src-tauri/src/skills/service.rs` 内临时目录测试

**接口：**

```rust
pub fn parse_skill_metadata(content: &str) -> Result<Option<SkillMetadata>, AppError>;
pub fn list_skills(agent: SkillAgentType, scope: SkillScope, workspace_path: Option<&Path>) -> Result<SkillsListResult, AppError>;
pub fn read_skill(agent: SkillAgentType, scope: SkillScope, skill_id: &str, workspace_path: Option<&Path>) -> Result<SkillContent, AppError>;
```

`SkillMetadata` 至少包含 `name: Option<String>` 和 `description: Option<String>`；没有 front matter 时列表仍然成功，`name` 回退为 Skill ID，`description` 为 `None`。

- [ ] **步骤 1：写 front matter 和扫描失败测试**

```rust
#[test]
fn parses_name_and_description_from_yaml_front_matter() {
    let metadata = parse_skill_metadata("---\nname: demo\ndescription: hello\n---\n# Body").unwrap().unwrap();
    assert_eq!(metadata.name.as_deref(), Some("demo"));
    assert_eq!(metadata.description.as_deref(), Some("hello"));
}

#[test]
fn scans_only_valid_skill_layouts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("demo")).unwrap();
    std::fs::write(dir.path().join("demo/SKILL.md"), "---\nname: Demo\n---\n# Demo").unwrap();
    std::fs::write(dir.path().join("single.md"), "# Single").unwrap();
    std::fs::write(dir.path().join("invalid.txt"), "ignored").unwrap();
    let result = list_skills_from_dir(
        SkillScope::Global,
        dir.path(),
        SkillStorageKind::SkillDirectoryOrMarkdownFile,
    )
    .unwrap();
    assert_eq!(result.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["demo", "single"]);
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test skills::frontmatter::tests skills::service::tests`

预期：FAIL，提示解析和扫描函数未实现。

- [ ] **步骤 3：实现解析和扫描**

front matter 只在正文以 `---` 开始时解析，使用 `serde_yaml` 映射为字符串字段；扫描时仅接受目录中的 `SKILL.md`，并在 `SkillStorageKind::SkillDirectoryOrMarkdownFile` 下额外接受 `<id>.md`。按 scope、id 稳定排序并去重，记录每个候选位置是否存在。

- [ ] **步骤 4：实现读取和只读标记**

根据 `SkillLayout` 计算内容路径，读取 UTF-8 文本；路径属于只读根时把 `read_only` 设为 true，但仍允许读取。目录不存在返回空列表而不是错误；权限错误和无效 UTF-8 返回 `skills.config_invalid`。

- [ ] **步骤 5：运行测试确认通过**

运行：`cd src-tauri && cargo test skills::frontmatter::tests skills::service::tests`

预期：PASS，覆盖 front matter、空目录、多候选目录去重、只读标记和读取内容。

- [ ] **步骤 6：提交**

```bash
git add src-tauri/src/skills/frontmatter.rs src-tauri/src/skills/service.rs src-tauri/src/skills/mod.rs
git commit -m "feat: list and read agent Skills"
```

### 任务 3：实现 Skill 保存、删除和只读保护

**文件：**
- 修改：`src-tauri/src/skills/service.rs`
- 修改：`src-tauri/src/skills/paths.rs`
- 测试：`src-tauri/src/skills/service.rs` 内单元测试

**接口：**

```rust
pub fn save_skill(
    agent: SkillAgentType,
    scope: SkillScope,
    skill_id: &str,
    content: &str,
    layout: Option<SkillLayout>,
    workspace_path: Option<&Path>,
) -> Result<SkillItem, AppError>;

pub fn delete_skill(
    agent: SkillAgentType,
    scope: SkillScope,
    skill_id: &str,
    workspace_path: Option<&Path>,
) -> Result<bool, AppError>;
```

- [ ] **步骤 1：写保存/删除失败测试**

```rust
#[test]
fn saves_new_directory_skill_and_preserves_content() {
    let dir = tempfile::tempdir().unwrap();
    let content = "---\nname: demo\ndescription: test\n---\n# Body\n";
    let item = save_skill_at(
        SkillAgentType::ClaudeCode,
        SkillScope::Global,
        "demo",
        content,
        Some(SkillLayout::SkillDirectory),
        dir.path(),
    )
    .unwrap();
    assert_eq!(item.layout, SkillLayout::SkillDirectory);
    assert_eq!(std::fs::read_to_string(dir.path().join("demo/SKILL.md")).unwrap(), content);
}

#[test]
fn rejects_write_to_read_only_skill() {
    let error = save_existing_system_skill_fixture();
    assert_error_code(error, "skills.read_only");
}

#[test]
fn deleting_unknown_skill_returns_false() {
    let dir = tempfile::tempdir().unwrap();
    let removed = delete_skill_at(
        SkillAgentType::ClaudeCode,
        SkillScope::Global,
        "missing",
        dir.path(),
    )
    .unwrap();
    assert!(!removed);
}
```

- [ ] **步骤 2：运行测试确认失败**

运行：`cd src-tauri && cargo test skills::service::tests::saves_new_directory_skill_and_preserves_content`

预期：FAIL，提示保存函数未实现。

- [ ] **步骤 3：实现布局选择和安全保存**

目录型 Skill 保存到 `<dir>/<id>/SKILL.md`；单文件型保存到 `<dir>/<id>.md`。已有 Skill 优先沿用已有布局；新建时使用客户端默认布局。为测试提供接收显式根目录的 `save_skill_at`/`delete_skill_at`，生产函数只负责从客户端规格解析根目录。创建目录前再次验证目标位于允许根下；使用同目录临时文件替换 `SKILL.md`/`.md`，不把正文写入日志。

- [ ] **步骤 4：实现删除和只读拒绝**

删除前重新定位 Skill 并检查 `read_only`；目录型删除整个 Skill 目录，单文件型删除文件；不存在返回 false；只读或路径越界返回稳定错误码，不进行任何文件修改。

- [ ] **步骤 5：运行测试确认通过**

运行：`cd src-tauri && cargo test skills::service::tests`

预期：PASS，覆盖新建、编辑、布局保持、删除、路径穿越和只读拒绝。

- [ ] **步骤 6：提交**

```bash
git add src-tauri/src/skills/service.rs src-tauri/src/skills/paths.rs
git commit -m "feat: save and delete Skills safely"
```

### 任务 4：接入 Skills Tauri/Web command 和前端 API 类型

**文件：**
- 创建：`src-tauri/src/skills/command.rs`
- 修改：`src-tauri/src/skills/mod.rs`
- 修改：`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/web/handlers/mod.rs`
- 修改：`src/lib/api/client.ts`
- 修改：`src/lib/api/types.ts`
- 修改：`tests/transport/command-contract.test.ts`

**接口：** 命令名称和参数必须固定为：

```text
skills_list_agents()
skills_list(agent_type, scope, workspace_path?)
skills_read(agent_type, scope, skill_id, workspace_path?)
skills_save(agent_type, scope, skill_id, content, layout?, workspace_path?)
skills_delete(agent_type, scope, skill_id, workspace_path?)
```

- [ ] **步骤 1：先扩展前端类型和 API 封装**

在 `src/lib/api/types.ts` 定义与 Rust 模型对应的 `SkillAgentType`、`SkillScope`、`SkillLayout`、`SkillItem`、`SkillsListResult`、`SkillContent`。添加：

```ts
export function skillsList(input: {
  agent_type: SkillAgentType;
  scope: SkillScope;
  workspace_path?: string | null;
}): Promise<SkillsListResult> {
  return invoke("skills_list", input);
}
```

先运行 `pnpm test:run -- tests/transport/command-contract.test.ts`，预期因后端注册缺失而 FAIL。

- [ ] **步骤 2：实现 Tauri command 薄层并注册**

command 层只负责反序列化参数并调用 `skills::service`；加入 `skills_list_agents`、`skills_list`、`skills_read`、`skills_save`、`skills_delete` 到 `generate_handler!`。

- [ ] **步骤 3：实现 Web dispatch 和敏感命令门禁**

在 `dispatch_command` 增加五个分支；`skills_save`、`skills_delete` 加入 `is_sensitive_command`，其余命令保持 Web 只读可用。Web 参数使用 snake_case，与 Tauri API 封装一致。

- [ ] **步骤 4：运行契约测试和 Rust 测试**

运行：`pnpm test:run -- tests/transport/command-contract.test.ts && cd src-tauri && cargo test web::handlers::tests`。

预期：PASS，五个命令都存在 Tauri/Web 注册，敏感门禁关闭时保存和删除返回 404。

- [ ] **步骤 5：提交**

```bash
git add src-tauri/src/skills/command.rs src-tauri/src/skills/mod.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/client.ts src/lib/api/types.ts tests/transport/command-contract.test.ts
git commit -m "feat: expose Skills commands over app transports"
```

### 任务 5：接入 Skills 左侧入口、路由和国际化

**文件：**
- 创建：`src/screens/SkillsScreen.tsx`
- 修改：`src/components/layout/AppLayout.tsx`
- 修改：`src/App.tsx`
- 修改：`src/lib/i18n.tsx`
- 修改：`tests/AppLayout.test.tsx`

**接口：**
- `SkillsScreen` 无必需 props，默认客户端为 Codex、默认作用域为 `global`。
- `AppLayout` 在已有 `MCP` 按钮之后、`Settings` 之前渲染 `Skills` 按钮，并把 `Skills` 加入设置高亮屏幕集合。
- `App` 将 `Skills` 加入 `implementedScreens`，在 `screen === "Skills"` 时渲染 `SkillsScreen`。

- [ ] **步骤 1：写导航失败测试**

```tsx
it("renders Skills below MCP and above Settings", () => {
  render(<AppLayout {...defaultProps} activeScreen="Skills" />);
  const labels = screen.getAllByRole("button").map((item) => item.textContent ?? "");
  const mcp = labels.findIndex((label) => label.includes("MCP"));
  const skills = labels.findIndex((label) => label.includes("Skills"));
  const settings = labels.findIndex((label) => label.includes("Settings"));
  expect(mcp).toBeGreaterThanOrEqual(0);
  expect(mcp).toBeLessThan(skills);
  expect(skills).toBeLessThan(settings);
});
```

运行：`pnpm test:run -- tests/AppLayout.test.tsx`，预期 FAIL。

- [ ] **步骤 2：实现入口和路由**

使用 `lucide-react` 的 `Sparkles` 图标；保留 MCP 入口和 Settings 入口的 active 状态；加入 `nav.skills` 及 Skills 页面公共 loading/error/empty 文案的英文和简体中文翻译。

- [ ] **步骤 3：运行导航测试**

运行：`pnpm test:run -- tests/AppLayout.test.tsx`。

预期：PASS。

- [ ] **步骤 4：提交**

```bash
git add src/screens/SkillsScreen.tsx src/components/layout/AppLayout.tsx src/App.tsx src/lib/i18n.tsx tests/AppLayout.test.tsx
git commit -m "feat: add Skills settings navigation entry"
```

### 任务 6：实现 Skills 工具栏、目录选择和列表

**文件：**
- 创建：`src/components/skills/SkillsToolbar.tsx`
- 创建：`src/components/skills/SkillScopePicker.tsx`
- 创建：`src/components/skills/SkillsList.tsx`
- 修改：`src/screens/SkillsScreen.tsx`
- 测试：`tests/SkillsScreen.test.tsx`

**接口：**
- `SkillsToolbar` 接收当前客户端、作用域、项目路径和回调。
- `SkillScopePicker` 在桌面运行时调用 `open({ directory: true, multiple: false })`；Web 运行时显示文本路径输入，不导入 `@tauri-apps/plugin-dialog` 的硬依赖。
- `SkillsList` 接收 `SkillItem[]`、筛选词、选择回调和删除回调。

- [ ] **步骤 1：写加载和作用域切换失败测试**

```tsx
it("loads global Codex skills and passes project path after choosing Project", async () => {
  vi.mocked(skillsListAgents).mockResolvedValue(agentFixture);
  vi.mocked(skillsList).mockResolvedValue(codexSkillsFixture);
  render(<SkillsScreen />);
  expect(await screen.findByText("imagegen")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("radio", { name: /Project|项目/ }));
  await userEvent.type(screen.getByRole("textbox", { name: /Project path|项目目录/ }), "C:/workspace");
  expect(skillsList).toHaveBeenLastCalledWith(expect.objectContaining({ scope: "project", workspace_path: "C:/workspace" }));
});
```

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`，预期 FAIL。

- [ ] **步骤 2：实现客户端和作用域工具栏**

加载 `skillsListAgents` 后默认选择 Codex；切换客户端或作用域时清空当前 Skill 和编辑草稿；项目作用域没有路径时不调用列表命令，显示目录选择空状态。桌面目录选择结果写入本地组件状态，Web 允许手动输入绝对路径。

- [ ] **步骤 3：实现列表筛选和状态**

列表按 id、name、description 过滤，显示 scope、layout 和 read-only 标记；加载、空目录、客户端不支持和错误状态独立渲染；刷新按钮只重新调用当前客户端/作用域。

- [ ] **步骤 4：运行测试确认通过**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx`

预期：PASS，覆盖全局/项目切换、目录缺失、搜索、空列表和切换清理状态。

- [ ] **步骤 5：提交**

```bash
git add src/components/skills/SkillsToolbar.tsx src/components/skills/SkillScopePicker.tsx src/components/skills/SkillsList.tsx src/screens/SkillsScreen.tsx tests/SkillsScreen.test.tsx
git commit -m "feat: add Skills scope and list UI"
```

### 任务 7：实现 Skill 编辑、预览、保存和删除交互

**文件：**
- 创建：`src/components/skills/SkillEditor.tsx`
- 创建：`src/components/skills/SkillPreview.tsx`
- 修改：`src/screens/SkillsScreen.tsx`
- 修改：`src/lib/i18n.tsx`
- 测试：`tests/SkillsEditor.test.tsx`

**接口：**
- `SkillEditor` 接收 `SkillContent | null`、草稿 id、正文、布局和 `read_only`，发出 `onSave`、`onDelete`、`onNew`。
- `SkillPreview` 接收 Markdown 正文和 front matter 摘要；使用现有轻量 Markdown 渲染依赖或纯文本 fallback，不新增大型编辑器依赖。

- [ ] **步骤 1：写编辑交互失败测试**

```tsx
it("reads a selected skill and saves edited content", async () => {
  vi.mocked(skillsRead).mockResolvedValue({ skill: skillFixture, content: "# Old" });
  render(<SkillsScreen />);
  await userEvent.click(await screen.findByText(skillFixture.name));
  const editor = await screen.findByRole("textbox", { name: /Skill content|Skill 内容/ });
  await userEvent.clear(editor);
  await userEvent.type(editor, "# New");
  await userEvent.click(screen.getByRole("button", { name: /Save|保存/ }));
  expect(skillsSave).toHaveBeenCalledWith(expect.objectContaining({ skill_id: skillFixture.id, content: "# New" }));
});

it("disables save and delete for read-only skill", async () => {
  render(<SkillsScreen initialSkill={readOnlySkillFixture} />);
  expect(screen.getByRole("button", { name: /Save|保存/ })).toBeDisabled();
  expect(screen.getByRole("button", { name: /Delete|删除/ })).toBeDisabled();
});
```

- [ ] **步骤 2：实现读取和三栏布局**

选择列表项调用 `skillsRead`，中栏显示完整正文，右栏显示预览；新建时生成安全默认 Skill ID 和 Markdown 模板；编辑器使用固定最小高度和 `aria-label`，避免内容变化导致布局跳动。

- [ ] **步骤 3：实现保存、删除和冲突状态**

保存调用 `skillsSave`，成功后重新列表并重新读取当前项；删除使用确认对话框，成功后选择下一个 Skill；只读项在 UI 禁用按钮，同时后端错误仍显示结构化提示；保存/删除进行中只锁定当前动作。

- [ ] **步骤 4：实现预览和 front matter 摘要**

预览显示 Markdown 正文，摘要显示 name/description；无法解析 front matter 不阻塞编辑；所有文本遵循现有中英文 i18n。

- [ ] **步骤 5：运行测试确认通过**

运行：`pnpm test:run -- tests/SkillsEditor.test.tsx`

预期：PASS，覆盖读取、编辑、保存、删除确认、只读禁用、空草稿和错误反馈。

- [ ] **步骤 6：提交**

```bash
git add src/components/skills/SkillEditor.tsx src/components/skills/SkillPreview.tsx src/screens/SkillsScreen.tsx src/lib/i18n.tsx tests/SkillsEditor.test.tsx
git commit -m "feat: add Skills editor and preview"
```

### 任务 8：完成 Skills 回归验证

**文件：**
- 修改：`tests/transport/command-contract.test.ts`（仅在前序任务遗漏时补充）
- 修改：`src-tauri/src/skills/*`（仅修复测试发现的问题）

- [ ] **步骤 1：运行 Skills 前端测试和类型检查**

运行：`pnpm typecheck && pnpm test:run -- tests/SkillsScreen.test.tsx tests/SkillsEditor.test.tsx tests/AppLayout.test.tsx`

预期：PASS。

- [ ] **步骤 2：运行 Rust 检查和测试**

运行：`pnpm rust:check && pnpm rust:test`

预期：无编译错误，路径、front matter、扫描、读写、只读和 Web dispatch 测试全部 PASS。

- [ ] **步骤 3：运行全量回归**

运行：`pnpm test:run`

预期：既有账户、设置、传输和屏幕测试不回归，MCP 计划新增测试继续通过。

- [ ] **步骤 4：检查工作区和提交修复**

运行：`git status --short`，确认未覆盖用户原有改动；若只修复本计划产生的问题，执行：

```bash
git add src-tauri/src/skills src/screens/SkillsScreen.tsx src/components/skills tests
git commit -m "test: verify Skills settings migration"
```
