# 账号桌面工作区与可迁移导入导出设计

## 状态

本设计已在 2026-08-04 的对话中确认。范围只包含各 Agent 平台对应的右侧账号与算力池页面，不改变左侧导航和其他功能页。

## 问题

当前账号页采用多个大卡片纵向堆叠，标题、算力池、反馈、筛选、列表和分页共同参与页面滚动。结果是：

- 工具栏和算力池操作会被滚出屏幕；
- 单个账号占用高度过大，一屏只能看到少量账号；
- 列表操作按钮带完整文字，横向空间利用率低；
- 算力池的重要状态和次要详情混在同一个大区域；
- 已有跨页选择不能直接导出，也不能用同一文件在另一台电脑批量恢复官方账号和 API 账号。

CLIProxyAPI（下称 CPA）支持官方 OAuth/Auth-File 账号，也支持 `gemini-api-key`、`claude-api-key`、`codex-api-key`、`xai-api-key` 和 `openai-compatibility` 等 API 配置；上游还定义了 `interactions-api-key` 与 `vertex-api-key`，但本期不接收后两者。CPA 原生存储分为 Auth-File JSON 与配置文件段，没有官方的“全部账号统一数组”格式。AI Switch 因此定义一个不加外层包装的迁移数组：官方项采用 CPA Auth-File 字段，API 项采用对应 CPA 配置段 entry 的字段投影，并以 `x-ai-switch` 元数据完成数组分类、来源幂等和 AI Switch 字段恢复。该数组不是 CPA 可直接读取的单文件。

## 目标

1. 将右侧账号页改为经典桌面客户端结构：固定顶部工具栏、固定紧凑算力池条、中间独立滚动内容、固定底部状态栏。
2. 将默认账号行压缩至 `48–54px`，让常用桌面高度下明显增加可见账号数。
3. 所有常用操作无需展开即可执行；次要信息按需展开。
4. 行尾操作只显示统一图标，通过 Tooltip、`aria-label` 和焦点状态解释含义。
5. 使用现代 macOS 风格的薄边框、平面填充和轻阴影，不使用复古凸起、双线或厚重描边。
6. 支持跨页勾选账号后导出可复制的 AI Switch 迁移 JSON、保存 `.json` 文件，以及为 API 账号生成 `aiswitch://` 导入链接。
7. 导出的文件必须是裸 JSON 数组：`[{...}, {...}]`，不增加顶层包装对象。该数组是 AI Switch 的迁移文件，不是 CPA 原生可直接读取的单文件。
8. 迁移格式允许官方账号、API 账号以及多个平台混合，并可在另一台电脑中直接预览后批量导入；本期账号页入口只导出当前平台和当前分段中的跨页选择，筛选变化不清除已选 ID。
9. 每项使用命名空间为 `x-ai-switch` 的最小迁移元数据；它恢复本规范定义的 AI Switch 字段，而不是承诺所有内部字段无损恢复。导入默认不恢复原算力池成员状态。

## 非目标

- 不重新设计左侧导航、设置页、会话页或其他页面。
- 不改为无限滚动或虚拟列表；继续使用现有后端分页和跨页排序。
- 不将整份裸数组宣称为 CPA 原生完整备份文件。CPA 外部使用时，官方账号仍需按元素拆成 Auth-File，API 项仍需聚合到对应配置段。
- 不把 AI Switch 导入链接当作完整备份。链接只表达当前 `aiswitch://v1/import` 能表达的 API 字段，迁移 JSON 才是完整迁移载体。
- 不按邮箱、显示名称或账号 ID 静默覆盖已有账号。
- 不恢复旧账号的运行状态、额度、冷却、失败次数或请求统计。

## 页面架构

### 外层滚动职责

`AppLayout` 目前让整个右侧内容容器 `overflow-y-auto`。账号页需要改为例外：

- Agent 账号页面的右侧容器使用 `overflow-hidden`；
- `AccountsScreen` 根节点使用 `h-full min-h-0 flex flex-col overflow-hidden`；
- 其他页面继续沿用现有整页滚动行为；
- 只有账号页的中间内容区使用 `overflow-y-auto`；统计分段也复用该区域，不恢复整页滚动。

该变化必须通过 `activeScreen` 或明确的布局属性定向应用，不能让其他页面失去滚动能力。

### 垂直区域

右侧工作区自上而下分为四层：

1. `44px` 顶部工具栏；
2. `30px` 算力池状态条；
3. `minmax(0, 1fr)` 中间内容区；
4. `32px` 底部状态栏。

四层都位于同一个现代化面板内。顶部、算力池条和底部不参与内容滚动。弹窗、Popover 与抽屉使用覆盖层，不通过增加页面高度展示。

## 顶部工具栏

### 普通模式

左侧显示当前平台名称、页面标题和紧凑的平台支持状态。标题不再占用两行大标题区域。

右侧始终显示以下 `28–30px` 图标按钮：

- 新增账号；
- 批量迁移导入；
- 会话管理；
- 批次筛选；
- 刷新账号列表；
- 刷新官方账号额度。

筛选按钮用角标显示已启用的筛选数。筛选内容放在 Popover 中，继续使用现有批次选项和“单账号”选项；选择筛选后页码重置为第一页。

