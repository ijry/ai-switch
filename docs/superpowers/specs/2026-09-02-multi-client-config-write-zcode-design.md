# 多客户端配置写入：ZCode 接入算力池

日期：2026-09-02
状态：设计已确认，待实现

## 背景

`写入路由配置文件` 目前是 platform → 单适配器的一对一映射：每个平台只有一个原生客户端可写（Codex CLI、Claude Code、Gemini CLI、Grok）。用户希望在 ZCode 中接入 ai-switch 的算力池，手动配置繁琐。ZCode 之后还会有别的客户端提出同样需求，因此本次做通用机制，ZCode 作为第一个落地实例。

## 关键外部事实（ZCode 配置逆向结论）

以下结论来自本机 ZCode 3.10.2 的 `C:\Program Files\ZCode\resources\app.asar` 与 `~/.zcode/v2/config.json`，是本设计的前提：

- 配置文件为 `~/.zcode/v2/config.json`，根级 `provider` 是一个 record，键为 provider id。
- **记录键可以是任意字符串**，bundle 中没有 UUID 校验；但不能以 `builtin:` 或 `default-` 开头（前者会被强制判为内置并从 agent registry 中排除，后者在 apiKey 为空时会被过滤）。
- ZCode 自己重写该文件时，会丢弃根级 `$schema` 和条目内以 `zcode` 开头的键，**其他自定义键在条目内可以存活**。因此托管标记可以写在条目内。
- `models` 为空的 provider 在 ZCode 中不可选（运行时 schema 要求至少一条模型且 apiKey 非空），且 ZCode **不会对自定义 provider 请求 `/v1/models`**。模型清单必须由写入方提供。
- `kind` 决定请求路径，也决定 baseURL 的写法：
  - `kind: "openai"` → `{baseURL}/responses`
  - `kind: "anthropic"` → `{baseURL}/v1/messages`
  - `kind: "openai-compatible"` → `{baseURL}/chat/completions`
- 配置中显式写 `apiFormat` **无效**：只要条目带 `kind`（或 `options.baseURL` / `api` / `npm`），ZCode 会用 `kind` 重算并覆盖 `apiFormat`。
- ZCode **不监听**该文件变化（bundle 中无 watcher）。写入后需重启 ZCode 或切到 provider 设置页才生效。
- ZCode 自己的写入是"读盘 → 改 → 临时文件 → rename"，配 `config.json.lock` 目录锁。
- **文件 schema 解析失败时，ZCode 静默回落到 legacy 文件并最终得到空 provider 列表。** 即写坏此文件会导致用户所有 provider 消失，不只是 ai-switch 那条。这使"必须走现有 snapshot/备份/回滚管道"成为硬要求。

## 范围决策

- **复用现有平台池**：不新增 `PlatformId`。ZCode 写入时复用目标平台的 sk（`route_proxy_keys` 以 platform 为主键）和该平台的池。因此平台枚举的 12 处硬编码（`PlatformId::ALL`、能力矩阵、`AgentIcon`、`AppLayout` 导航、`VibeScreen` 页签、`platformLabels` 等）一处都不需要改动。
- **ZCode 只开 codex 与 claude 两个平台**：这两条落在已有协议桥分支上，行为可推定。grok / gemini 在 ZCode 目标上不提供（ZCode 的 `kind` 枚举无 gemini；grok 走 `openai-compatible` 未经实测）。
- **模型清单纯从池映射生成**：接管既有条目时整份 `models` 替换，用户手工添加的模型会被移除。已与用户确认；因此托管标记只需 `managed` 与 `platform` 两个字段，不做 `managedModels` 记账。

## 架构：适配器与注册表

`TargetAdapter` 新增三个方法：

