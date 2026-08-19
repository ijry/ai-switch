# 账号预设（服务商预设）设计

## 目标

在"新增账号"表单顶部提供预设下拉。选中一条预设即自动填好 Base URL、接口格式、模型映射和账号名称，用户通常只需再填 API Key。首批只收录 codex 平台下的 AgentRouter 两条线路。

## 背景

当前新增 API 账号需要用户逐个手填 Base URL、接口格式和模型映射（`src/screens/AccountsScreen.tsx:5281-5447`）。这些值对特定服务商是固定的，用户得去翻服务商文档，填错要到实际请求失败才发现。

表单里已有一个同类控件可供参照：User-Agent 预设，由纯数据模块 `src/lib/accountUserAgent.ts` 加屏幕内小组件 `UserAgentFields`（`AccountsScreen.tsx:1556-1609`）实现。本设计沿用同一分工。

## 首批预设数据

两条预设，都属于 codex 平台，接口格式均为 OpenAI Chat Completions（`interface_format: "openai"`），模型映射均为单条 `gpt-5.6-sol -> gpt-5.6-sol`。

| id | 标签 | 默认账号名 | Base URL |
|---|---|---|---|
| `agentrouter-primary` | `AgentRouter (agentrouter.org)` | `AgentRouter` | `https://agentrouter.org/v1` |
| `agentrouter-backup` | `AgentRouter (ps.air-outer.com)` | `AgentRouter 备用` | `https://ps.air-outer.com/v1` |

两条 `defaultName` 必须不同，否则同时添加两条线路会在账号列表里产生两个同名账号，难以分辨。

`gpt-5.6-sol` 是 codex 已有的基线模型 id，见 `src/components/accounts/ModelMappingSummary.tsx:11-16`。

## 架构

### 新建 `src/lib/accountPresets.ts`

纯数据加纯函数，不依赖 React。

```ts
export type AccountPreset = {
  id: string;
  platform: PlatformId;
  label: string;
  defaultName: string;
  baseUrl: string;
  interfaceFormat: InterfaceFormat;
  modelMappings: ModelMapping[];
};

export const ACCOUNT_PRESETS: AccountPreset[];
export function presetsForPlatform(platform: PlatformId): AccountPreset[];
export function matchPresetByBaseUrl(platform: PlatformId, baseUrl: string): AccountPreset | null;
```

三个导出各有单一职责：

- `ACCOUNT_PRESETS` 是唯一扩展点。新增服务商只需往数组追加一条，不碰 UI。
- `presetsForPlatform` 决定是否渲染下拉以及列出哪些条目。
- `matchPresetByBaseUrl` 实现反向匹配，把 Base URL 映射回预设。

**不提供 `applyPreset` 函数。** 套用预设需要写 5 个分散的 `useState`（name、baseUrl、interfaceFormat、mappings、fetchedModels），纯函数无法触及。要包装就得发明一个当前不存在的表单状态对象，或者把 5 个 setter 传进去，两者都比在组件的 `onChange` 里直接写 5 行赋值更绕。因此套用逻辑留在 UI 层，lib 只负责数据与判定。

### 修改 `src/screens/AccountsScreen.tsx`

- 新增 `PresetFields` 组件（约 30 行），紧邻 `UserAgentFields`（`:1556`）放置，只负责渲染 select 与提示条。
- 预设下拉插在"账号名称"字段之前（`:5283` 之前），即 API 表单顶部。它必须位于 `createMode === "api"` 分支内部（该分支始于 `:5281`），因为"批量导入"模式没有 Base URL 与模型映射字段，预设在那里无意义。
- 套用逻辑写在下拉的 `onChange` 中，直接调用现有 setter。
- **不新增任何 state。** 下拉选中项由 `matchPresetByBaseUrl(activePlatform, apiBaseUrl)` 在渲染时派生。

零新增 state 带来两个免费行为：改 Base URL 后下拉自动回到"自定义"，无需同步代码；平台切换时表单重置（`:2194-2208`）自动带走预设选中态，同样无需额外代码。

### 后端零改动

