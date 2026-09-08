# AnyRouter Responses 密文恢复：验收记录

## 当前结论

截至 2026-09-08，本次变更的离线回归通过，但 **AnyRouter 真实恢复验收尚未通过**。不能将模拟上游成功、GET 模型列表成功、HTTP 已连通或渠道满载，描述成 AnyRouter 推理请求已修复。

用户查看的是 AnyRouter 网站日志。此前手写探测产生的 `invalid codex request` 也会出现在该网站，而不是只存在于 AI Switch 本地。不能用本地旧进程未加载补丁的解释代替上游成功验收。

## 两类错误和重试边界

- `invalid_encrypted_content`：本次密文恢复的触发条件。AnyRouter 的消息可能只是 `bad response status code 400`，因此优先按结构化错误码识别，而不是只检查消息。
- `invalid_responses_request` / `invalid codex request`：不属于密文错误，不触发密文净化。是否需要修改请求格式，必须通过可成功请求与失败请求的单变量对照确认，不能直接套用密文重试。
- 该恢复机制不按域名特判，当前接入点是 `ProtocolBridgeKind::ResponsesToResponses`，不覆盖全部协议或官方凭据直通路径。
- 默认关闭发送前净化：每个客户端请求最多净化重发一次，使用相同账号，不消耗普通重试预算。无可净化内容或再次收到同一密文错误时，返回上游错误，不遍历池或处罚账号/模型。
- 自动恢复状态只存在于当前请求内，后续请求仍可能触发一次恢复；没有跨请求的自动学习或无限循环。可通过下述账号开关明确选择每次发送前净化。
- 其他错误仍按原有路由失败策略处理；本变更并未改为“所有 HTTP 400 都只重试一次”。

## 此轮收紧

- 净化范围限制为顶层 `input` 的历史项及消息的直接 `content` 分段，不再递归扫描整个 JSON。
- 保留工具参数、工具输出、元数据中的业务 JSON，即使它们恰好含有 `type: compaction` 或 `encrypted_content`。
- 保留空/空字符串密文的明文 reasoning；只移除因净化而变空的消息，不顺带删除原有空消息。
- 保留 `include: ["reasoning.encrypted_content"]`、当前 `compaction_trigger`、普通消息和调用/结果关联。
- 收紧消息兜底，避免将“其他参数 invalid，但提及 encrypted content”的错误误判为密文失败。
- 无法解密的历史压缩内容不能恢复；删除该内容不等于保留了其中的上下文，明文历史仍保留。

## 隔离实测入口

测试默认忽略，只有显式运行才消耗上游额度。它只读源 SQLite，将指定凭据复制到内存数据库，并在随机环回端口启动当前编译代码的代理；不调用已安装应用，不写源数据库，不输出 Key。

从 `src-tauri` 运行：

```powershell
$env:CARGO_TARGET_DIR = 'target-codex'
$env:AI_SWITCH_LIVE_RESPONSES_DB = "$HOME\.ai-switch\ai-switch.db"
$env:AI_SWITCH_LIVE_RESPONSES_CREDENTIAL = '<待测试的 Codex 凭据 ID>'
$env:AI_SWITCH_LIVE_RESPONSES_MODEL = 'gpt-6-astra'
cargo test --no-default-features --features standalone-server --lib live_responses_encrypted_content_recovery -- --ignored --nocapture
```

默认的报错恢复模式必须同时满足：

1. 正常请求收到完整 Responses 响应和非空正文。
2. 加入失效历史密文后，上游先返回 `400 / invalid_encrypted_content`。
3. 同一账号净化后的请求成功，形成 `200 → 400 → 200` 三次上游记录。
4. 账号仍正常且没有新增失败计数。

HTTP 200 空响应、只有 keepalive、网络超时、其他错误码或满载均不通过。

## 2026-09-08 上游探测结果

- 使用数据库 Key 直连原模型：多次 `500 / get_channel_failed`，消息表示 `gpt-6-astra` 达到负载上限。可在 Any 网站按请求号 `20260908132003582608872Pk6XXG1Z` 核对其中一次探测。
- 移除 Lite 请求头、移除 reasoning.context、增加 instructions 的对照也遭遇渠道满载，**不足以确定究竟哪个字段导致此前的 invalid codex request**。
- 另一个配置模型 `gpt-5-codex` 返回不支持该模型，只作对照，不用其结果替代原模型验收。
- 当前代码的隔离代理实测三次均在正常请求阶段失败：两次读取无数据超时，一次连接失败。没有进入密文重试阶段，没有任何一次满足上述验收链。
- 停止继续发送同类失败请求，避免继续产生误导性的上游日志。待上游可用后使用同一入口复测，保留“不通过”结论直到完整验收成功。

## 离线回归结果