- `client_key()` — 客户端标识。现有四个适配器的 `client_key` 恰好等于 `target_key`；ZCode 的两个实例共享 `client_key = "zcode"`。
- `client_display_name()` — 客户端显示名，用于弹窗与结果面板。
- `native()` — 该客户端是否为此平台的第一方 CLI。用于弹窗首次打开时的默认勾选与 settings 无记录时的回退。设为必填以强制每个适配器表态。

`platform()` 语义不变：一个适配器实例只服务一个平台。

| 适配器 | client_key | target_key | platform | native |
|---|---|---|---|---|
| `CodexAdapter` | `codex` | `codex` | codex | true |
| `JsonAgentAdapter::claude()` | `claude_code` | `claude_code` | claude | true |
| `JsonAgentAdapter::gemini()` | `gemini_cli` | `gemini_cli` | gemini | true |
| `JsonAgentAdapter::grok()` | `grok` | `grok` | grok | true |
| `ZCodeAdapter::codex()` | `zcode` | `zcode_codex` | codex | false |
| `ZCodeAdapter::claude()` | `zcode` | `zcode_claude` | claude | false |

注册表维持两条唯一性不变量，由测试守护：`target_key` 全局唯一；`(client_key, platform)` 全局唯一。

注册表 API 变更：

- `by_target_key` 原样保留（回滚、reconcile、Targets 页依赖，不动）。
- 新增 `by_client_and_platform(client_key, platform)`，写入路径使用。
- 新增 `clients_for_platform(platform) -> Vec<ClientTargetDescriptor>`，驱动弹窗。
- **删除 `for_platform`**：有了显式客户端选择后语义已含糊（"该平台的哪个客户端"）。现有 5 处调用改为 `by_client_and_platform` 或走新的选择逻辑。

`target_apps` 新增两行种子：`(zcode_codex, codex, "ZCode (Codex)")`、`(zcode_claude, claude, "ZCode (Claude)")`。`TargetRepository::ensure_defaults` 的种子表已是幂等插入 + platform 回填，无需迁移文件。

**`validate_adapter_target`（DB 行的 platform 必须等于适配器的 platform）不做任何修改。** 快照、回滚、路径锁、`target_state` 的粒度仍然是按 (客户端, 平台)。这是选择本方案而非"适配器声明支持多平台"的主要理由：后者需要放宽这条防止写错文件的不变量。

### 同文件冲突不会发生

ZCode 的两个适配器共用 `~/.zcode/v2/config.json`。若两者进入同一个 `write_group`，第一个提交后第二个的 `write_atomic_if_unchanged` 会因原文件哈希已变而报 `config.concurrent_modification`，触发整组回滚。

这不会发生，有两重保证：`write_route_proxy_configs` 每次只处理一个 `platform`（弹窗选择的是客户端，不是平台），而 `(client_key, platform)` 唯一意味着一个平台下每个客户端最多解析出一个适配器；加之写入改为每客户端一次 `write_group`（见下文），每组实际只含一个请求。注册表唯一性由测试守护，是这条推理的支点。

## ZCode 适配器

新建 `src-tauri/src/adapters/route_config/zcode.rs`。结构体 `ZCodeAdapter { target_key, platform, kind, base_url_suffix }` 参数化出两个实例：

| | codex 实例 | claude 实例 |
|---|---|---|
| `kind` | `openai` | `anthropic` |
| 写入的 baseURL | `{base}/v1` | `{base}`（不加 `/v1`） |
| ZCode 实际请求 | `{base}/v1/responses` | `{base}/v1/messages` |

claude 实例若带 `/v1` 会得到 `/v1/v1/messages`，因此后缀必须按 `kind` 区分。

`resolve_path(home)` → `home/.zcode/v2/config.json`。

**不支持 `ZCODE_DATA_BASE_DIR` / `setting.json:dataBaseDir` 覆盖。** `resolve_path(home)` 是 trait 签名的全部输入，而 `validate_snapshot_path` 要求回滚时重新解析出的路径与快照记录的字节级一致；引入外部可变的路径来源会让一次环境变量改动使历史快照全部变为 `config.path_unsafe`。改过目录的用户看到"未建立"状态，是可解释的降级。