### 选择模式

只要 `selectedAccountIds` 非空，工具栏右侧切换为批量操作组：

- 当前分段对应的加入或移出算力池；
- 导出所选账号；
- 删除所选账号；
- 清除选择。

所有按钮仍为图标按钮。选中数量同时显示在底部状态栏中。选择保持当前平台与当前分段内的 ID 语义：切换平台或切换任一分段时清空选择；翻页、修改筛选和修改页大小时保留选择。筛选变化后，当前未显示但仍被选中的数量必须继续可见，导出和其他批量操作仍作用于这些 ID。

异步操作期间只禁用会造成重复提交的按钮，并在原图标位置显示旋转或进度状态，不改变按钮尺寸。

## 紧凑算力池条

算力池不再使用渐变大卡片。固定条采用单行摘要：

- 左侧：运行状态点、`算力池`、成员总数；
- 中间：`可用`、`冷却`、`异常`数量；
- 右侧：启动或停止代理、写入配置、生成测试、展开详情四个图标按钮。

状态数量应基于完整池成员计算，不能用当前分页可见账号推断。三个健康计数互斥且总和等于成员总数：先将禁用、认证失效或需要人工处理的成员计为“异常”，再将剩余具有有效冷却时间的成员计为“冷却”，其余已启用成员计为“可用”。本期在现有 `RoutePoolState` 中增加向后兼容的健康摘要字段。

展开按钮打开锚定在状态条下方的 Popover，不改变固定条高度。Popover 内展示：

- 本地代理地址；
- 最近路由到的账号；
- 最近一次配置写入结果；
- 最近一次测试结果与请求链路；
- 需要诊断时才展开的请求/响应 JSON。

关闭 Popover 后，启动/停止、写配置和测试按钮仍始终可用。操作成功或失败的简短反馈进入底部状态栏；详细结果保留在 Popover 中。

## 中间账号列表

### 列表头

账号分段的中间区域顶部保留一个 `30–32px` 的紧凑列表头，可使用 sticky 定位。它包含：

- 当前页全选复选框；
- 名称/状态等列提示；
- 当前筛选摘要；
- 当前分段总账号数。

全选只选择当前页可见账号，不自动选择所有匹配页。已在其他页选中的 ID 不受影响。

### 默认账号行

默认行高目标为 `48–54px`，结构为：

- 拖动手柄；
- 选择复选框；
- 账号主信息；
- 一行紧凑状态摘要；
- 行尾图标操作组；
- 详情展开图标。

主信息优先显示账号名称。账号类型、状态和批次不再全部使用大号胶囊标签：

- 状态使用小圆点加短文字；
- 账号类型使用短标签或图标；
- 批次名截断显示；
- 冷却、订阅、重置时间和详细额度进入展开区；
- 默认第二行只保留最有用的一条摘要，例如请求成功率或官方额度摘要。

### 始终可见的行操作

行尾操作不需要展开即可使用，并只显示 Lucide 图标：

- 当前分段对应的加入或移出算力池；
- 官方账号额度刷新；
- 复制账号；
- 连接测试；
- 编辑账号。

不适用于当前账号类型的按钮不渲染。能力不可用时按钮保持位置但禁用，并通过 Tooltip 说明原因。图标状态变化不能造成布局位移，例如复制完成只把 `Copy` 换为 `Check`。

### 展开详情

展开区只承载次要信息，不承载常用操作。内容包括：

- 邮箱、内部类型和批次完整名称；
- 完整状态、冷却时间与最近失败原因；
- 订阅类型、主额度、周额度和重置时间；
- 请求数、成功数、失败数和成功率；
- API 账号的 Base URL、接口格式、模型映射摘要；
- 其他只读诊断信息。

同一时间可以展开一个账号，避免列表高度无限增长。展开或收起不改变选择和拖动状态。

### 加载、空状态与错误

- 加载时使用与真实行同高的骨架行；
- 空状态占据中间可滚动区域中心，不再使用大虚线卡片；
- 可恢复错误显示在列表顶部的紧凑横幅，并提供图标重试；
- 状态栏保留最近一条简短操作反馈，`aria-live` 宣读异步结果。

## 统计分段

选择“统计”后，中间区域显示现有统计指标和请求记录，仍然只有中间区域滚动。统计周期选择器放在中间区域顶部的紧凑 sticky 工具行。

底部状态栏右侧在统计分段切换为请求记录的页大小和分页操作。请求详情继续按需展开；大段请求/响应数据不能把顶部工具栏或底部状态栏滚出屏幕。

## 底部状态栏

### 左侧

放置固定分段器：

- 已入池；
- 未入池；
- 统计。

分段器使用 macOS 风格的平面选中态和细边框，不使用大胶囊按钮。现有分段语义、默认值和后端池内/池外分页保持不变。

### 中间

按优先级显示一条短状态：

- 已选账号数量；
- 当前分段账号总数；
- 刷新、入池、导出、导入或删除的最近反馈；
- 加载或错误状态。

长错误只显示摘要和“详情”入口，避免撑高状态栏。

### 右侧

账号分段显示：

- 每页 `20 / 50 / 100`；
- 上一页图标；
- `当前页 / 总页数`；
- 下一页图标。