- 定向过滤 `encrypted_content`：8 项通过，1 项显式上游实测默认忽略。
- `cargo test --no-default-features --features standalone-server --lib`：1420 项通过，0 失败，5 项忽略。
- 修改文件的 rustfmt 检查及 `git diff --check` 通过。
- 上述离线结果不覆盖上游真实可用性；显式启用的实时验收三次均失败，未将其跳过后宣称通过。

## 账号级发送前净化开关

- 入口：Codex 的 API 账号，接口格式选择 `openai-responses`，在新增或编辑的「高级」页启用「Responses 历史密文净化」。
- 持久化字段：`config_json.responses_encrypted_content_cleanup`，默认 `false`，旧账号缺少该字段时也视为关闭，无需数据库迁移。
- 开启时在构造上游请求时清理，而不是等 400 后才修改。只影响该账号的原生 Responses 请求，不改变其他账号或协议。
- 记录本次上游请求是否已经预清理。已清理的请求即使又收到同类密文错误，也直接返回，不再进行一次相同的清理重发。
- 不删除顶层 include，也不改变输出密文。客户端仍可能回传历史密文，但下一次发送到开启此选项的账号时再次预清理，不需要先让上游拒绝。
- 会保留普通消息、调用和结果、工具配置、元数据及当前 compaction_trigger；仍需接受加密推理及压缩上下文丢失的兼容性代价。

实测发送前净化模式，增加：

```powershell
$env:AI_SWITCH_LIVE_RESPONSES_CLEANUP = '1'
```

该模式要求正常请求、带失效密文的请求、再次回传同样密文的请求均完整成功，只有 `200 → 200 → 200` 三次上游调用，不允许靠额外重试通过。

可选 `AI_SWITCH_LIVE_RESPONSES_FIXTURE` 指向本地 JSON：`{"body": <请求体>, "headers": <请求头对象>}`。头部只读取白名单，Authorization 始终使用内存测试库生成的本地 Key，真正上游 Key 只取自源库。捕获的请求头和默认模板互斥，不能给旧格式额外混入 Lite 协议标记。夹具不要提交真实 Key、业务输入或私有会话内容。

## 本轮新增验收结果

- `pnpm typecheck` 通过；AccountsScreen 的 217 项测试通过。
- Rust library：1423 项通过、0 失败、5 项默认忽略。
- 回归覆盖了创建默认值、勾选保存、编辑读取/关闭、表单重置、非 Responses 隐藏、关闭原样发送、开启首次预清理，以及两轮独立请求各只发送一次。
- 本机 Codex 0.147.0 客户端连接本地模拟 SSE 端点，采集请求结构；不发送到 Any，不执行模型工具。该版本对 `gpt-6-astra` 使用 fallback 元数据，不能据此声称完全复现原始会话。
- 将捕获结构的业务输入替换为简单连通性提示，删除认证头；使用该格式及数据库 Key 直连 Any 得到 HTTP 500 空响应。
- 开启发送前净化的当前编译代码，在随机端口/内存数据库中用该结构实测，也得到上游 HTTP 500 空响应，客户端收到代理 502。仍在正常请求阶段失败，未获得三轮成功链，实测明确失败。
- 同一数据库 Key 的 GET `/v1/models` 返回 200，包含 `gpt-5-codex` 和 `gpt-6-astra`；这不等价于推理权限或推理请求可用。
- 结论：发送前净化的实现与回归通过，但 **本轮仍未成功请求 AnyRouter 的推理接口**。不把 500 空响应归因于密文，也不把它包装为修复成功；网站端失败原因仍需独立定位。

## cc-switch 对照

用户所说的 cc-switch 按 `farion1231/cc-switch` 项目核对，源码快照为 `f3b18df12007d0fd79fd8ad8d310880664015197`。

- Issue #4464 有用户报告会话迁移后出现 `invalid_encrypted_content`，说明 cc-switch 用户也可能遇到这类问题；这不是 AnyRouter 专属错误。
- PR #2581 提出过移除加密历史后重试一次，并对供应商记忆 30 分钟的方案。GitHub API 确认其 `state=closed`、`merged=false`、`merged_at=null`；不能将这个提案视为已发布功能。
- 核对该快照的 handlers、forwarder、Codex provider、body_filter 及相关配置，未发现原生 Responses 路径的同类通用密文错误重试。已有 Anthropic thinking 签名整流由 Anthropic 分支触发，不等同于 Codex 密文净化。
- xAI 的特定 Responses agent_message 兼容转换也不能当作 AnyRouter 的通用解密失败处理。
- 因此若 cc-switch 同样跨上游回放不可验证的密文，仍需要对应的兼容策略；不能仅因为更换客户端就假定密文天然可跨账号使用。

核对来源：

- `https://github.com/farion1231/cc-switch/issues/4464`
- `https://github.com/farion1231/cc-switch/pull/2581`
- `https://api.github.com/repos/farion1231/cc-switch/pulls/2581`
- `https://github.com/farion1231/cc-switch/tree/f3b18df12007d0fd79fd8ad8d310880664015197/src-tauri/src/proxy`