### 渲染

加法式渲染：解析 JSON → 定位或新建自己那条 provider → 只改这一条 → 序列化。同文件内另一平台的条目以及用户手工添加的其他 provider 全部原样保留。

写入的条目形状（以 codex 实例为例）：

```json
{
  "name": "AI Switch (Codex)",
  "kind": "openai",
  "source": "custom",
  "options": {
    "apiKey": "<platform sk>",
    "baseURL": "http://127.0.0.1:19527/v1",
    "apiKeyRequired": true
  },
  "models": {
    "gpt-5.6-sol": {
      "limit": { "context": 200000, "output": 128000 },
      "modalities": { "input": ["text"], "output": ["text"] }
    }
  },
  "aiSwitch": { "managed": true, "platform": "codex" }
}
```

- `aiSwitch` 是托管标记，能在 ZCode 自己重写文件时存活。
- **不写 `apiFormat`**：有 `kind` 时会被 ZCode 用 `kind` 重算覆盖，写了是误导。
- **不写 `enabled`**：保持"未显式禁用"。

### 接管既有条目

按顺序取第一个命中：

1. `aiSwitch.managed == true && aiSwitch.platform == 本平台`
2. 归一化 baseURL 指向本机代理，且 `options.apiKey` 等于本平台 sk 或其历史别名（查 `route_proxy_key_aliases`，用户轮换过 key 也能识别）
3. 都未命中则新建，键用固定的 `ai-switch-codex` / `ai-switch-claude`

接管时**保留原有的记录键和 `name`**（用户可能改过名字），只更新 `options`、`kind`、`models` 和 `aiSwitch`。`models` 整份替换。

### inspect

`managed` 判定：存在一条 `aiSwitch.managed == true && aiSwitch.platform == 本平台` 的记录。两个实例读同一文件，因此**必须按平台过滤**——否则只含 claude 条目的文件会让 `zcode_codex` 也报 `managed`，Targets 页两行互相冒认。

### 破坏性风险与防线

写坏此文件会使用户所有 provider 消失，因此现有三道防线一道都不能省：`existing_text` 拒绝非 UTF-8；解析失败抛 `validation.route_config_existing_invalid` 而非覆盖；渲染后重新解析验证。

并发窗口：ZCode 在我们读盘之后、写盘之前完成一次自己的保存。`write_atomic_if_unchanged` 的原文件哈希校验会捕获并报 `config.concurrent_modification`，用户重试即可。**不实现 ZCode 的私有锁协议**——那需要复刻其锁目录格式，而当前失败模式已经是安全的。

## 写入编排

### 命令签名

`write_route_proxy_configs(base_url, platform, client_keys?)`。`client_keys` 缺省时后端按「settings 中该平台的记录 → 无记录则只写 `native` 客户端」回退，旧调用方行为不变。显式传入的键逐个校验，未注册或不支持该平台的报 `config.client_unavailable`，不静默跳过。

settings 新增 `config_write_clients_json`，形如 `{"codex":["codex","zcode"],"claude":["claude_code"]}`。按平台分别记录，因为弹窗总是在某个平台的上下文中打开。

### 每客户端独立一次 write_group

`write_group` 是全组原子的：任一请求 prepare 失败则整组中止。若把 codex 与 zcode 放进同一组，用户的 ZCode 配置文件一旦损坏（`validation.route_config_existing_invalid`），**连 Codex 都写不了**——对最常见场景的可用性倒退。

因此改为每客户端一次 `write_group`，汇总各自的 outcome。部分成功可接受：它们是彼此独立的客户端配置，不是一个事务单元，且结果面板本来就按目标逐条渲染状态。

配套修正：新建的 platform sk 只在**所有**客户端都写失败时才回删。现有代码是单客户端场景下的"失败就删"；多客户端下若 codex 成功而 zcode 失败，删 key 会打断已生效的 codex。

