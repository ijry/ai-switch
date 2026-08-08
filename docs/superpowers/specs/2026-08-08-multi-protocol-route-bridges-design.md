# 多协议路由桥接设计

## 目标

扩展本地路由代理，让 Codex Responses 和 Claude Messages 入口可以使用上游协议为 OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 或 Gemini native 的账号。这样可以解决当前 Codex 默认写入 `/v1/responses` 配置，但部分上游账号只支持 `/chat/completions` 导致无法使用的问题。

## 已确认决策

- 账号协议统一为 `openai`、`openai-responses`、`anthropic`、`gemini`。
- 直接删除旧的 `anthropic-messages` 格式支持。当前没有 Claude 旧数据，不需要迁移兼容。
- Claude 原生流量使用 Anthropic Messages 的 `/v1/messages`，不支持旧的 Anthropic Text Completions `/v1/complete`。
- Claude 本地 Base URL 是路由代理根地址，例如 `http://127.0.0.1:<port>`。
- Codex 本地 Base URL 仍是 `http://127.0.0.1:<port>/v1`。
- Gemini 本地入口本轮保持现状：只支持 Gemini 自己，不增加协议下拉框。
- `responses_custom_tool_compat` 只在 Codex Responses 配置中显示。
- 转换逻辑采用 CC-Switch 风格的协议对专用 transformer，不引入强行统一所有协议的通用中间结构。

## 路由矩阵

| 本地入口 | 上游 `openai` | 上游 `openai-responses` | 上游 `anthropic` | 上游 `gemini` |
| --- | --- | --- | --- | --- |
| Codex Responses | 桥接 | 透传 | 桥接 | 桥接 |
| Claude Messages | 桥接 | 桥接 | 透传 | 桥接 |
| Gemini | 保持现状 | 保持现状 | 保持现状 | 透传 |
| 其他 OpenAI Chat 入口 | 透传 | 保持现状 | 保持现状 | 保持现状 |

## 范围

### 本轮包含

- 将 UI、解析、导入、导出、测试、凭据处理里的 `anthropic-messages` 替换为 `anthropic`。
- Codex 路由配置显示四种上游协议选项。
- Claude 路由配置显示四种上游协议选项。
- Gemini 路由配置保持 Gemini-only，不显示上游协议下拉框。
- 按路由矩阵转换请求路径、请求体、响应体和已缓冲 SSE。
- 模型连通性测试和真实代理请求使用同一套 bridge 行为。
- 保持现有路由选择、认证头生成、重试、冷却、模型映射、用量提取、请求日志、响应构造等代理职责不变。

### 本轮不包含

- Claude 旧接口 `/v1/complete`。
- 真正逐 chunk 的实时流式转换重构。当前代理会缓冲上游 SSE，本轮接受缓冲后转换。
- Gemini 本地入口桥接到非 Gemini 上游。
- 无法在目标协议中表达的 hosted tools、音频、文件搜索或文件上传语义。

## 架构

将 `services::route_protocol_bridge` 扩展为一个小 dispatcher 加若干协议对模块：

- `responses_chat.rs`：Codex Responses <-> OpenAI Chat Completions。
- `responses_claude.rs`：Codex Responses <-> Anthropic Messages。
- `responses_gemini.rs`：Codex Responses <-> Gemini native。
- `claude_chat.rs`：Anthropic Messages <-> OpenAI Chat Completions。
- `claude_responses.rs`：Anthropic Messages <-> OpenAI Responses。
- `claude_gemini.rs`：Anthropic Messages <-> Gemini native。
- 共享 helper：内容块、工具 schema、token 控制、响应 ID、用量归一化、SSE 解析与生成。

dispatcher 根据两个事实判断是否需要桥接：本地平台入口路径，以及选中凭据的 `interface_format`。判断结果要么是透传，要么是具体 bridge kind。请求侧 bridge 只改写上游路径和请求体。响应侧 bridge 只在需要时改写上游响应体和 content type。

认证不进入 bridge。现有代理继续负责选择路由凭据，并按选中账号协议附加上游 Authorization 或 API key header。