AgentRouter 的三样配置全部落在已有字段上：`config_json.base_url`、`config_json.interface_format`、`config_json.model_mappings`。预设纯粹是前端填表助手，`CreateApiRouteCredentialInput` 不变，Rust 侧不动，无数据库迁移。

由预设创建的账号与手工填写创建的账号在存储上完全一致，因此预设功能日后修改或移除都不影响已创建的账号。

### 作用范围：仅新增表单

编辑抽屉（`:5579-6090`）不加预设下拉。编辑时账号已有确定的 Base URL，再提供预设入口容易误触改坏线上账号。

## 数据流

### 套用预设

用户从下拉选中一条预设时，依次执行：

```
setApiBaseUrl(preset.baseUrl)
setApiInterfaceFormat(preset.interfaceFormat)
setApiMappings(preset.modelMappings)          // 覆盖已有映射
setApiName(preset.defaultName)                // 仅当 apiName.trim() 为空
setApiFetchedModels([])                       // 上游已变，缓存失效
setApiFetchModelsError(null)
setApiMappingsError(null)
```

**模型映射采取覆盖而非合并。** 预设的意义是一次装好某服务商的正确配置。Base URL 已被整体替换，此前的映射是为旧上游写的，保留下来大概率错误，而这类错误要到实际请求失败才暴露。

**账号名称仅在为空时填入**，避免覆盖用户已输入的名字。

### 选择"自定义"

下拉列表的首项恒为"自定义"，其后才是该平台的各条预设。未匹配到任何预设时，下拉显示的就是这一项。

选中"自定义"不做任何修改，保持当前值原样，与 `UserAgentFields` 中 `custom` 分支的既有处理一致（`:1582-1585`）。它不是清空表单的入口，只表示当前配置不对应任何预设。

### 反向匹配

```
apiBaseUrl → matchPresetByBaseUrl(activePlatform, apiBaseUrl) → 选中项 | null（显示"自定义"）
```

匹配规则：对两侧 Base URL 同样做 trim、去尾部斜杠、转小写后要求**完全相等**，使 `  https://AgentRouter.org/v1/  ` 仍能识别为主线路。不做前缀匹配，因此 `https://agentrouter.org/v2` 或 `https://agentrouter.org` 都算未命中——它们是不同的上游端点，认成主线路会让用户以为配置已就绪。

**只比较 Base URL，不比较模型映射。** 用户增删映射行不会让下拉跳回"自定义"，因为账号确实仍属于该服务商，只是映射被自定义了。

### 非 codex 平台

`presetsForPlatform(platform)` 返回空数组时整个下拉不渲染。这与表单既有惯例一致：`shouldShowInterfaceFormatSelect`（`:623`）在只有一个可选值时同样不渲染接口格式下拉。空控件对用户没有信息量。

预设不改变 `platform`。平台由左侧导航决定，不应有第二个入口修改它。

## 提示条

仅在匹配到预设时显示，文案：

```
已套用 AgentRouter 预设，通常只需填写 API Key。
```

"通常"一词是有意保留的。账号名称虽会自动填入，但用户可能改名或清空；若写死"只需填写 API Key"，在名称被清空、保存被 `:2616` 拦下时该提示即为假话。

## 错误处理

本功能无 IO、无异步、无校验失败路径，只做本地状态赋值。唯一的失败模式是 select 的值找不到对应预设（理论上不会发生），按 `UserAgentFields:1579-1581` 的先例直接 `return`。

真正的校验仍由现有逻辑负责，预设不绕过任何一条：

- `normalizeModelMappings`（`:797-829`）校验模型映射
- 保存前的账号名称非空检查（`:2616`）与至少一个 API Key 检查（`:2620`）

## 不受影响的既有行为

- 用户手改 Base URL 时的清缓存逻辑（`:5345-5349`）原样保留。
- 批量 API Key（一行一个）照旧工作，多个 Key 共用同一份预设配置。
- 深链导入与账号导入（`RouteCredentialImportDialog.tsx`、`DeepLinkImportDialog.tsx`）不涉及预设。

## 测试

### `tests/accountPresets.test.ts`（新建）