统计分段显示对应请求记录分页。分页按钮只使用图标，保留 Tooltip、`aria-label` 和禁用状态。

## 视觉与交互规范

- 面板和按钮使用 `1px` 中性薄边框；
- 常规圆角为 `6–8px`，弹窗可使用 `10–12px`；
- 不使用复古 bevel、内外双阴影、厚描边或高亮边缘；
- 主操作使用低饱和实色或浅色选中态，不使用大面积渐变；
- 阴影只用于工作区、Popover 和弹窗的层级区分；
- 图标统一使用 Lucide，常规尺寸 `14–16px`；
- 所有图标按钮在 hover 与键盘 focus 时显示文字 Tooltip；
- 所有操作保留稳定的 `aria-label` 和可见 `focus-visible` 焦点环；
- hover 只改变颜色、边框或背景，不使用会导致布局位移的缩放；
- 拖动手柄继续支持键盘进入移动模式、方向键移动和 `Escape` 取消；
- 遵循 `prefers-reduced-motion`，仅保留必要的短过渡。

## 可迁移 JSON 格式

### 格式定位

导出名称为“AI Switch 迁移 JSON（CPA 字段投影）”。复制和保存文件必须使用同一个后端序列化结果，规则如下：

- 顶层必须是 JSON 数组，不允许 `{ "accounts": [...] }` 或其他包装；
- 每个数组元素代表一个账号；
- 文件使用 UTF-8、两空格缩进和末尾换行；
- 迁移文件允许混合官方账号、API 账号和不同平台；账号页入口本期只提交单一平台、单一分段中的 ID 集合，筛选只影响可见列表，不限制已保持的跨页选择；
- 序列化顺序稳定：单平台按 `sort_order ASC, created_at DESC, id ASC`；内部服务处理混合平台请求时先按 `PlatformId::ALL` 的固定平台顺序，再按同一平台内顺序；
- 导出请求中的重复账号 ID 在后端去重；
- `x-ai-switch` 的最小分类字段始终保留；用户开关只控制显示名、批次、来源、池状态和 AI Switch 专属 API 字段等增强恢复信息，不能移除格式/版本/平台/类型字段。

示例：

```json
[
  {
    "type": "codex",
    "email": "team@example.com",
    "access_token": "eyJ...",
    "refresh_token": "rt_...",
    "account_id": "acct_123",
    "x-ai-switch": {
      "format": "ai-switch.route-credential",
      "schema_version": 1,
      "source_instance_id": "7f0d9f15-8d42-4a36-9f06-75cf1b3c4a10",
      "source_credential_id": "credential-official-1",
      "platform": "codex",
      "kind": "official",
      "display_name": "Team Account",
      "source_batch_id": "batch-1",
      "batch_name": "Team A",
      "in_pool": true,
      "origin_format": "cpa"
    }
  },
  {
    "api-key": "sk-...",
    "base-url": "https://api.example.com/v1",
    "models": [
      {
        "name": "provider-codex",
        "alias": "gpt-5",
        "display-name": "Codex"
      }
    ],
    "x-ai-switch": {
      "format": "ai-switch.route-credential",
      "schema_version": 1,
      "source_instance_id": "7f0d9f15-8d42-4a36-9f06-75cf1b3c4a10",
      "source_credential_id": "credential-api-1",
      "platform": "codex",
      "kind": "api",
      "cpa_section": "codex-api-key",
      "display_name": "Codex Relay",
      "in_pool": false,
      "interface_format": "openai-responses",
      "responses_custom_tool_compat": true,
      "model_mappings": [
        {
          "from": "gpt-5",
          "to": "provider-codex",
          "label": "Codex"
        }
      ]
    }
  }
]
```

### `x-ai-switch` 元数据

元数据是 AI Switch namespaced vendor extension。每项都必须包含最小核心；“包含增强迁移信息”开关只控制标为可选的字段。版本 `1` 支持：

```text
format: 必须为 ai-switch.route-credential
schema_version: 必须为受支持的 major 版本 1
platform: 必须为 AI Switch 平台 ID
kind: 必须为 official 或 api
cpa_section: API 项必须提供，表示 CPA 投影目标；官方项不得提供
source_instance_id: 导出器始终写入来源安装的稳定 UUID；导入器为兼容旧文件允许缺失
source_credential_id: 导出器始终写入来源电脑上的账号 ID；导入器为兼容旧文件允许缺失，且只有与 source_instance_id、platform、kind 一起才构成全局来源身份
display_name: 可选，账号显示名称
source_batch_id: 可选，来源批次 ID
batch_name: 可选，恢复批次显示名称
in_pool: 可选，导出时是否在算力池中
origin_format: 可选，cpa、sub2api 或 ai-switch
interface_format: API 账号可选
responses_custom_tool_compat: API 账号可选
api_key_field: Claude API 账号可选
model_mappings: API 账号可选，保留 AI Switch 原始模型映射、label 与 supports_1m
```

元数据与顶层载荷对同一语义字段发生冲突时，该项为 fatal error，不能选择其中一方继续导入。不支持的 major `schema_version` 阻止该项导入；同一 major 下未知可选字段可以忽略并产生 warning。元数据不能包含额度、运行状态、冷却、失败历史、请求统计或本机文件路径。

