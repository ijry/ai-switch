# 测试模型名按账号缓存设计

## 目标

真实生成测试弹窗里的「测试模型（可选）」改为按账号各自缓存，并持久化到 localStorage。同平台的不同账号不再共用同一个模型名。

## 背景

当前该输入框的值存在 `routeTestModelsByPlatform`（`src/screens/AccountsScreen.tsx:1794`）：

```ts
const [routeTestModelsByPlatform, setRouteTestModelsByPlatform] =
  useState<Partial<Record<PlatformKey, string>>>({});
const routeTestModel = routeTestModelsByPlatform[activePlatform] ?? "";
```

按平台存一个模型名，带来两个问题：

- **同平台所有账号共用一个值。** 不同账号支持的模型不一样：给 AgentRouter 测完 `gpt-5.6-sol`，切到官方账号仍是这个值，而官方账号不支持它。
- **不持久化。** 关掉应用即丢，每次重开都要重填。

同弹窗里的另一个控件已经是持久化的：测试接口选择由 `src/lib/codexModelTestEndpoint.ts` 落盘到 localStorage。两个控件一个记得一个不记得，不一致。

## 架构

### 新建 `src/lib/modelTestModels.ts`

纯数据加纯函数，不依赖 React，与 `codexModelTestEndpoint.ts` 同构（可注入 storage、try/catch 兜底）。

```ts
export const MODEL_TEST_MODELS_STORAGE_KEY = "ai-switch.model-test-models";

export type ModelTestModelEntry = {
  model: string;
  platform: PlatformId;
};

export type ModelTestModelMap = Record<string, ModelTestModelEntry>;

export function poolModelTestKey(platform: PlatformId): string;

export function loadModelTestModels(
  storage?: Pick<Storage, "getItem">,
): ModelTestModelMap;

export function saveModelTestModel(
  key: string,
  model: string,
  platform: PlatformId,
  storage?: Pick<Storage, "getItem" | "setItem">,
): void;

export function pruneModelTestModels(
  liveAccountIds: Iterable<string>,
  platform: PlatformId,
  storage?: Pick<Storage, "getItem" | "setItem">,
): void;
```

四个导出各有单一职责：

- `poolModelTestKey` 是唯一构造池键的地方，避免 `"pool:" + platform` 散落在 UI 里拼错。返回 `pool:<platform>`。
- `loadModelTestModels` 读取并规范化。任何异常、顶层非对象、畸形条目一律降级，绝不抛给渲染。
- `saveModelTestModel` 读-改-写单条。
- `pruneModelTestModels` 惰性清理孤儿键。

### 存储形状

单个 localStorage 键 `ai-switch.model-test-models`，值为一个 JSON 对象：

```json
{
  "cred-api-1": { "model": "gpt-5.6-sol", "platform": "codex" },
  "cred-api-7": { "model": "claude-opus-4-8", "platform": "claude" },
  "pool:codex": { "model": "gpt-5", "platform": "codex" },
  "pool:claude": { "model": "claude-sonnet-4-5", "platform": "claude" }
}
```

账号用**裸 id** 作键。`route_credentials.id` 是 `TEXT PRIMARY KEY`（`src-tauri/migrations/202607130011_route_credentials.sql:4`），全局唯一，不需要平台前缀。算力池用 `pool:` 前缀，与账号 id 天然不冲突。

条目里的 `platform` **不是冗余**。它的唯一用途是让惰性清理能按平台过滤，理由见下文「惰性清理」。

选单键存一张表而非一账号一键，是因为惰性清理只有这个形状做得干净：一次读取、一次 filter、一次写回。一账号一键要遍历整个 localStorage 按前缀筛，还得小心不误伤其它功能的键。

### 修改 `src/screens/AccountsScreen.tsx`

删除 `routeTestModelsByPlatform`，换成：

```ts
const [modelTestModels, setModelTestModels] = useState<ModelTestModelMap>(
  () => loadModelTestModels(),
);
const modelTestStorageKey = modelTestAccount?.id ?? poolModelTestKey(activePlatform);
const routeTestModel = modelTestModels[modelTestStorageKey]?.model ?? "";
```