| 测试 | 断言 |
|---|---|
| `presetsForPlatform("codex")` | 返回两条 AgentRouter，域名分别为 `agentrouter.org` 与 `ps.air-outer.com` |
| `presetsForPlatform("claude")`、`("gemini")` | 返回 `[]` |
| 预设内容 | 主线路 `baseUrl` 正确、`interfaceFormat` 为 `"openai"`、`modelMappings` 恰为 `[{ from: "gpt-5.6-sol", to: "gpt-5.6-sol" }]` |
| `defaultName` 唯一 | 两条预设的 `defaultName` 不相等 |
| `matchPresetByBaseUrl` 命中 | 精确、带尾斜杠、大小写混写、前后带空格四种写法均匹配到主线路 |
| `matchPresetByBaseUrl` 未命中 | 陌生 URL 与空串返回 `null`；`https://agentrouter.org/v2` 与 `https://agentrouter.org`（前缀相近但端点不同）返回 `null`；Base URL 正确但平台为 `claude` 时返回 `null` |

### `tests/AccountsScreen.test.tsx`（追加）

| 测试 | 断言 |
|---|---|
| codex 下选中预设 | Base URL、接口格式、账号名称三个输入框均变为预设值；模型映射出现 `gpt-5.6-sol` |
| 名称不覆盖 | 先手填 `我的账号` 再选预设，名称仍为 `我的账号`，Base URL 已更新 |
| 覆盖模型映射 | 先手加一行 `foo -> bar` 再选预设，`foo` 消失，只剩 `gpt-5.6-sol` |
| 改 Base URL 回到自定义 | 选预设后改写 Base URL，下拉变回"自定义"，提示条消失 |
| 端到端保存 | 只选预设加只填 API Key，`createApiRouteCredential` 收到预设的 `base_url`、`interface_format: "openai"` 与 `model_mappings_json` |
| claude 平台不渲染 | `platform="claude"` 时查不到预设下拉 |

端到端保存这条最有价值：它验证只填一个 API Key，落库的账号配置即完整正确。

### 测试写法

沿用现有惯例，按中文字面量查询（`getByLabelText("Base URL")`、`getByLabelText("接口格式")`），不引入 i18n——`AccountsScreen.tsx` 全文硬编码中文，现有测试也都如此断言。预设下拉的 `aria-label` 定为 `创建 账号预设`，跟随 `UserAgentFields` 的 `${idPrefix} User-Agent 预设` 命名法（`:1575`）。

### 回归检查

`pnpm test:run`、`pnpm typecheck`。后端零改动，仍运行 `pnpm rust:check` 确认无意外牵连。

## 完成判定

- codex 新增账号表单顶部出现预设下拉，含两条 AgentRouter 线路。
- 选中预设后 Base URL、接口格式、模型映射、账号名称（原为空时）自动填好，并显示提示条。
- 已手填的账号名称不被预设覆盖。
- 选中预设会覆盖已有模型映射并清空已获取的模型列表。
- 改写 Base URL 后下拉回到"自定义"，提示条消失。
- claude、gemini 等平台不渲染预设下拉。
- 仅选预设加填一个 API Key 即可成功创建账号，落库配置与手工填写一致。
- 新增纯函数测试与账号界面测试通过，`pnpm typecheck` 与 `pnpm rust:check` 通过。

## 决策记录

| 决策 | 选择 | 理由 |
|---|---|---|
| 两个域名如何呈现 | 两条独立预设条目 | 一次点击完成，无二级控件；扩展只需追加数组条目 |
| 预设是状态还是动作 | 状态，按 Base URL 反向匹配 | 与既有 `matchUserAgentPreset` 一致；Base URL 天然唯一标识服务商加线路 |
| 账号名称 | 为空时填入预设默认名 | 让"只需填 API Key"成立，且预设行为对用户可见，而非隐式兜底 |
| 非 codex 平台 | 不渲染下拉 | 与 `shouldShowInterfaceFormatSelect` 惯例一致；空控件无信息量 |
| 模型映射 | 覆盖 | Base URL 已整体替换，旧映射大概率错误，合并会留下指向不存在模型的行 |
| 代码落位 | lib 纯函数加屏幕内小组件 | 复用 `accountUserAgent.ts` 加 `UserAgentFields` 的既有分工；纯函数可脱离 React 单测 |