### 官方账号项

官方账号项使用 CPA Auth-File 的扁平对象形式。核心字段包括：

- `type`；
- `email`；
- `id_token`、`access_token`、`refresh_token`、`account_id`；
- `last_refresh`、`expired`、`expires_in`、`disabled`、`token_type`；
- `base_url`、`token_endpoint`、`auth_kind`、`sub`、`redirect_uri`、`client_id`；
- `headers`；
- Agent Identity 所需的 `workspace_id`、`chatgpt_account_id`、`agent_runtime_id`、`task_id`、`agent_private_key`、`auth_mode` 和 `chatgpt_account_is_fedramp`。

生成官方账号项时不能直接返回 `config_json.raw`，因为 OAuth 刷新只会更新当前规范化 secret/config，原始 raw 中可能保留过期令牌。生成算法必须：

1. 将可信的原始 CPA 对象作为未知字段模板；来源对象只有在可信 CPA/Auth-File 标记存在时才能提供未知字段；
2. 将 Sub2API 或 wrapper 中的 `credentials`、`tokens` 提升为顶层字段；
3. 只用当前 `secret_payload_json` 的显式 secret allowlist 覆盖模板：OAuth 使用 `id_token`、`access_token`、`refresh_token`、`account_id`；Agent Identity 额外允许本节列出的私钥、runtime、task 和账号标识字段；
4. 只用当前 `config_json` 的显式认证配置 allowlist 覆盖模板：本节列出的 token 元数据、认证端点、模式、账号标识和受信 headers；
5. 当前字段为空时删除模板中的旧值和 camelCase 别名，防止旧令牌复活；
6. 使用当前邮箱和可信平台类型覆盖 raw；Grok 的 CPA 类型优先保留可信 `xai`，否则规范化为 `xai`；
7. 移除 `raw`、`raw_type`、`import_format`、预览、数据库 ID、状态、排序、额度、冷却和统计等内部字段；
8. 最后写入 `x-ai-switch` 最小核心，并按用户开关写入或移除增强恢复字段。

普通 OAuth 项至少需要 `access_token` 或 `refresh_token`。Agent Identity 项必须具备运行所需的私钥、runtime、task 和账号标识；历史残缺数据在导出预览中列为错误，不能静默生成不可用凭据。`secret_payload_json` 出现 allowlist 之外的未知非空字段时必须阻止该项导出并给出安全错误码，不能猜测该字段是否应泄露；API 的 secret/config 也只允许本节明确字段，未知 secret 字段阻止导出，未知非 secret API 字段可忽略并产生 warning；官方 raw 模板中的未知顶层字段只有在来源被标记为可信 CPA/Auth-File 时才允许原样保留。

### API 账号项

API 账号使用 CPA 配置段 entry 的字段投影，目标配置段记录在 `x-ai-switch.cpa_section`，不把 AI Switch 自定义 discriminator 混入 CPA entry 顶层。投影必须根据 `platform + interface_format + endpoint` 按以下优先级决定，不能只根据当前页面平台猜测：

| AI Switch 方言 | CPA 投影目标 |
| --- | --- |
| `anthropic` 或 `anthropic-messages` | `claude-api-key` |
| `gemini` | `gemini-api-key` |
| `openai-responses` | `codex-api-key` |
| `openai` 且平台为 Grok、Base URL 明确认定为 xAI 官方端点 | `xai-api-key` |
| 其他 `openai` 兼容端点 | `openai-compatibility` |

不支持或彼此矛盾的方言/平台/endpoint 组合作为该项 fatal error，不能强制转换。CPA 的 `interactions-api-key`、`vertex-api-key` 在 v1 中明确不支持；CPA 顶层 `api-keys` 是访问 CPA 服务本身的客户端鉴权 key，不识别为上游 API 账号。

`claude-api-key`、`gemini-api-key`、`codex-api-key` 和 `xai-api-key` 的单项结构为：

```json
{
  "api-key": "sk-ant-...",
  "base-url": "https://api.example.com",
  "headers": {
    "User-Agent": "custom-client/1.0"
  },
  "models": [
    {
      "name": "provider-sonnet",
      "alias": "claude-sonnet-5",
      "display-name": "Sonnet",
      "max-context-length": 1048576
    }
  ]
}
```

`openai-compatibility` 保持 CPA provider 结构，每个导出账号只包含一个 key entry：

```json
{
  "name": "OpenRouter",
  "base-url": "https://openrouter.ai/api/v1",
  "headers": {},
  "api-key-entries": [
    {
      "api-key": "sk-or-..."
    }
  ],
  "models": [
    {
      "name": "moonshotai/kimi-k2",
      "alias": "kimi-k2"
    }
  ]
}
```

AI Switch 模型映射转换规则为：

- `to` -> CPA `name`；
- `from` -> CPA `alias`；
- `label` -> CPA `display-name`；
- `supports_1m: true` -> CPA `max-context-length: 1048576`；
- 无值字段不输出。