### Codex 模型目录的作用域

`write_codex_model_catalog` 写 `~/.codex/ai-switch-model-catalog.json`，由 Codex CLI 的 `config.toml` 引用。现在的条件是 `platform == Codex` 即写，改为**选中的客户端包含 `codex`** 才写。只勾 ZCode 时不应触碰 `~/.codex/`。

### models 从池生成

将 `route_model_capability.rs` 的 `advertised_model_catalog_entries` 提为 `pub(crate)`，ZCode 与 Codex 模型目录共用同一份"本池对外宣告哪些模型"的事实，两者不会漂移。

模型清单经 `RouteConfigInput` 新增的 `client_models` 字段传入适配器，由服务层从池计算——与 `claude_env` 完全同构（gemini/grok 忽略 `claude_env`，同理 codex/claude 适配器忽略 `client_models`）。附带收益：`config_write_is_stale` 自动覆盖"池映射变了"的情形，因为它本就是用真适配器重新渲染再比对。

每条模型写入：

- `limit.context` — 模型 id 以 `[1m]` 结尾写 `1000000`，其余 `200000`
- `limit.output` — `128000`
- `modalities` — 显式 `{"input":["text"],"output":["text"]}`
- **不写 `name`**（省略时 ZCode 用记录键作显示名）
- **不写模型级 `zcode` 子对象**（该键由 ZCode 自己管，写了会被重写）

池中该平台没有任何在池的 api 账号时，`advertised_model_catalog_entries` 返回空。此时**拒绝写入并报 `config.pool_models_empty`**，而不是写一个在 ZCode 中点不开的死条目。

### stale 判定的作用域

`route_config_write_is_stale` 同样接受可选 `client_keys`，缺省时走与写入一致的回退（settings 记录 → native）。语义是"选中的客户端中存在任一需要重写"：逐客户端用真适配器渲染并比对，任一为真即返回 true。任何一个客户端出错时该客户端记为 false——stale 提示绝不能是让界面崩掉的那个东西，这是现有实现已确立的原则。

因 `route_config_adapter(platform)`（基于即将删除的 `for_platform`）不再适用，此函数改为遍历解析出的客户端适配器。

## 前端

### 按钮行为

`AccountsScreen` 的「写入路由配置文件」按钮从"直接写"改为"打开弹窗"。禁用条件收窄到只剩代理未运行（无 base_url 则无从写起）——平台能力门禁下移到弹窗内逐客户端判定，因为同一平台下不同客户端的可用性可以不同。琥珀色小圆点语义不变：当前选中的客户端中有任一需要重写。

### 新增读命令

`list_config_write_clients(platform)`，每个客户端返回：`client_key`、`display_name`、`native`、`target_key`、`config_path`、`file_status`、`error_code`、`restart_required`。

不复用 `listTargetConfigStatuses`：它跨全平台枚举、且要跑 reconcile 与快照统计，弹窗用不上；更关键的是它不带 `client_key` 与 `native`。因 `(client_key, platform)` 唯一，platform=codex 时正好返回两行（`codex`、`zcode_codex`），前端无需合并。

`restart_required` 的含义是"长驻应用，配置在启动时读取"：ZCode 为 `true`，四个 CLI 为 `false`（下次调用即读到新配置）。这是客户端的事实故放后端；对应的中文提示句留在前端，Rust 中不放 UI 文案。

### 弹窗

`src/components/accounts/ConfigWriteTargetsDialog.tsx`，沿用 `CopyRouteCredentialDialog` 的结构（Escape 关闭、焦点管理、pending 时锁交互）。

- 每行一个复选框，附当前文件状态（未建立 / 已接管 / 未接管 / 无法解析）
- 能力不可用的行禁用并显示原因
- 首次打开（settings 无记录）只勾 `native` 的客户端，默认行为与今天完全一致
- 确认后写入选中项，并把选择存入 `config_write_clients_json`

