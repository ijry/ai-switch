# 账号失败策略配置实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 将临时请求重试、Codex 服务过载重试和连续语义错误判定统一为可按账号配置的失败策略，并在账号编辑页清晰说明适用的失败情况。

**架构：** 策略持久化在账号 `config_json.failure_policy` 中，避免新增面向用户的数据库列并兼容既有账号。Rust 提供统一的解析、默认值和边界校验，代理与模型测试共用相同策略；连续错误记录由仓储方法接收账号阈值。前端编辑账号时读写该对象，并用说明面板列出会重试、会累计异常及不会重试的失败类型。

**技术栈：** Rust、Serde JSON、SQLx、Axum/Reqwest、React、TypeScript、Vitest、Testing Library。

## 全局约束

- 每个账号独立配置失败策略，默认额外重试 `2` 次、固定重试间隔 `200ms`、相同语义错误连续 `10` 次后设为异常。
- 网络连接失败、超时、响应读取失败、HTTP `408`、`429`、`5xx` 与 Codex“服务器当前过载”语义响应共用重试次数和间隔。
- HTTP `401`、`403` 不在同账号重试；现有撤销或鉴权失败处理保持不变。
- 普通永久语义错误不重试，只按相同 HTTP 状态与规范化消息连续累计；错误变化或成功会清零。
- 重试耗尽的临时错误只记录一次临时失败，不累计为语义异常。
- 既有账号没有配置时自动使用默认值，不要求数据迁移或批量回填。
- 编辑页必须明确展示“会自动重试”“会累计为异常”“不会自动重试”三类失败情况。
- 不提交、不推送，不修改用户现有的其他未提交改动。

---

### 任务 1：定义统一失败策略模型

**文件：**
- 修改：`src-tauri/src/models/route_credential.rs`
- 修改：`src-tauri/src/services/route_credential_service.rs`
- 测试：`src-tauri/src/services/route_credential_service.rs`

**接口：**
- 新增 `RouteCredentialFailurePolicy { retry_count, retry_interval_ms, semantic_error_threshold }`。
- 新增默认常量 `2`、`200`、`10` 和安全范围校验。
- 新增从 `config_json.failure_policy` 读取并回退默认值的解析函数。

- [ ] 添加失败策略结构、默认值及 JSON 解析测试。
- [ ] 在更新账号前校验策略数字为整数且处于允许范围。
- [ ] 保持旧账号和缺少部分字段的配置向后兼容。

### 任务 2：统一代理与模型测试重试

**文件：**
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/services/route_model_test_service.rs`
- 测试：`src-tauri/src/services/route_proxy_service.rs`
- 测试：`src-tauri/src/services/route_model_test_service.rs`

**接口：**
- 代理每次选择账号后解析该账号的失败策略。
- 模型测试发送函数接收同一失败策略。

- [ ] 将网络、读响应、`408`、`429`、`5xx` 的硬编码两次重试替换为账号配置。
- [ ] 将 Codex 过载的五段退避合并为相同次数和固定间隔策略。
- [ ] 每次额外重试前等待配置的间隔；配置为 `0` 时立即重试。
- [ ] 保持 `401`、`403` 不做同账号重试并继续既有账号切换逻辑。
- [ ] 添加默认值、自定义次数、自定义间隔和过载统一行为测试。

### 任务 3：使用账号级异常阈值

**文件：**
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`
- 修改：`src-tauri/src/services/route_proxy_service.rs`
- 修改：`src-tauri/src/services/route_model_test_service.rs`
- 测试：`src-tauri/src/database/repositories/route_credential_repository.rs`

**接口：**
- `record_semantic_failure_with_status` 增加 `error_threshold: i64` 参数。

- [ ] 删除仓储层固定阈值并使用调用方传入的已校验阈值。
- [ ] 代理和模型测试从当前账号策略传入异常阈值。
- [ ] 添加自定义阈值、阈值下限及错误变化清零测试。

### 任务 4：接入账号编辑界面

**文件：**
- 修改：`src/lib/api/types.ts`
- 修改：`src/screens/AccountsScreen.tsx`
- 测试：`tests/AccountsScreen.test.tsx`

**接口：**
- TypeScript 新增 `RouteCredentialFailurePolicy`。
- 编辑表单提供额外重试次数、重试间隔（毫秒）、连续异常触发次数三个数字输入。

- [ ] 打开编辑弹窗时从 `config_json.failure_policy` 读取，缺失时显示 `2`、`200`、`10`。
- [ ] 保存时将策略合并回原始 `config_json`，不得覆盖模型映射、请求头或其他账号配置。
- [ ] 在输入区下方展示三组清晰说明：临时网络/超时/响应读取/408/429/5xx/Codex 过载会重试；相同永久语义错误会累计；401/403 不重试且鉴权处理不变。
- [ ] 限制无效数字并给出就地错误，避免发送非法配置。
- [ ] 添加默认展示、已有配置回填、保存合并和说明文案测试。

### 任务 5：格式化与验证

**文件：**
- 验证：上述所有修改文件

- [ ] 运行 `pnpm exec vitest run tests/AccountsScreen.test.tsx`。
- [ ] 运行相关 Rust 单元测试，再运行 `cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] 运行 `pnpm typecheck`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 和 `git diff --check`。