反向导入的默认方言为：`claude-api-key -> anthropic`、`gemini-api-key -> gemini`、`codex-api-key -> openai-responses`、`xai-api-key -> openai`、`openai-compatibility -> openai`。`x-ai-switch.interface_format` 只有与 `cpa_section` 兼容时才恢复；冲突为 fatal error。模型反向映射为 `name -> to`、`alias -> from`、`display-name -> label`，`max-context-length >= 1048576 -> supports_1m: true`。Agent Identity 继续使用官方账号专用解析路径，不能按普通 OAuth token 规则降级。

`responses_custom_tool_compat` 和 Claude `api_key_field` 没有完全等价的 CPA 字段，因此只保存在增强 `x-ai-switch` 元数据中。关闭增强信息时，导出预览必须明确提示这些字段不会随 CPA 字段投影恢复。API v1 只保存本规范明确列出的 CPA/AI Switch 字段；未知 API 字段忽略并产生 warning，不承诺未知字段往返。

`openai-compatibility` 的便携项每账号只包含一个 `api-key-entries`。转换为 CPA 原生配置时，只有 canonical provider 配置（规范化 name、Base URL、headers、models）完全一致的项才能聚合 key entries；同名但配置不同的项不得静默合并。

### AI Switch 导入链接

导出弹窗的第二个视图显示 API 账号的 `aiswitch://v1/import?resource=provider...` 链接，每个 API 账号最多一条，并提供单条复制和“复制全部链接（每行一条）”。这不是 CPA Scheme，也不是一条可一次批量导入的 URL。

- `app` 来自 AI Switch 平台；
- `name` 来自显示名称；
- `endpoint` 来自 Base URL；
- `apiKey` 来自 API Key；
- `model` 或 Claude 角色模型参数只在当前 v1 链接能表达时输出；
- 官方账号不生成链接；
- 只有平台支持 deeplink、`interface_format` 与该 `app` 的 v1 固定方言一致、endpoint 为单一 HTTP(S) URL、只有一个 key，且不存在 headers、`api_key_field`、`responses_custom_tool_compat` 等不可表达字段时才自动生成；
- 普通平台最多允许一个且 `from` 符合 v1 固定 alias 的模型映射；Claude 只允许可映射到 haiku/sonnet/opus 参数的条目；
- 不满足条件的账号标记为“无法生成”，本期不提供有损链接生成；
- 复制前再次提示 API Key 会进入 URL 与系统剪贴板；完整链接不得进入日志、历史记录、Toast、遥测或测试快照；
- 链接导入仍可使用，但完整迁移应使用 JSON 数组。

## 导出交互

### 入口

用户在当前平台和当前分段的一个或多个分页中勾选账号后，点击选择模式工具栏中的导出图标。所选 ID 由后端批量加载，不能从当前页缓存拼接账号内容。切换平台或分段会清空选择；筛选、翻页和页大小变化保留选择，因此导出可包含当前筛选下暂时不可见的已选账号。本期页面导出仍为单平台文件；导入协议本身允许混合平台数组。

### 弹窗

导出弹窗包含：

- `迁移 JSON` 与 `AI Switch 导入链接` 分段；
- 官方/API/总账号数量；
- “包含增强迁移信息”开关，默认开启；最小 `x-ai-switch` 核心不受该开关影响；
- 只读、可选择的完整 JSON 文本；
- 复制当前视图；
- 保存 JSON；
- 警告与错误摘要。

弹窗打开即表示用户主动请求查看敏感凭据，可以显示完整序列化结果，但必须提示该内容包含密钥。密钥、令牌和完整导入链接不得进入日志、Toast、错误详情、遥测、React Query 持久缓存、`localStorage` 或测试快照。关闭弹窗时立即清空组件中的 JSON 和链接状态；复制后的短暂成功状态不能携带敏感内容。

任一所选账号无法安全序列化时，复制和保存按钮禁用，并列出账号名称与安全错误码。导出不能静默丢弃错误账号后生成部分文件。只有不会破坏可迁移性的兼容性差异作为 warning，允许用户继续导出。

### 保存文件

建议文件名为（当前账号页单平台导出）：

```text
ai-switch-<platform>-accounts-<YYYYMMDD-HHmmss>.json
```

内部服务生成混合平台文件时，文件名使用 `ai-switch-mixed-accounts-<YYYYMMDD-HHmmss>.json`。

Desktop 新增 Tauri-only `save_route_credential_export({ suggested_file_name, json_text })`：命令内部打开原生保存对话框，用户选择后校验或补齐 `.json` 后缀，重新确认内容是限制范围内的裸数组，再使用原子安全写入能力落盘。前端不得传入任意文件路径；用户取消返回明确 `cancelled`，不是错误。该命令只注册到 Tauri handler 和 `desktopOnlyCommands`，绝不注册 Web handler。Web 只使用 Blob 下载，并为敏感导出响应设置 `Cache-Control: no-store`。复制与保存必须使用弹窗中同一个 `json_text`，避免两次生成导致字段或令牌版本不一致。

## 跨电脑批量导入

### 入口与输入

顶部工具栏的迁移导入按钮打开独立弹窗，支持：

- 选择一个 `.json` 文件；
- 粘贴 JSON 文本；
- 导入混合平台的裸数组。