## 端点规则

- OpenAI Chat 上游路径：`/chat/completions`。
- OpenAI Responses 上游路径：`/responses`。
- Anthropic 上游路径：`/v1/messages`。
- Gemini 非流式上游路径：`/v1beta/models/{model}:generateContent`。
- Gemini 流式上游路径：`/v1beta/models/{model}:streamGenerateContent?alt=sse`。

路径归一化必须避免 `/v1/v1/messages` 这类重复版本前缀。Codex 通过 `/v1/responses` 进入代理。Claude 使用根 Base URL，并通过 `/v1/messages` 进入代理。

## 请求转换

请求 transformer 应尽量保留目标协议能表达的内容：

- 文本内容和多轮消息。
- 目标协议有等价图片块时，保留图片 URL 和 base64 图片。
- 可表达的函数工具、工具调用、工具结果、tool choice、parallel tool calls。
- 常见生成控制，包括 `temperature`、`top_p`、`stop`、stream 模式和 token 限制。
- 目标协议有兼容字段时，保留 reasoning 或 thinking 控制。
- 目标协议有兼容表示时，保留 JSON 输出控制。
- 现有模型映射完成后的 provider-specific 模型名。

遇到 hosted tools、音频、原始文件引用或其他目标协议无法表达的内容块时，必须在发送上游请求前返回明确转换错误。代理可以复用现有失败凭据上报路径，不能静默转发一个协议不匹配的坏请求。

## 响应转换

响应 transformer 必须返回本地客户端期望的协议：

- Codex 本地请求始终收到 OpenAI Responses JSON 或 Responses SSE。
- Claude 本地请求始终收到 Anthropic Messages JSON 或 Anthropic Messages SSE。
- Gemini 本地请求保持现状。

非流式转换应保留文本、内容块、工具调用、停止原因、模型、响应 ID 和用量。流式转换应解析已缓冲的上游 SSE record，将 delta 和生命周期事件映射为本地协议事件格式，并尽可能保留上游错误 record。

如果上游返回非 JSON 错误体，按上游状态透传。如果上游以成功状态返回无法转换的响应体，返回网关转换失败，不能伪造成成功的本地响应。

## UI 与数据模型

- 前端类型、Rust 模型解析、凭据导入导出、deep link 和测试统一使用 `anthropic` 作为 Claude 协议值。
- 删除所有 `anthropic-messages` 选项标签和解析分支。
- Codex Responses 账号配置显示协议下拉框，选项为 `openai`、`openai-responses`、`anthropic`、`gemini`。
- Claude 账号配置显示同样的四个协议选项。
- Gemini 账号配置保持当前 Gemini-only 行为，不增加协议下拉框。
- `responses_custom_tool_compat` 只在 Codex Responses 中显示，因为它是 Codex 兼容开关，不是通用账号能力。

## 测试

- 覆盖路由矩阵的 bridge-kind 选择单元测试。
- 覆盖每个协议对模块的请求路径和请求体转换测试。
- 覆盖非流式响应转换：文本、适用的图片、工具调用、工具结果、停止原因、ID 和用量。
- 覆盖已缓冲 SSE 转换：代表性的文本 delta、工具调用 delta、完成事件、用量和错误 record。
- 覆盖 Codex、Claude、Gemini 的协议下拉框可见性和选项集合 UI 测试。
- 覆盖导入、导出、deep link：证明 `anthropic` 可用且 `anthropic-messages` 已移除。
- 覆盖模型连通性测试：证明测试请求使用与真实代理请求相同的 bridge dispatcher。
- 现有 Rust 和前端测试套件应继续通过。

## 实现备注

以 CC-Switch 作为协议对转换和端点改写的行为参考，尤其是 Claude provider 和 transform 模块。但 AI Switch 的实现应留在现有路由代理服务边界内，不直接照搬 CC-Switch 的目录结构。

现有 Responses 到 Chat 的实现是第一个协议对模块。只有在能减少重复协议管道代码时，才抽取通用 helper。不要在没有明确重复需求前引入大型通用 schema。