`routeTestModel` 由当前弹窗对象**派生**，不是独立 state。这样「切账号自动换值」不需要任何同步代码。

`modelTestModels` 这个内存态必须存在，不能每次渲染都调 `loadModelTestModels()`。因为落盘只发生在点「开始测试」时，用户打字的中间状态没有落盘，只能活在内存里；每次渲染重读 localStorage 会让输入框在打字过程中被旧值打回去。即：**内存态是权威，localStorage 是提交后的快照。**

### 后端零改动

模型名是纯前端的填表偏好，不属于账号配置。`CreateApiRouteCredentialInput` 与 `config_json` 均不变，Rust 侧不动，无数据库迁移。

## 数据流

### 弹窗打开

无需新增代码。`modelTestAccount` 由 `openAccountTestDialog`（`:3374`）设为该账号，或由 `openRouteTestDialog`（`:3283`）置为 `null` 表示算力池测试；`routeTestModel` 当即派生出对应缓存值。

**原样回填，不校验。** 缓存里是什么就显示什么，即使它已不在该账号当前的 datalist 选项里。输入框本来就允许自由填写（`modelTestModelOptions` 只是 `<datalist>` 建议），强行清空反而会删掉用户有意填的非列表内模型。

### 输入框改动

只写内存，不落盘：

```ts
onChange={(event) =>
  setModelTestModels((current) => ({
    ...current,
    [modelTestStorageKey]: {
      model: event.target.value,
      platform: activePlatform,
    },
  }))
}
```

存**未 trim 的原值**，让用户能正常输入空格；trim 只在提交和落盘时做。

### 点「开始测试」

在 `submitModelTest`（`:3387`）里落盘。复用现有那行 `model: routeTestModel.trim() || null` 的同一个 trim 结果，保证「存的就是测的」：

```ts
const trimmedModel = routeTestModel.trim();
saveModelTestModel(modelTestStorageKey, trimmedModel, activePlatform);
```

选择「点开始测试时才写入」而非「每次输入变化就写入」，是为了让语义是「上次测过什么」而非「上次填过什么」：随手改了又取消的值不该污染缓存。

**模型名 trim 后为空时 `saveModelTestModel` 删除该键**，而不是存空字符串。否则清空输入框会留下一条无意义的 `""`，还会让惰性清理误以为这账号有缓存。删键后下次打开回到占位符状态，与「从没测过」完全一致。

### 惰性清理

账号被删除后，它的缓存键成为孤儿。清理时机：`allCredentialsQuery`（`:2078`）成功返回后，用返回的账号 id 列表调 `pruneModelTestModels`。

这里有一个必须处理的陷阱：**该查询只返回当前平台的账号**（`queryFn: () => listRouteCredentials(activePlatform)`）。若直接把它当作「现存账号全集」去清理，切到 codex 标签页就会把所有 claude 账号的缓存全部删掉。

条目里的 `platform` 字段正是为解决这一点而存在。清理规则：

- 跳过所有 `pool:` 前缀的键——池键不对应任何账号，永不清理。
- 跳过 `entry.platform !== platform` 的键——不属于本平台，本次查询无权判断它是否还存在。
- 删除 `entry.platform === platform` 且 id 不在 `liveAccountIds` 里的键。

被替代方案是「应用启动时用全平台账号列表清理一次」，但当前没有取全平台账号的接口，新增就破坏了后端零改动，故不采用。

#### 归档账号的缓存会被清掉

`list_by_platform` 的 SQL 是 `WHERE rc.platform = ? AND rc.archived_at IS NULL`（`src-tauri/src/database/repositories/route_credential_repository.rs:651`），**归档账号不在返回列表里**。因此按上述规则，归档一个账号会连带删掉它的测试模型缓存；日后恢复该账号需要重填一次。

这是有意接受的行为，不是疏漏：