Desktop 与 Web 都由浏览器 `<input type=file>`/File API 读取文件内容并传递文本，不使用文件选择器返回的路径，也不新增 Web 可调用的任意路径读取命令。读取前检查文件大小并以严格 UTF-8 解码；粘贴文本由后端执行同样的大小限制。

现有“新增账号”弹窗中的平台专用官方导入继续保留，用于 session JSON、Sub2API 等灵活输入；迁移导入只处理本设计定义的数组。

### 自动识别

导入器优先按每项 `x-ai-switch.kind` 与 `x-ai-switch.cpa_section` 分类；无迁移元数据时只接受可明确识别的官方 Auth-File 项。API 项的 `type` 仅在兼容旧文件时作为迁移判别字段，不是 CPA 原生字段；聚合回 CPA 配置前必须删除它。

- `codex`、`claude`、`gemini`、`xai`/`grok` 等官方 Auth-File 类型 -> 官方账号；
- `x-ai-switch.cpa_section` 为受支持的 API section -> 对应 API 账号；
- 无 `x-ai-switch` 的裸 API entry 只有在用户显式选择目标平台和方言后才可导入，否则标记为无法可靠分类；
- `interactions-api-key`、`vertex-api-key` 与 CPA 顶层 `api-keys` -> 明确标记为不受支持的 CPA 配置，不降级猜测。

`openai-compatibility` 如果没有 `x-ai-switch.platform`，导入预览要求用户按原数组 `item_index` 为该项选择目标平台和方言，不能静默猜测为 Codex。所有 choice 都以原数组的 `item_index` 为稳定键，预览和提交不得使用名称或 type 作为索引。

### 预览

解析完成后先显示一次确认页：

- 总数、官方数、API 数；
- 各平台与各 `x-ai-switch.cpa_section`（兼容旧文件时显示 legacy `type`）数量；
- 将创建的批次数；
- 可导入项、完全重复项、冲突项和错误项；
- 带有 `in_pool: true` 的账号数量；
- “恢复原已入池状态”复选框，默认关闭。

用户只确认一次。解析或结构错误必须在确认前出现。预览项只返回 `item_index`、脱敏显示名、kind、platform/type、分类状态和安全错误码，不返回原始 item、token、API key、完整导入链接或 fingerprint。只有预览中标记为可导入的账号进入提交阶段；提交时重新解析同一原文并在事务内复核。

### 批次恢复

- 有完整来源身份的项按 `(source_instance_id, source_batch_id, batch_name)` 分组；
- 只有 `batch_name` 的项按本次导入内的名称分组；
- 没有 `source_instance_id` 的旧文件只在本次文件内按来源批次字段分组，不把来源 ID 当全局身份；
- 每组创建新的本地批次 ID；
- 不因为本机存在同名批次就自动合并；
- 无批次信息的项保持单账号。

### 重复与冲突

后端为规范化凭据计算仅在内存和数据库查询中使用的 SHA-256 指纹，指纹不得返回前端或写入日志。指纹输入为去掉 `x-ai-switch`、显示名、批次、池状态和运行统计后的认证/路由规范化字段，按键排序生成 canonical JSON 后计算 SHA-256：API 包含 API key、规范化 Base URL、interface format、headers、模型映射和兼容开关；有 refresh token 的 OAuth 以 refresh token、account/workspace 标识、认证端点和模式为主并排除易变化的 access/id token、expiry、last_refresh；access-token-only 凭据包含 access token；Agent Identity 包含私钥、runtime、task 和账号标识。来源表保存导入时的不可变指纹，本地编辑或自动刷新不更新该值。

- 同一输入数组中 canonical payload 指纹完全相同 -> 只保留第一项，其余标记为输入内重复；
- 已存在相同 `(source_instance_id, source_credential_id, platform, kind)` 且来源指纹相同 -> 标记为重复并跳过；
- 已存在相同 `(source_instance_id, source_credential_id, platform, kind)` 但来源指纹不同 -> 标记为冲突，不覆盖；
- 没有完整可信来源身份时，即使与本机当前凭据指纹相同也只标记为“可能重复”并警告，不自动跳过；用户确认后可创建新账号；
- 邮箱、显示名称或 account ID 相同但指纹不同 -> 不自动合并，也不覆盖；
- 冲突和重复都在最终结果中单独计数。

为支持来源 ID 幂等识别，导入时将受支持的 `x-ai-switch` 来源信息保存在专用来源映射表，不塞进 `config_json`，也不能影响实际路由配置生成。

### 提交与算力池

提交时重新校验账号和重复状态，防止预览后数据库变化。迁移服务只调用一次 `pool.begin()`；预览后的重复/冲突复核、批次创建、credential 创建、来源映射插入和可选池成员追加全部使用同一个 `&mut Transaction<'_, Sqlite>`：

- 默认忽略全部 `in_pool` 值，新账号保持未入池；
- 只有用户主动勾选“恢复原已入池状态”、再次确认“将把含凭据文件中的池状态写入本机”，并通过显示受影响平台与账号数量的确认步骤后，才将成功导入且元数据为 `true` 的账号加入各自平台算力池；
- 恢复只追加本事务新建成员：按平台读取现有 `MAX(sort_order)` 后递增插入，使用唯一约束与 `ON CONFLICT DO NOTHING` 防重，绝不删除、替换、禁用或重排目标电脑已有成员；
- 重复、冲突或失败项不改变已有池状态；
- 只有至少一个成功导入项的组才创建批次；任一数据库错误使 command 整体失败并回滚，不返回“部分成功”的数据库结果，也不留下半批账号或半批池成员。

