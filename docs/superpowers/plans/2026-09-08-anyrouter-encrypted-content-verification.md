# AnyRouter Responses 密文恢复：验收记录

## 当前结论

截至 2026-09-08，本次变更的离线回归通过，但 **AnyRouter 真实恢复验收尚未通过**。不能将模拟上游成功、GET 模型列表成功、HTTP 已连通或渠道满载，描述成 AnyRouter 推理请求已修复。

用户查看的是 AnyRouter 网站日志。此前手写探测产生的 `invalid codex request` 也会出现在该网站，而不是只存在于 AI Switch 本地。不能用本地旧进程未加载补丁的解释代替上游成功验收。

## 两类错误和重试边界

- `invalid_encrypted_content`：本次密文恢复的触发条件。AnyRouter 的消息可能只是 `bad response status code 400`，因此优先按结构化错误码识别，而不是只检查消息。
- `invalid_responses_request` / `invalid codex request`：不属于密文错误，不触发密文净化。是否需要修改请求格式，必须通过可成功请求与失败请求的单变量对照确认，不能直接套用密文重试。
- 该恢复机制不按域名特判，当前接入点是 `ProtocolBridgeKind::ResponsesToResponses`，不覆盖全部协议或官方凭据直通路径。
- 每个客户端请求最多净化重发一次，使用相同账号，不消耗普通重试预算。无可净化内容或再次收到同一密文错误时，返回上游错误，不遍历池或处罚账号/模型。
- 状态只存在于当前请求内。后续请求若再次带来失效密文，仍可能再触发一次恢复。没有跨请求的自动学习、永久禁用密文或无限循环。
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

必须同时满足：

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