- 归档账号本来就无法测试——`openAccountTestDialog`（`:3374`）第一行就是 `if (credential.archived_at || ...) return;`。缓存在账号恢复前完全不可达。
- 要区分「已删除」与「已归档」就得让前端能拿到含归档账号的 id 列表，而当前唯一这样的来源是分页查询 `credentialsQuery`，它只返回当前页当前筛选条件下的账号，拿它做清理会误删更多。
- 代价上限是一个短字符串，恢复账号后填一次即可。

若日后觉得这一条不可接受，正确的修法是给清理单独提供一个「本平台全部未删除账号 id」的轻量接口，而不是改用分页查询的结果。

### 不迁移旧值

`routeTestModelsByPlatform` 原本只存在内存里，从来没有落盘，因此没有需要迁移的历史数据。账号首次测试时输入框为空，用户填一次即被记住。

## 错误处理

本功能无 IO、无异步，唯一的外部依赖是 `localStorage`，它在受限浏览器上下文里可能抛异常。沿用 `codexModelTestEndpoint.ts` 的兜底策略：

- `loadModelTestModels` 整体 try/catch，异常返回 `{}`。
- `saveModelTestModel`、`pruneModelTestModels` 整体 try/catch，静默失败。写入失败的后果只是「这次没记住」，不该让弹窗崩掉。

畸形数据同样宽容，降级永远是「当作没缓存」而非抛错：

| 输入 | 结果 |
|---|---|
| 非 JSON 字符串 | `{}` |
| JSON 但顶层非对象（数字、字符串、`null`） | `{}` |
| 顶层是数组 | `{}` |
| 条目非对象 | 跳过该条，其余保留 |
| 条目缺 `model` 或 `model` 非字符串 | 跳过该条 |
| 条目缺 `platform` 或 `platform` 非字符串 | 跳过该条 |

真正的校验仍归现有逻辑：`submitModelTest` 的 `routeTestModel.trim() || null`，以及后端对模型名的处理。缓存不绕过任何一条。

## 测试

### `tests/lib/modelTestModels.test.ts`（新建）

| 测试 | 断言 |
|---|---|
| 池键格式与空存储 | `poolModelTestKey("codex")` 为 `"pool:codex"`；空存储 `loadModelTestModels()` 返回 `{}` |
| 读取合法 map | 账号键与池键都被原样读出，含 `model` 与 `platform` |
| 畸形顶层降级 | 非 JSON、数字、`null`、数组四种输入均返回 `{}` |
| 畸形条目跳过 | 条目非对象、缺 `model`、`model` 非字符串、缺 `platform` 四种情况各自被跳过，同一 map 里的合法条目仍返回 |
| 写入保留其余条目 | 写 `cred-a` 不影响已存在的 `cred-b` 与 `pool:codex` |
| 空白模型名删键 | 对已有键写入 `"   "` 后，该键从 map 中消失，其余键保留 |
| prune 删本平台孤儿 | `platform` 为 codex 且 id 不在存活列表里的键被删 |
| prune 的三条豁免 | 其它平台的账号键、全部 `pool:` 键、存活 id 的键均保留 |
| storage 抛异常 | `load` 返回 `{}` 不抛；`save`、`prune` 均不抛 |

### `tests/AccountsScreen.test.tsx`（追加）

| 测试 | 断言 |
|---|---|
| 从 localStorage 回填 | 预置 `cred-api-1` 的缓存后打开该账号弹窗，输入框显示缓存值 |
| 打字不落盘、提交才落盘 | 输入后 localStorage 仍无该键；点「开始测试」后键出现且值为 trim 后的模型名 |
| 账号键与池键互不干扰 | 单账号测试写入的值不影响算力池弹窗显示的值，反之亦然 |
| 清空输入框删键 | 对已有缓存的账号清空输入框并提交，该键从 localStorage 中消失 |
| 孤儿缓存被清理 | 预置一个不存在账号 id 的 codex 缓存键，渲染后该键消失；同时预置的 claude 账号键与 `pool:` 键仍在 |

`beforeEach` 已有 `window.localStorage.clear()`（`tests/AccountsScreen.test.tsx:252`），无需额外清理。

### 必须重写的既有测试