完成页显示导入成功、跳过重复、冲突、失败和恢复入池数量，并刷新受影响平台的账号页与算力池查询。

### 资源限制

- 源 JSON 必须为 UTF-8，最大 `8 MiB`；
- 数组最多 `2000` 项，单项序列化后最大 `256 KiB`；
- 单次导出最多 `2000` 个 ID，最终 `json_text` 最大 `8 MiB`；
- 前端先做友好检查，Rust 服务再次强制检查；错误只返回大小/数量和安全码；
- Web command 的外层 JSON 会产生转义开销，因此 Axum `/api/*` body limit 至少设为 `12 MiB`；Authorization 必须在 body/`Json<Value>` 提取和大块分配之前由 middleware 或 handler 前置阶段完成，不能扩大未鉴权请求的内存消耗面。

## 后端边界

### 前端组件边界

`AccountsScreen` 继续负责查询、mutation 和页面级状态协调，但新工作区视图拆为聚焦组件，避免继续扩大单文件渲染树：

- `AccountWorkspaceToolbar`：普通/选择工具栏和筛选 Popover；
- `PoolStatusStrip`：固定摘要、常用池操作与详情 Popover；
- `AccountListPane`：列表头、滚动区、账号行和展开详情；
- `AccountWorkspaceStatusBar`：分段器、反馈和分页；
- `RouteCredentialExportDialog`：迁移 JSON/导入链接预览、复制和保存；
- `RouteCredentialImportDialog`：文件/文本输入、预览、确认和结果。

这些组件只通过显式 props 接收数据与回调，不各自重复发起账号查询。迁移 JSON/导入链接的真实格式生成必须位于 Rust 服务，前端仅展示后端结果。

### 服务与接口边界

账号 CRUD 服务不直接承担格式转换。新增专用迁移服务负责：

- 按 ID 批量读取完整凭据并保持稳定顺序；
- 合成官方 CPA Auth-File 项；
- 映射 API CPA 配置项；
- 生成符合条件的 AI Switch 导入链接；
- 生成唯一的 pretty JSON 文本；
- 解析、预览和提交便携数组；
- 计算不外泄的凭据指纹；
- 恢复批次与可选池成员状态。

新增数据库迁移以隔离安装身份与来源映射：

```text
transfer_installation_identity(
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  instance_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
)

route_credential_transfer_origins(
  route_credential_id TEXT PRIMARY KEY REFERENCES route_credentials(id) ON DELETE CASCADE,
  source_instance_id TEXT NOT NULL,
  source_credential_id TEXT NOT NULL,
  source_platform TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_schema_version INTEGER NOT NULL,
  source_fingerprint TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(source_instance_id, source_credential_id, source_platform, source_kind)
)
```

首次需要导出或导入时生成稳定 UUID 作为本安装 `instance_id`，之后不随升级变化；来源指纹增加查询索引。每次导出写当前安装 `instance_id` 和当前本地 credential ID。旧文件只有 `source_credential_id` 而无 `source_instance_id` 时，不把该 ID 当作全局幂等键。

现有 `202607130011_route_credentials.sql` 已对 `(platform, route_credential_id)` 建立唯一约束，因此本功能不新增池成员去重迁移。迁移服务新增 transaction-aware 原语：`BatchRepository::create_tx`、`RouteCredentialRepository::create_tx`、来源 repository 的查询/插入方法以及 `RoutePoolRepository::append_members_tx`。这些方法接收现有 transaction，不自行 `begin/commit`；提交路径不得复用会自行开事务的现有 `create`，也不得调用会替换成员集合的 `replace_members`。

内部 command 返回预览包装对象是允许的；“不得包一层”只约束复制内容和保存文件。逻辑接口为：

```text
export_route_credentials(input)
  input: selection_context, credential_ids, include_enhanced_metadata
  output: json_text, suggested_file_name, counts, scheme_links, warnings, errors

preview_route_credential_import(input)
  input: text, ambiguous_platform_choices
  output: counts, items

import_route_credentials(input)
  input: text, ambiguous_platform_choices, restore_pool_membership
  output: imported, skipped_duplicates, conflicts, failed, restored_pool_members
```

Repository 增加按 ID 批量读取，避免逐账号 `get` 形成 N+1 查询。缺失 ID 或不属于当前选择上下文的 ID 必须在导出前返回结构化错误，不能静默少导。

`selection_context` 只包含当前 `platform + pool_scope`，用于验证所有 ID 确实属于用户勾选时的平台和分段；筛选不属于安全边界，因为选择会跨筛选保持。跨平台或分段外 ID 必须返回结构化错误，不能静默少导。内部迁移序列化器仍可处理混合平台数组，供导入测试、未来全局导出或受信服务调用复用。

Tauri command 与 Web transport 都支持格式生成、预览和导入。只有原生保存是 Desktop 专属能力；Web 仅返回内容并由浏览器下载。Web 敏感导出/导入 command 必须先通过现有认证、设置 `Cache-Control: no-store`、禁止服务端日志/缓存响应体，并要求非 loopback 部署使用 TLS；未满足这些条件时 Web handler 不注册该能力。