**重启提示**：勾选任何 `restart_required` 的客户端时，弹窗底部出现「写入后需重启 ZCode 才生效（ZCode 不监听配置文件变化）」。写入成功后，若结果中含 ZCode 目标，结果面板中同句再出现一次——用户很可能写完即切窗口，只在弹窗里提示一次会被漏掉。

结果面板当前直接渲染 `target_key`，会显示为 `zcode_codex`。改为按 `target_key` 映射到客户端显示名。

### 契约

`list_config_write_clients` 需同时注册进 `lib.rs` 的 `generate_handler!`、web handlers 的 match 和 `client.ts`；`tests/transport/command-contract.test.ts` 会验证三处一致。`write_route_proxy_configs` 与 `route_config_write_is_stale` 各加一个可选 `client_keys` 参数，两个传输层都要跟随。

## 错误处理

新增错误码：

| 码 | 场景 | recoverable |
|---|---|---|
| `config.client_unavailable` | 请求的 `client_key` 未注册，或该客户端不支持此平台 | true |
| `config.pool_models_empty` | 池中该平台无可宣告模型，拒绝写入死条目 | true |

复用既有码：文件损坏仍为 `validation.route_config_existing_invalid`（ZCode 场景后果最重，措辞需点明"拒绝覆盖以免丢失你的 provider 配置"）；ZCode 在读盘后自行保存过为 `config.concurrent_modification`；能力门禁不变，仍为 `capability.unavailable`。

## 测试

### Rust

`zcode.rs` 内联：

- codex/claude 两实例的 baseURL 后缀正确（claude 不带 `/v1`）
- 写入保留同文件内其他 provider 与另一平台的条目
- 接管的三条路径各一例：`aiSwitch` 标记、baseURL + apiKey、历史别名 key；三者均不中时用固定键新建
- 接管时保留原记录键与 `name`
- **整份 `models` 替换（手工添加的模型被移除）** —— 这是已确认的行为，测试将其钉为预期而非意外
- `inspect` 按平台过滤：只含 claude 条目的文件对 codex 实例为 `unmanaged`
- 损坏 JSON 抛错且不覆盖

注册表：`target_key` 全局唯一；`(client_key, platform)` 全局唯一；每个适配器 `native()` 有明确值；`clients_for_platform(Codex)` 返回 codex 与 zcode 两项。

服务层：

- 只勾 zcode 时不写 `~/.codex/ai-switch-model-catalog.json`
- **ZCode 文件损坏时 Codex 仍写入成功**（"不用一个 write_group 包住"的核心断言）
- 上述场景下新建的 sk 不被回删
- 池无模型时报 `config.pool_models_empty` 且不落盘
- `config_write_is_stale` 对池映射变化返回 true；某客户端渲染出错时不影响其余客户端的判定

`config_write_service.rs` 的 `Fixture::codex_request()` 现用 `for_platform`，随其删除改为 `by_client_and_platform("codex", Codex)`。

### 前端

弹窗渲染两行且状态正确；首次打开只勾 native；勾选持久化后重开保持；勾 ZCode 时出现重启提示、只勾 CLI 时不出现；结果面板显示客户端显示名而非 `zcode_codex`；能力不可用的行禁用且显示原因；outcome 与错误中不出现 `sk-`（现有断言扩展到新目标）。

### 真机验证

自动化测试不覆盖"ZCode 真的能用这条 provider 发出请求"。实现后需：备份 `~/.zcode/v2/config.json` → 写入 → 重启 ZCode → 确认 provider 可选、发出一次请求、在 ai-switch 请求日志中看到它落到池上。此步未通过则不算完成。

### 验证命令

`pnpm typecheck`、`pnpm test:run`、`CARGO_TARGET_DIR=target-codex cargo test`、`cargo fmt --check`。按 AGENTS.md，AI 验证只使用 `src-tauri/target-codex/`。