`tests/AccountsScreen.test.tsx:2658` 的 `keeps the optional test model separately for each agent tab` 在新设计下**会真实失败**，不是碰巧变绿。

原因：该测试 `view.rerender` 切到 claude 后断言输入框为空，但 `listRouteCredentials` 被 mock 成无论传什么平台都返回同一个 `credentialsFixture`（`:378`），所以 claude 标签页里出现的仍是 `id: "cred-api-1"` 那个账号。新设计按账号 id 取值，键没变，值仍是 `gpt-4o`，`toHaveValue("")` 失败。

**这个失败是正确的。** 同一个账号 id 就该共享同一份缓存。测试原本的前提（切平台等于换账号）是 mock 的产物；生产环境里 `listRouteCredentials(platform)` 只返回该平台的账号，claude 标签页不会出现 codex 的 `cred-api-1`。

改写为 `keeps the optional test model separately for each account`：在同平台的两个不同账号（`cred-official-1` 与 `cred-api-1`）之间切换，各自记住各自的模型名。

跨平台隔离改由**池级**用例覆盖——`pool:codex` 与 `pool:claude` 是天然不同的键，那才是真正按平台区分的东西。

### 回归检查

`pnpm test:run`、`pnpm typecheck`。后端零改动，仍运行 `pnpm rust:check` 确认无意外牵连。

## 不受影响的既有行为

- `modelTestModelOptions`（`:2376`）的 datalist 选项计算逻辑不变，它本来就已按账号隔离。
- 测试接口选择（`codexModelTestEndpoint`）的持久化不变。
- `submitModelTest` 发给后端的 `model` 字段取值方式不变，仍是 `routeTestModel.trim() || null`。
- 进行中提示条（`:3914`）读 `routeTestModel` 的方式不变。
- 复制 curl（`:3309`）读 `routeTestModel` 的方式不变。

## 完成判定

- 同平台的两个账号各自记住各自的测试模型名，互不覆盖。
- 关闭并重开应用后，账号的测试模型名仍在。
- 算力池测试的模型名按平台单独保存，不与任何单账号缓存串味。
- 打字过程不落盘，点「开始测试」后才落盘，且落盘值与实际发送值一致。
- 清空输入框并提交后该缓存被删除，下次打开显示占位符。
- 删除或归档账号后，切回该平台标签页时其孤儿缓存被清理，其它平台的缓存不受影响。
- localStorage 不可用或内容畸形时，弹窗照常工作，输入框视作无缓存。
- `src-tauri/` 无改动；`pnpm test:run`、`pnpm typecheck`、`pnpm rust:check` 全部通过。

## 决策记录

| 决策 | 选择 | 理由 |
|---|---|---|
| 存储位置 | localStorage，按账号 id 存 | 跨会话保留；纯前端改动，不碰 Rust 与数据库；「上次测试用的模型」不属于账号配置 |
| 存储形状 | 单键存一张表 | 惰性清理只需一次读-filter-写；一账号一键要遍历全部 localStorage 键 |
| 条目是否带 `platform` | 带 | 账号列表按平台分页返回，不带 platform 就无法区分「已删除」与「属于别的平台」，清理必然误删 |
| 算力池 | 按平台单独存 `pool:<platform>` | 池测试跨账号，没有单一账号 id 可归属；与账号 id 天然不冲突 |
| 写入时机 | 点「开始测试」时 | 语义为「上次测过什么」；随手改了又取消的值不污染缓存 |
| 空模型名 | 删键而非存空串 | 避免无意义条目，且不让清理误判「有缓存」 |
| 回填校验 | 原样回填，不校验 | datalist 只是建议，输入框允许自由填写；强行清空会删掉用户有意填的非列表模型 |
| 归档账号的缓存 | 一并清掉，接受重填 | 归档账号无法测试，缓存不可达；要保住它就得引入含归档账号的 id 列表来源，代价大于一个短字符串 |
| 旧值迁移 | 不迁移 | 原状态从未落盘，没有历史数据；且迁移等于把「共用」又带回来一轮 |