## 错误处理

- 顶层不是数组、数组元素不是对象、缺少有效 `x-ai-switch` 最小核心且又不能明确识别为官方 Auth-File -> 阻止对应项确认；
- API 项缺少 key、Base URL 或目标结构必需字段 -> 对应项错误；
- 官方 OAuth 项缺少可用 token -> 对应项错误；
- Agent Identity 缺少运行所需字段 -> 对应项错误；
- 可信官方 CPA raw 中未知顶层字段 -> 允许保留；API 项或普通元数据中的未知字段 -> 忽略并 warning；
- 未知或不受支持的 `cpa_section`/旧 API `type` -> 对应项错误，不降级猜测；
- `x-ai-switch` 与顶层字段冲突或 major 版本未知 -> 对应项 fatal error；
- 关闭迁移元数据导致字段丢失 -> 导出警告，但不泄露字段值；
- 导入链接不能完整表达 -> 仍保留迁移 JSON，链接区将该账号标记为“无法生成”；
- 复制失败或保存失败 -> 弹窗内显示可恢复错误，JSON 文本不丢失；
- 导入事务失败 -> 全部回滚并显示结构化错误；
- 所有错误信息只包含账号显示名、字段名和安全错误码，不包含 token、API Key 或完整 URL 查询串。

## 测试

### Rust 序列化

- 官方账号以当前 secret/config 覆盖过期 raw token；
- 当前空字段会删除 raw 中的旧 snake_case 与 camelCase 别名；
- CPA wrapper 与 Sub2API 字段正确扁平化；
- Agent Identity 字段完整时可往返导入，残缺时报告错误；
- Grok 官方类型规范化为可信 `xai`；
- 各 API dialect 映射到正确 `x-ai-switch.cpa_section`，API 顶层不注入自定义 `type`；
- `openai-compatibility` 每项只包含一个 key entry；
- 模型 `from/to/label/supports_1m` 正确映射；
- 增强信息开关只添加或移除可选 `x-ai-switch` 字段，最小核心始终存在；
- 重复输入 ID 去重，输出顺序稳定；
- 复制与保存使用相同 `json_text`；
- warnings 和 errors 不包含任何密钥或 token。

### Rust 导入

- 混合官方/API/平台数组正确分类和计数；
- 无包装数组可直接预览和导入；
- `openai-compatibility` 无平台元数据时要求选择；
- 批次按来源批次恢复且不合并本机同名批次；
- 同文件重复、已有完全重复和来源冲突按规则处理；
- 不按邮箱或显示名称覆盖；
- 不支持的元数据 major 版本阻止该项导入；同 major 未知可选字段产生 warning；
- 默认不恢复池成员；
- 主动开启后只追加标记账号；
- 事务写入失败时账号、批次和池成员全部回滚。

### React

- 账号页外层不滚动，只有中间内容区滚动；
- 工具栏、算力池条和底部状态栏保持可见；
- 普通模式与选择模式工具栏正确切换；
- 行操作只显示图标且保留现有 `aria-label`；
- 次要信息只在详情展开后出现；
- 跨页选择数量保持，导出提交完整 ID 集合；
- 底部状态栏在账号和统计分段切换正确的分页器；
- 导出弹窗切换迁移 JSON/导入链接、复制、保存和显示安全警告；
- 导入弹窗完成文件/文本输入、混合类型预览和一次确认；
- “恢复原已入池状态”默认未勾选；
- 导入完成汇总并刷新受影响查询；
- 所有图标按钮可通过键盘操作并在 hover/focus 显示 Tooltip。

### Transport 合同

- 格式生成、预览和提交 command 注册到 Tauri；只有满足鉴权、`no-store`、无日志/缓存和非 loopback TLS 条件时才注册对应 Web handler；
- `save_route_credential_export` 只注册到 Tauri 和 `desktopOnlyCommands`；
- Desktop 专属保存能力不暴露为 Web 任意路径写入；
- API 类型、Rust DTO 与 TypeScript DTO 字段一致。

## 兼容性说明

- 现有分页、筛选、拖动排序、账号测试、编辑、删除、额度刷新和池成员语义继续保留；
- **AI Switch 迁移 JSON**：本应用直接导入裸数组；每项具有强制最小 `x-ai-switch`，新电脑无需手工拆分；本期页面导出单平台，导入器允许混合平台；
- **CPA Auth-File / config entry**：仅是字段投影和 CPA 原生导出的转换目标。官方数组元素可拆为独立 Auth-File；API 项按 `x-ai-switch.cpa_section` 和 canonical provider 配置聚合，转换前移除 `x-ai-switch`，且同名不同配置不得合并；
- **AI Switch 导入链接**：仅为符合 v1 表达能力的 API 账号生成，可能受字段能力限制，不作为完整备份；
- 现有平台专用官方导入继续接收无 `x-ai-switch` 的 CPA Auth-File、session JSON、Sub2API 等输入；无元数据的 CPA config entry 不属于可自动分类的 AI Switch 迁移数组，需通过专门的 config 导入/转换流程处理。
