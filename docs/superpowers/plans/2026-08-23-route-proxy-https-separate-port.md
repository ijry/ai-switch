# 本地算力池 HTTPS 独立端口实施计划

> **已作废（2026-09-04）**：功能已实现，但**不是按这份计划**。用户要求「HTTPS 不与
> HTTP 共用端口，开启后自动使用新端口」，也就是没有开关、没有替换模式，所以 Task 1
> （`separate_port` 配置字段与迁移）、Task 5 的 `set_separate_port` 命令、Task 6 的
> 复选框全部不再需要。实际落地的是两态 `RouteProxyTransport { HttpOnly, HttpAndHttps }`
> 加双监听器，以及在「写入配置」弹窗里显示 HTTP/HTTPS 双端点。这份文件只作为当时的
> 推理留档，**不要照它继续实施**。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为本地算力池 HTTPS 增加「启用独立端口」复选框，让 HTTPS 监听 `HTTP 端口 + 1` 并与 HTTP 并存，客户端配置继续写 HTTP，从而使读不到本地根证书的客户端（macOS/Linux 的 curl、Node 版 Claude Code）保持可用。

**Architecture:** `RouteProxyInner` 的四个平铺字段收进 `ProxyListener` 结构，运行时同时持 `http` 与 `https` 两个可选监听器，共享同一个 axum app 与 `ProxyAppState`。`RouteProxyTransport` 二态枚举替换为 `RouteProxyTransportPlan` 三态枚举，把「配置 → 行为」的映射穷举出来。核心不变量：`base_url` 恒为「客户端该用的地址」，因此写客户端配置、模型连通性测试、失效提示三处消费方零改动。

**Tech Stack:** Rust / Tokio / axum 0.8 / axum-server（TLS）/ serde；React + TypeScript + React Query；测试用 `cargo test --lib` 与 vitest。

## Global Constraints

- `BIND_HOST` 保持 `127.0.0.1`，两个监听器都只听回环，本计划不引入任何对外监听。
- 客户端配置永远写 HTTP 地址（并存模式下），这是本功能的目的，任何任务不得改变。
- 并存模式下 HTTPS 启动失败**不得**导致 `start()` 返回 `Err`；HTTP 必须继续服务。这是核心保证。
- 替换模式（`HttpsOnly`）下 HTTPS 启动失败**仍然**返回 `Err`——没有备用监听器时返回 `Ok` 是撒谎。
- 已有配置文件（无 `separatePort` 字段）必须继续表现为替换模式，老用户零感知。
- Rust 注释与提交信息中的技术判断必须写清「为什么」，与仓库现有风格一致。
- `cargo fmt` 只按显式路径传参，禁止裸跑（会重排无关的 WIP 代码）。
- 提交信息不得添加 `Co-Authored-By` 或任何 AI 署名。
- 每个任务结束时 `cd src-tauri && cargo test --lib` 必须全绿；涉及前端的任务另需 `pnpm test:run` 与 `pnpm typecheck`。

## 文件结构

| 文件 | 职责 | 任务 |
|---|---|---|
| `src-tauri/src/models/route_proxy_https.rs` | `RouteProxyHttpsConfig` 增 `separate_port` 字段与不对称默认值 | 1 |
| `src-tauri/src/services/route_proxy_https_service.rs` | `transport()` 产出三态 plan；六处结构体字面量补字段；新增 `set_separate_port` | 1、3、5 |
| `src-tauri/src/services/route_proxy_service.rs` | `ProxyListener` 抽取、双监听器启停、`RouteProxyTransportPlan` | 2、3、4 |
| `src-tauri/src/commands/route_proxy_https_commands.rs` | 新增 Tauri 命令 | 5 |
| `src-tauri/src/lib.rs` | 注册命令 | 5 |
| `src-tauri/src/web/handlers/mod.rs` | Web 传输层 dispatch | 5 |
| `src/lib/api/client.ts`、`src/lib/api/types.ts` | 前端 API 与类型 | 6 |
| `src/lib/i18n.tsx` | 中英文案 | 6 |
| `src/components/settings/route-proxy-https-settings.tsx` | 复选框与双端点显示 | 6 |
| `tests/SettingsScreen.test.tsx` | 前端测试 | 6 |

任务顺序是自底向上的：配置 → 运行时结构 → 三态 plan → 端口规则 → 命令 → 界面。任务 1、2 可独立提交且不改变任何现有行为；任务 3 是行为切换点。

---

### Task 1: 配置字段与迁移

**Files:**
- Modify: `src-tauri/src/models/route_proxy_https.rs:6-22`
- Modify: `src-tauri/src/services/route_proxy_https_service.rs` — 六处 `RouteProxyHttpsConfig { .. }` 字面量：`:212`、`:224`、`:248`、`:335`、`:1436`、`:1689`、`:1704`
- Test: `src-tauri/src/services/route_proxy_https_service.rs`（同文件 `mod tests`）

**Interfaces:**
- Consumes: 无（首个任务）
- Produces: `RouteProxyHttpsConfig.separate_port: bool`；字段默认 `false`、结构体默认 `true`

- [ ] **Step 1: 写失败的测试**

加到 `src-tauri/src/services/route_proxy_https_service.rs` 的 `mod tests` 里（紧跟现有的 `load_config_migrates_legacy_enabled_proxy_to_auto_start` 之后）：

```rust
    #[tokio::test]
    async fn separate_port_defaults_on_for_new_installs_and_off_for_existing_configs() {
        let temp = tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());

        // 没有配置文件 = 新装：默认走并存模式，这样新用户不会掉进
        // 「HTTPS 替换 HTTP 导致 curl / Node 客户端失效」那个坑。
        let fresh = RouteProxyHttpsService::load_config(&paths)
            .await
            .expect("load default config");
        assert!(fresh.separate_port);

        // 配置文件存在但没有该字段 = 老用户：必须保持替换模式，
        // 否则他们客户端配置里已写好的 https:// 地址会突然失配。
        tokio::fs::create_dir_all(&paths.data_dir)
            .await
            .expect("data dir");
        tokio::fs::write(
            &paths.route_proxy_https_config_file,
            r#"{"enabled":true,"autoStart":true}"#,
        )
        .await
        .expect("legacy config");
        let legacy = RouteProxyHttpsService::load_config(&paths)
            .await
            .expect("load legacy config");
        assert!(!legacy.separate_port);

        // 显式 false 要被尊重，不能被「新装默认」覆盖。
        tokio::fs::write(
            &paths.route_proxy_https_config_file,
            r#"{"enabled":true,"autoStart":true,"separatePort":false}"#,
        )
        .await
        .expect("explicit config");
        let explicit = RouteProxyHttpsService::load_config(&paths)
            .await
            .expect("load explicit config");
        assert!(!explicit.separate_port);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib separate_port_defaults_on_for_new_installs`
Expected: 编译失败，`no field 'separate_port' on type 'RouteProxyHttpsConfig'`

- [ ] **Step 3: 加字段与不对称默认值**

`src-tauri/src/models/route_proxy_https.rs` 把 `RouteProxyHttpsConfig` 与其 `Default` 实现替换为：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    /// HTTPS 是否监听自己的端口、与 HTTP 并存。
    ///
    /// 字段默认（`false`）与结构体默认（`true`）刻意不同，这个不对称本身
    /// 就是迁移逻辑：没有配置文件走结构体默认，新装即并存模式；配置文件
    /// 存在但没有这个字段走 serde 字段默认，老用户保持原「替换」模式，其
    /// 客户端配置里已写入的 https:// 地址继续有效。
    #[serde(default)]
    pub separate_port: bool,
}

impl Default for RouteProxyHttpsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_start: false,
            separate_port: true,
        }
    }
}
```

- [ ] **Step 4: 补齐七处结构体字面量**

编译器会逐个指出缺字段的位置。**每一处都写 `separate_port: config.separate_port`（沿用当前配置值），绝不硬编码** —— 硬编码会让「关闭 HTTPS」之类的操作顺手把用户的端口偏好重置掉。

`:212` 与 `:335` 所在的函数内已有 `config` 或需要先读取。以 `disable`（`:241-268`）为例，先在函数开头读配置：

```rust
    pub async fn disable(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let config = Self::load_config(&state.paths).await?;
        let previous = RouteProxyService::status(&state.route_proxy).await;
        if previous.running {
            RouteProxyService::stop(&state.route_proxy).await?;
        }
        Self::save_config(
            &state.paths,
            &RouteProxyHttpsConfig {
                enabled: false,
                auto_start: true,
                // 关闭 HTTPS 不该顺手重置用户的端口偏好：下次开启时
                // 他们期望仍是上次选的模式。
                separate_port: config.separate_port,
            },
        )
        .await?;
```

其余各处同理：若函数内已有 `config` 变量直接用；`enable_with_trust`（`:212`、`:224`）内已通过 `Self::load_config` 拿到过配置，复用即可；测试里的三处（`:1436`、`:1689`、`:1704`）按各自意图显式写 `true` 或 `false`。

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib route_proxy_https`
Expected: 全部 PASS，含新增的 `separate_port_defaults_on_for_new_installs_and_off_for_existing_configs`

- [ ] **Step 6: 格式化并提交**

```bash
cd src-tauri && cargo fmt -- src/models/route_proxy_https.rs src/services/route_proxy_https_service.rs && cd ..
git add src-tauri/src/models/route_proxy_https.rs src-tauri/src/services/route_proxy_https_service.rs
git commit -m "feat(https): 配置新增 separatePort 字段与老配置迁移

字段默认 false、结构体默认 true，这个不对称即迁移逻辑：新装走并存模式，
已有配置文件保持原替换模式，老用户客户端里已写入的 https:// 地址继续有效。

各处 save_config 一律沿用当前配置值而非硬编码，避免关闭 HTTPS 之类的操作
顺手重置用户的端口偏好。"
```

---

### Task 2: 抽出 `ProxyListener`（纯重构，行为不变）

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:138-168`（`RouteProxyStatus`、`RouteProxyInner`）
- Modify: `src-tauri/src/services/route_proxy_service.rs:315-450`（`start_with_upstream_timeouts`、`status`、`stop`）
- Test: `src-tauri/src/services/route_proxy_service.rs`（同文件 `mod tests`）

**Interfaces:**
- Consumes: 无
- Produces:
  - `struct ProxyListener { port: u16, base_url: String, shutdown: oneshot::Sender<()>, join_handle: JoinHandle<()> }`（私有）
  - `RouteProxyInner { http: Option<ProxyListener>, https: Option<ProxyListener>, https_error: Option<String> }`（私有）
  - `RouteProxyInner::running()`、`client_listener()`、`status()`、`shutdown_all()`（私有方法）
  - `RouteProxyStatus` 新增 `pub https_port: Option<u16>`、`pub https_base_url: Option<String>`、`pub https_error: Option<String>`

本任务**只做结构重整，不引入并存能力**。`running` 字段删除改为推导，是为了消掉一个能与现实不一致的状态。

- [ ] **Step 1: 写失败的测试**

<!-- TASK2_STEP1 -->

加到 `src-tauri/src/services/route_proxy_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn status_reports_no_https_endpoint_for_a_plain_http_proxy() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        let started = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
            .await
            .expect("start http proxy");

        assert!(started.running);
        assert!(started
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://")));
        // 纯 HTTP 代理没有 HTTPS 端点，三个新字段都应为空——它们的存在
        // 不改变现有行为。
        assert_eq!(started.https_port, None);
        assert_eq!(started.https_base_url, None);
        assert_eq!(started.https_error, None);

        let stopped = RouteProxyService::stop(&runtime).await.expect("stop");
        assert!(!stopped.running);
        assert_eq!(stopped.https_base_url, None);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib status_reports_no_https_endpoint_for_a_plain_http_proxy`
Expected: 编译失败，`struct 'RouteProxyStatus' has no field named 'https_port'`

- [ ] **Step 3: 替换状态与运行时结构**

`src-tauri/src/services/route_proxy_service.rs` 的 `RouteProxyStatus`（`:138-143`）扩为：

```rust
pub struct RouteProxyStatus {
    pub running: bool,
    pub bind_host: String,
    pub port: Option<u16>,
    /// 客户端该用的地址。并存模式下是 HTTP 端点，替换模式下是 HTTPS 端点。
    /// 写客户端配置、模型连通性测试、配置失效提示都读这个字段，所以它的含义
    /// 必须始终是「客户端该用哪个」，而不是「哪个协议在跑」。
    pub base_url: Option<String>,
    pub https_port: Option<u16>,
    /// 与 HTTP 并存的那个 HTTPS 端点；替换模式下为 None（此时 base_url 即 https）。
    pub https_base_url: Option<String>,
    /// 并存模式下 HTTPS 未能启动的原因。有值时 HTTP 仍在服务。
    pub https_error: Option<String>,
}
```

`RouteProxyInner`（`:161-168`）整体替换为：

```rust
/// 一个已启动的监听器及其停机手柄。
struct ProxyListener {
    port: u16,
    base_url: String,
    shutdown: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

#[derive(Default)]
struct RouteProxyInner {
    http: Option<ProxyListener>,
    https: Option<ProxyListener>,
    https_error: Option<String>,
}

impl RouteProxyInner {
    /// 由监听器实际存在与否推导，而不是单独存一个 bool——那个 bool 能与现实
    /// 不一致。
    fn running(&self) -> bool {
        self.http.is_some() || self.https.is_some()
    }

    /// 客户端该用的地址：有 HTTP 就用 HTTP（并存模式），否则退到 HTTPS
    /// （替换模式）。
    fn client_listener(&self) -> Option<&ProxyListener> {
        self.http.as_ref().or(self.https.as_ref())
    }

    fn status(&self) -> RouteProxyStatus {
        let client = self.client_listener();
        // 只有并存模式才有独立的 HTTPS 端点可报；替换模式下 HTTPS 就是
        // client_listener 本身，重复上报会让调用方以为有两个端口。
        let separate_https = self.http.as_ref().and(self.https.as_ref());
        RouteProxyStatus {
            running: self.running(),
            bind_host: BIND_HOST.to_string(),
            port: client.map(|listener| listener.port),
            base_url: client.map(|listener| listener.base_url.clone()),
            https_port: separate_https.map(|listener| listener.port),
            https_base_url: separate_https.map(|listener| listener.base_url.clone()),
            https_error: self.https_error.clone(),
        }
    }

    async fn shutdown_all(&mut self) {
        for listener in [self.http.take(), self.https.take()].into_iter().flatten() {
            let _ = listener.shutdown.send(());
            let _ = listener.join_handle.await;
        }
        self.https_error = None;
    }
}
```

- [ ] **Step 4: 改写 start / status / stop 使用新结构**

`start_with_upstream_timeouts`（`:315`）的早退分支（原 `:322-329`）改为：

```rust
        let mut inner = state.inner.lock().await;
        if inner.running() {
            return Ok(inner.status());
        }
```

`transport` 在 `:358` 的 `match` 中会被移动，所以要在移动前记下类别。在 `:339` 计算 `scheme` 处一并求出：

```rust
        let is_tls = matches!(transport, RouteProxyTransport::Https { .. });
        let scheme = if is_tls { "https" } else { "http" };
```

末尾（原 `:418-429`）改为：

```rust
        let listener = ProxyListener {
            port,
            base_url,
            shutdown: shutdown_tx,
            join_handle,
        };
        if is_tls {
            inner.https = Some(listener);
        } else {
            inner.http = Some(listener);
        }

        Ok(inner.status())
```

`stop`（`:432-450`）整体替换为：

```rust
    pub async fn stop(state: &RouteProxyRuntimeState) -> Result<RouteProxyStatus, AppError> {
        let mut inner = state.inner.lock().await;
        inner.shutdown_all().await;
        Ok(inner.status())
    }
```

`status`（`:432` 之前那个读 `inner.running`/`port`/`base_url` 的方法）改为返回 `state.inner.lock().await.status()`。

- [ ] **Step 5: 运行全量测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 全绿。现有约 20 处 `RouteProxyService::start(.., RouteProxyTransport::Http)` 的测试断言 `running` / `port` / `base_url`，语义未变故应照常通过——这是本次重构未改变行为的证据。

- [ ] **Step 6: 格式化并提交**

```bash
cd src-tauri && cargo fmt -- src/services/route_proxy_service.rs && cd ..
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "refactor(proxy): 抽出 ProxyListener，为双监听器让路

把运行时里平铺的 port / base_url / shutdown / join_handle 收进 ProxyListener，
运行时改为持 http 与 https 两个可选槽位。删掉 running 布尔量、改由监听器是否
存在推导，消掉一个可能与现实不一致的状态。

RouteProxyStatus 新增 httpsPort / httpsBaseUrl / httpsError 三个字段，本次均为
None，行为不变。base_url 的含义收紧为「客户端该用的地址」——写客户端配置、模型
连通性测试、配置失效提示三处都读它，因此并存模式下它必须是 HTTP 端点。"
```

---

### Task 3: 三态 plan 与双监听器启动

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`（新增 `RouteProxyTransportPlan`、`start` 改签名、抽出 `spawn_listener`）
- Modify: `src-tauri/src/services/route_proxy_https_service.rs`（`transport()`、`tls_transport()` 及其全部调用点）
- Modify: `src-tauri/src/services/route_model_test_service.rs:2377`、`:2461`（两处 `RouteProxyTransport::Http`）
- Test: 两个服务文件各自的 `mod tests`

**Interfaces:**
- Consumes: Task 1 的 `RouteProxyHttpsConfig.separate_port`；Task 2 的 `ProxyListener`、`RouteProxyInner::status()`
- Produces:
  - `pub enum RouteProxyTransportPlan { HttpOnly, HttpsOnly { certificate_pem_path: PathBuf, private_key_pem_path: PathBuf }, HttpAndHttps { certificate_pem_path: PathBuf, private_key_pem_path: PathBuf } }`
  - `RouteProxyService::start(state, pool, plan: RouteProxyTransportPlan)` — 签名变更，`RouteProxyTransport` 删除
  - `RouteProxyHttpsService::transport(paths) -> Result<RouteProxyTransportPlan, AppError>`
  - `RouteProxyHttpsService::tls_plan(material, separate_port) -> RouteProxyTransportPlan` — 取代 `tls_transport`

- [ ] **Step 1: 写失败的测试**

加到 `src-tauri/src/services/route_proxy_https_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn transport_plan_covers_all_three_configurations() {
        let temp = tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());

        RouteProxyHttpsService::save_config(
            &paths,
            &RouteProxyHttpsConfig {
                enabled: false,
                auto_start: false,
                separate_port: true,
            },
        )
        .await
        .expect("save disabled config");
        // HTTPS 关闭时 separate_port 取什么值都无关：都只有 HTTP。
        assert!(matches!(
            RouteProxyHttpsService::transport(&paths).await.expect("plan"),
            RouteProxyTransportPlan::HttpOnly
        ));

        RouteProxyHttpsService::save_config(
            &paths,
            &RouteProxyHttpsConfig {
                enabled: true,
                auto_start: true,
                separate_port: false,
            },
        )
        .await
        .expect("save replace-mode config");
        assert!(matches!(
            RouteProxyHttpsService::transport(&paths).await.expect("plan"),
            RouteProxyTransportPlan::HttpsOnly { .. }
        ));

        RouteProxyHttpsService::save_config(
            &paths,
            &RouteProxyHttpsConfig {
                enabled: true,
                auto_start: true,
                separate_port: true,
            },
        )
        .await
        .expect("save side-by-side config");
        assert!(matches!(
            RouteProxyHttpsService::transport(&paths).await.expect("plan"),
            RouteProxyTransportPlan::HttpAndHttps { .. }
        ));
    }
```

加到 `src-tauri/src/services/route_proxy_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn side_by_side_plan_serves_http_and_https_on_adjacent_ports() {
        let temp = tempdir().expect("temp dir");
        let material = write_test_certificate_material(temp.path()).await;
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        let started = RouteProxyService::start(
            &runtime,
            pool,
            RouteProxyTransportPlan::HttpAndHttps {
                certificate_pem_path: material.certificate,
                private_key_pem_path: material.private_key,
            },
        )
        .await
        .expect("start side-by-side proxy");

        // 客户端配置读 base_url，所以并存模式下它必须是 HTTP——这正是本功能
        // 的目的：读不到本地根证书的客户端仍然可用。
        assert!(started
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://")));
        assert!(started
            .https_base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://")));
        assert_eq!(started.https_error, None);
        // 相邻端口：断言是相对的，不假定 HTTP 落在哪个端口——开发机上应用
        // 本体常占着 19527。
        assert_eq!(
            started.https_port,
            started.port.map(|port| port + 1),
            "HTTPS 应落在 HTTP 端口 + 1"
        );

        RouteProxyService::stop(&runtime).await.expect("stop");
    }
```

该测试需要一个证书夹具。同样加到 `mod tests`（`rcgen` 已是常规依赖）：

```rust
    struct TestCertificateMaterial {
        certificate: std::path::PathBuf,
        private_key: std::path::PathBuf,
    }

    /// 就地签一张自签证书。这里只需要 axum-server 能加载的 PEM，不涉及信任，
    /// 所以不必走 RouteProxyHttpsService 那套完整材料生成。
    async fn write_test_certificate_material(dir: &std::path::Path) -> TestCertificateMaterial {
        let key = rcgen::KeyPair::generate().expect("key pair");
        let params = rcgen::CertificateParams::new(vec!["localhost".to_string()])
            .expect("certificate params");
        let certificate = params.self_signed(&key).expect("self signed certificate");

        let certificate_path = dir.join("test-cert.pem");
        let key_path = dir.join("test-key.pem");
        tokio::fs::write(&certificate_path, certificate.pem())
            .await
            .expect("write certificate");
        tokio::fs::write(&key_path, key.serialize_pem())
            .await
            .expect("write private key");

        TestCertificateMaterial {
            certificate: certificate_path,
            private_key: key_path,
        }
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib transport_plan_covers_all_three side_by_side_plan_serves`
Expected: 编译失败，`cannot find type 'RouteProxyTransportPlan' in this scope`

- [ ] **Step 3: 引入三态 plan 并抽出 spawn_listener**

`src-tauri/src/services/route_proxy_service.rs` 把 `RouteProxyTransport`（`:145-152`）替换为：

```rust
/// 代理要开哪些监听器。
///
/// 用三态枚举而不是 `Vec<RouteProxyTransport>`：后者允许「两个 HTTPS」这类无
/// 意义的组合，而这里的三种取值与配置的三种组合一一对应，穷举无遗漏。
#[derive(Debug, Clone)]
pub enum RouteProxyTransportPlan {
    HttpOnly,
    /// HTTPS 取代 HTTP 占用唯一端口（旧行为）。客户端配置会写 https://。
    HttpsOnly {
        certificate_pem_path: std::path::PathBuf,
        private_key_pem_path: std::path::PathBuf,
    },
    /// HTTP 与 HTTPS 各占一个端口。客户端配置写 http://，HTTPS 端点供愿意
    /// 处理根证书的客户端手动使用。
    HttpAndHttps {
        certificate_pem_path: std::path::PathBuf,
        private_key_pem_path: std::path::PathBuf,
    },
}
```

把 Task 2 留在 `start_with_upstream_timeouts` 里的单监听器启动逻辑（绑定 + 建 app + spawn）抽成一个自由函数，供两个监听器复用：

```rust
/// 在 `listener` 上启动一个监听器。`tls` 为 `Some` 时提供 TLS 证书对。
async fn spawn_listener(
    listener: TcpListener,
    app: Router,
    tls: Option<(&std::path::Path, &std::path::Path)>,
) -> Result<ProxyListener, AppError> {
    let addr = listener.local_addr().map_err(|err| AppError::Filesystem {
        code: "filesystem.route_proxy_addr",
        message: "Could not resolve route proxy address".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;
    let port = addr.port();
    let scheme = if tls.is_some() { "https" } else { "http" };
    let base_url = format!("{scheme}://{BIND_HOST}:{port}");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join_handle = match tls {
        None => tokio::spawn(async move {
            let server = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(err) = server.await {
                eprintln!("route proxy server error: {err}");
            }
        }),
        Some((certificate_pem_path, private_key_pem_path)) => {
            let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
                certificate_pem_path,
                private_key_pem_path,
            )
            .await
            .map_err(|error| AppError::Validation {
                code: "validation.route_proxy_https_certificate",
                message: "Could not load local route proxy HTTPS certificate".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
            let std_listener = listener.into_std().map_err(|error| AppError::Filesystem {
                code: "filesystem.route_proxy_tls_listener",
                message: "Could not prepare local HTTPS listener".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
            let handle = axum_server::Handle::new();

            tokio::spawn(async move {
                let server = axum_server::from_tcp_rustls(std_listener, rustls_config)
                    .handle(handle.clone())
                    .serve(app.into_make_service_with_connect_info::<SocketAddr>());
                tokio::pin!(server);

                tokio::select! {
                    result = &mut server => {
                        if let Err(error) = result {
                            eprintln!("route proxy HTTPS server error: {error}");
                        }
                    }
                    _ = shutdown_rx => {
                        handle.graceful_shutdown(Some(Duration::from_secs(5)));
                        if let Err(error) = server.await {
                            eprintln!("route proxy HTTPS shutdown error: {error}");
                        }
                    }
                }
            })
        }
    };

    Ok(ProxyListener {
        port,
        base_url,
        shutdown: shutdown_tx,
        join_handle,
    })
}
```

`start_with_upstream_timeouts` 的主体改为按 plan 分派。`ProxyAppState` 是 `Clone`，两个监听器共享同一份凭据池缓存与活动登记，不会出现两套计数：

```rust
        let mut inner = state.inner.lock().await;
        if inner.running() {
            return Ok(inner.status());
        }

        let app_state = ProxyAppState {
            pool,
            key_cache: Arc::new(Mutex::new(RouteProxyKeyCache::default())),
            activity: state.activity.clone(),
            live_log: state.live_log.clone(),
            codex_history: CodexReasoningCache::default(),
            upstream_timeouts,
        };
        let app = Router::new()
            .fallback(any(proxy_handler))
            .with_state(app_state);

        let tls = match &plan {
            RouteProxyTransportPlan::HttpOnly => None,
            RouteProxyTransportPlan::HttpsOnly {
                certificate_pem_path,
                private_key_pem_path,
            }
            | RouteProxyTransportPlan::HttpAndHttps {
                certificate_pem_path,
                private_key_pem_path,
            } => Some((certificate_pem_path.as_path(), private_key_pem_path.as_path())),
        };

        if let RouteProxyTransportPlan::HttpsOnly { .. } = plan {
            // 替换模式：HTTPS 独占端口。这里失败必须上报——没有备用监听器时
            // 返回 Ok 却无人监听就是撒谎。
            let listener = bind_route_proxy_listener().await?;
            inner.https = Some(spawn_listener(listener, app, tls).await?);
            return Ok(inner.status());
        }

        let http_listener = bind_route_proxy_listener().await?;
        let http = spawn_listener(http_listener, app.clone(), None).await?;
        let http_port = http.port;
        inner.http = Some(http);

        if let Some(tls) = tls {
            // 并存模式：HTTPS 起不来只记录原因，HTTP 继续服务。这是本功能的
            // 核心保证——证书问题不得再拖垮唯一可用的那条路。
            match bind_route_proxy_listener_from(http_port.saturating_add(1)).await {
                Ok(https_listener) => match spawn_listener(https_listener, app, Some(tls)).await {
                    Ok(https) => inner.https = Some(https),
                    Err(error) => inner.https_error = Some(error.to_string()),
                },
                Err(error) => inner.https_error = Some(error.to_string()),
            }
        }

        Ok(inner.status())
```

签名改为 `plan: RouteProxyTransportPlan`（`start`、`start_with_test_upstream_timeouts`、`start_with_upstream_timeouts` 三处）。

- [ ] **Step 4: 更新全部调用点**

`route_proxy_https_service.rs`：`tls_transport`（`:485-490`）替换为按配置产出 plan 的 `tls_plan`：

```rust
    fn tls_plan(
        material: &RouteProxyHttpsMaterial,
        separate_port: bool,
    ) -> RouteProxyTransportPlan {
        let certificate_pem_path = material.server_certificate_pem.clone();
        let private_key_pem_path = material.server_private_key_pem.clone();
        if separate_port {
            RouteProxyTransportPlan::HttpAndHttps {
                certificate_pem_path,
                private_key_pem_path,
            }
        } else {
            RouteProxyTransportPlan::HttpsOnly {
                certificate_pem_path,
                private_key_pem_path,
            }
        }
    }
```

`transport()`（`:139-147`）改为：

```rust
    pub async fn transport(paths: &AppPaths) -> Result<RouteProxyTransportPlan, AppError> {
        let config = Self::load_config(paths).await?;
        if !config.enabled {
            return Ok(RouteProxyTransportPlan::HttpOnly);
        }

        let material = Self::ensure_material(paths).await?;
        Ok(Self::tls_plan(&material, config.separate_port))
    }
```

其余八处 `Self::tls_transport(&material)` 调用点（`:195`、`:321`、`:389`、`:415`、`:611` 及各自的回滚分支）改为 `Self::tls_plan(&material, config.separate_port)`。这些函数大多已有 `config` 在手；`restore_replacement_after_failure`（`:608` 附近）若没有，则在函数开头 `let config = Self::load_config(&state.paths).await?;`。

全部 `RouteProxyTransport::Http` 改为 `RouteProxyTransportPlan::HttpOnly`：`route_proxy_https_service.rs:258`、`route_model_test_service.rs:2377` 与 `:2461`，以及 `route_proxy_service.rs` 中约 20 处测试。编译器会逐个指出。

- [ ] **Step 5: 运行全量测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 全绿，含两个新测试。若 `side_by_side_plan_serves_http_and_https_on_adjacent_ports` 因端口被占而失败，说明相邻端口规则本身有问题（`bind_route_proxy_listener_from` 会继续上扫，`+1` 只是起点），此时该测试的相对断言应改为 `https_port > port`。

- [ ] **Step 6: 格式化并提交**

```bash
cd src-tauri && cargo fmt -- src/services/route_proxy_service.rs src/services/route_proxy_https_service.rs src/services/route_model_test_service.rs && cd ..
git add src-tauri/src/services/
git commit -m "feat(proxy): HTTPS 可与 HTTP 并存于相邻端口

RouteProxyTransport 二态枚举换成 RouteProxyTransportPlan 三态，把配置的三种
组合与行为一一对应；不用 Vec<Transport>，那会允许「两个 HTTPS」这类无意义组合。

并存模式下 HTTP 按原规则从 19527 起上扫，HTTPS 从 HTTP 端口 + 1 起上扫，两者
共享同一个 axum app 与 ProxyAppState，因此凭据池缓存与活动登记只有一份。

关键差异：并存模式下 HTTPS 起不来只记入 https_error、HTTP 继续服务；替换模式下
仍然返回 Err，因为没有备用监听器时返回 Ok 却无人监听是撒谎。"
```

---

### Task 4: 相邻端口规则与 HTTPS 非致命保证

**Files:**
- Test: `src-tauri/src/services/route_proxy_service.rs`（同文件 `mod tests`）

**Interfaces:**
- Consumes: Task 3 的 `RouteProxyTransportPlan`、`bind_route_proxy_listener_from`
- Produces: 无新接口（纯测试任务）

这两条是本功能最容易在后续改动中被破坏的性质，各自单独立测。

- [ ] **Step 1: 写上扫规则的测试**

```rust
    #[tokio::test]
    async fn adjacent_port_scan_skips_a_busy_port() {
        // 占住一个端口，再从它开始上扫，必须落到下一个。这是「HTTPS = HTTP
        // 端口 + 1」在端口已被占用时的兜底行为。
        let occupied = TcpListener::bind((BIND_HOST, 0))
            .await
            .expect("occupy a port");
        let occupied_port = occupied.local_addr().expect("addr").port();

        let next = bind_route_proxy_listener_from(occupied_port)
            .await
            .expect("scan past the busy port");

        assert_eq!(
            next.local_addr().expect("addr").port(),
            occupied_port + 1,
            "上扫应跳过被占端口"
        );
    }
```

- [ ] **Step 2: 写 HTTPS 非致命的测试**

```rust
    #[tokio::test]
    async fn a_broken_certificate_leaves_http_serving_in_side_by_side_mode() {
        let temp = tempdir().expect("temp dir");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        // 指向不存在的证书：并存模式下这不该让整个代理起不来。
        let started = RouteProxyService::start(
            &runtime,
            pool,
            RouteProxyTransportPlan::HttpAndHttps {
                certificate_pem_path: temp.path().join("missing-cert.pem"),
                private_key_pem_path: temp.path().join("missing-key.pem"),
            },
        )
        .await
        .expect("并存模式下证书问题不得使启动失败");

        assert!(started.running);
        assert!(started
            .base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://")));
        // HTTPS 缺席但有原因可报，界面据此提示用户。
        assert_eq!(started.https_base_url, None);
        assert!(started.https_error.is_some());

        RouteProxyService::stop(&runtime).await.expect("stop");
    }

    #[tokio::test]
    async fn a_broken_certificate_fails_the_start_in_replace_mode() {
        let temp = tempdir().expect("temp dir");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = RouteProxyRuntimeState::default();

        // 替换模式没有备用监听器，返回 Ok 却无人监听会骗到调用方。
        let error = RouteProxyService::start(
            &runtime,
            pool,
            RouteProxyTransportPlan::HttpsOnly {
                certificate_pem_path: temp.path().join("missing-cert.pem"),
                private_key_pem_path: temp.path().join("missing-key.pem"),
            },
        )
        .await
        .expect_err("替换模式下证书问题必须上报");

        assert!(error
            .to_string()
            .to_ascii_lowercase()
            .contains("certificate"));
        assert!(!RouteProxyService::status(&runtime).await.running);
    }
```

- [ ] **Step 3: 运行这三个测试**

Run: `cd src-tauri && cargo test --lib adjacent_port_scan a_broken_certificate`
Expected: 三个全部 PASS。若 `a_broken_certificate_leaves_http_serving_in_side_by_side_mode` 失败，说明 Task 3 的非致命分支写错了——那是本功能的核心保证，必须先修好再继续。

- [ ] **Step 4: 运行全量测试并提交**

```bash
cd src-tauri && cargo test --lib && cargo fmt -- src/services/route_proxy_service.rs && cd ..
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "test(proxy): 覆盖相邻端口上扫与 HTTPS 非致命

三条性质最容易在后续改动中被无声破坏，各自单独立测：上扫会跳过被占端口；
并存模式下坏证书只记 https_error 而 HTTP 继续服务；替换模式下坏证书仍然报错。"
```

---

### Task 5: 服务方法、命令与 HTTPS 状态

**Files:**
- Modify: `src-tauri/src/models/route_proxy_https.rs`（`RouteProxyHttpsStatus` 增 `separate_port`、`http_base_url`）
- Modify: `src-tauri/src/services/route_proxy_https_service.rs`（`status_for_state`、`status_with_trust`、新增 `set_separate_port`）
- Modify: `src-tauri/src/commands/route_proxy_https_commands.rs`（新增命令）
- Modify: `src-tauri/src/lib.rs`（注册命令）
- Modify: `src-tauri/src/web/handlers/mod.rs`（Web dispatch）
- Test: `src-tauri/src/services/route_proxy_https_service.rs`

**Interfaces:**
- Consumes: Task 1 的 `separate_port` 配置；Task 3 的 `transport()`、`RouteProxyTransportPlan`
- Produces:
  - `RouteProxyHttpsStatus` 增 `pub separate_port: bool`、`pub http_base_url: Option<String>`（`proxy_base_url` 语义收紧为「HTTPS 端点」）
  - `RouteProxyHttpsService::set_separate_port(state: &AppState, separate_port: bool) -> Result<RouteProxyHttpsOperationOutcome, AppError>`
  - Tauri 命令 `set_route_proxy_https_separate_port`，参数 `separatePort: boolean`

- [ ] **Step 1: 写失败的测试**

加到 `src-tauri/src/services/route_proxy_https_service.rs` 的 `mod tests`：

```rust
    #[tokio::test]
    async fn setting_separate_port_while_https_is_off_saves_without_restarting() {
        let fixture = test_state().await;

        // HTTPS 关着的时候两种取值都映射到 HttpOnly，plan 没变，重启纯属多余。
        let outcome = RouteProxyHttpsService::set_separate_port(&fixture.state, false)
            .await
            .expect("save preference");

        assert!(!outcome.https.separate_port);
        assert!(!outcome.https.enabled);
        // 代理本来没在跑，不该被这个操作启动。
        assert!(!outcome.route_proxy.running);

        let saved = RouteProxyHttpsService::load_config(&fixture.state.paths)
            .await
            .expect("load config");
        assert!(!saved.separate_port);
    }

    #[tokio::test]
    async fn https_status_reports_both_endpoints_in_side_by_side_mode() {
        let fixture = test_state().await;
        RouteProxyHttpsService::save_config(
            &fixture.state.paths,
            &RouteProxyHttpsConfig {
                enabled: true,
                auto_start: true,
                separate_port: true,
            },
        )
        .await
        .expect("save side-by-side config");
        let plan = RouteProxyHttpsService::transport(&fixture.state.paths)
            .await
            .expect("plan");
        RouteProxyService::start(&fixture.state.route_proxy, fixture.state.pool.clone(), plan)
            .await
            .expect("start proxy");

        let status = RouteProxyHttpsService::status_for_state(&fixture.state)
            .await
            .expect("status");

        // 界面要同时显示两个端点：客户端在用 HTTP，而 HTTPS 需要用户手动粘贴。
        assert!(status
            .http_base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://")));
        assert!(status
            .proxy_base_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://")));
        assert!(status.separate_port);

        RouteProxyService::stop(&fixture.state.route_proxy)
            .await
            .expect("stop");
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib setting_separate_port_while_https_is_off https_status_reports_both_endpoints`
Expected: 编译失败，`no function or associated item named 'set_separate_port'`

- [ ] **Step 3: 扩展 HTTPS 状态模型**

`src-tauri/src/models/route_proxy_https.rs` 的 `RouteProxyHttpsStatus` 增两个字段：

```rust
pub struct RouteProxyHttpsStatus {
    pub enabled: bool,
    /// HTTPS 是否与 HTTP 并存于独立端口。
    pub separate_port: bool,
    pub cert_ready: bool,
    pub trust_status: RouteProxyTrustStatus,
    pub trust_adapter: Option<String>,
    pub root_fingerprint: Option<String>,
    pub expires_at: Option<String>,
    pub certificate_dir: String,
    pub root_certificate_path: Option<String>,
    /// HTTPS 端点。并存模式下是那个独立端口，替换模式下就是唯一端口。
    pub proxy_base_url: Option<String>,
    /// 客户端配置实际使用的 HTTP 端点；仅并存模式下有值。
    pub http_base_url: Option<String>,
    pub message: Option<String>,
    pub manual_instructions: Vec<String>,
}
```

- [ ] **Step 4: 改 status 取值并新增 set_separate_port**

`status_for_state`（`:167-173`）改为把两个端点都传下去：

```rust
    pub async fn status_for_state(state: &AppState) -> Result<RouteProxyHttpsStatus, AppError> {
        let proxy = RouteProxyService::status(&state.route_proxy).await;
        // 并存模式下 HTTPS 是那个独立端点，HTTP 才是客户端在用的；替换模式下
        // https_base_url 为 None，base_url 本身就是 https。
        let (https_endpoint, http_endpoint) = match proxy.https_base_url {
            Some(https) => (Some(https), proxy.base_url),
            None => (proxy.base_url, None),
        };
        Self::status(&state.paths, https_endpoint, http_endpoint).await
    }
```

`status`（`:632`）与 `status_with_trust`（`:639`）各增一个 `http_base_url: Option<String>` 参数，并在构造 `RouteProxyHttpsStatus` 时（`:677` 附近）填入 `separate_port: config.separate_port` 与 `http_base_url`。文件内其余 `Self::status_with_trust(...)` / `Self::status(...)` 调用点补 `None` 作为该参数——那些路径（重装根证书、卸载等）不需要报告 HTTP 端点。

在 `disable`（`:241`）之后新增：

```rust
    pub async fn set_separate_port(
        state: &AppState,
        separate_port: bool,
    ) -> Result<RouteProxyHttpsOperationOutcome, AppError> {
        let mut config = Self::load_config(&state.paths).await?;
        config.separate_port = separate_port;
        Self::save_config(&state.paths, &config).await?;

        // HTTPS 关着的时候两种取值都映射到 HttpOnly，plan 没变，重启纯属多余。
        // 偏好照常保存，用户可以先配好、开启 HTTPS 时即生效。
        if !config.enabled {
            let route_proxy = RouteProxyService::status(&state.route_proxy).await;
            let https = Self::status_for_state(state).await?;
            return Ok(RouteProxyHttpsOperationOutcome {
                https,
                route_proxy,
                config_writes: Vec::new(),
            });
        }

        let plan = Self::transport(&state.paths).await?;
        let previous = RouteProxyService::status(&state.route_proxy).await;
        if previous.running {
            RouteProxyService::stop(&state.route_proxy).await?;
        }
        let route_proxy =
            RouteProxyService::start(&state.route_proxy, state.pool.clone(), plan).await?;
        let https = Self::status_for_state(state).await?;

        Ok(RouteProxyHttpsOperationOutcome {
            https,
            route_proxy,
            config_writes: Vec::new(),
        })
    }
```

- [ ] **Step 5: 接线命令**

`src-tauri/src/commands/route_proxy_https_commands.rs` 在 `disable_route_proxy_https`（`:26-33`）之后加：

```rust
#[tauri::command]
pub async fn set_route_proxy_https_separate_port(
    state: State<'_, AppState>,
    separate_port: bool,
) -> Result<RouteProxyHttpsOperationOutcome, ApiError> {
    RouteProxyHttpsService::set_separate_port(&state, separate_port)
        .await
        .map_err(ApiError::from)
}
```

`src-tauri/src/lib.rs` 在 `disable_route_proxy_https` 的 `use` 与 `tauri::generate_handler![...]` 列表里各加 `set_route_proxy_https_separate_port`。

`src-tauri/src/web/handlers/mod.rs` 在 `"disable_route_proxy_https"` 分支（`:670`）之后加：

```rust
        "set_route_proxy_https_separate_port" => to_value(
            RouteProxyHttpsService::set_separate_port(
                state.as_ref(),
                required_bool_arg(&args, "separatePort")?,
            )
            .await
            .map_err(to_error)?,
        ),
```

若该文件没有 `required_bool_arg` 辅助函数，参照同文件既有的 `required_string_arg` 写法新增一个：从 `args` 取键、`as_bool()`、缺失或类型不符时返回同款 `ApiError`。

- [ ] **Step 6: 运行全量测试并提交**

```bash
cd src-tauri && cargo test --lib && cargo fmt -- src/models/route_proxy_https.rs src/services/route_proxy_https_service.rs src/commands/route_proxy_https_commands.rs src/lib.rs src/web/handlers/mod.rs && cd ..
git add src-tauri/src
git commit -m "feat(https): 新增独立端口开关命令与双端点状态

RouteProxyHttpsStatus 增 separatePort 与 httpBaseUrl；proxyBaseUrl 语义收紧为
「HTTPS 端点」，并存模式下取那个独立端口、替换模式下仍是唯一端口，对老用户
无变化。界面需要同时拿到两个端点，因为客户端在用 HTTP，而 HTTPS 要手动粘贴。

set_separate_port 在 HTTPS 关闭时只存偏好、不重启代理——那种情况下两种取值都
映射到 HttpOnly，plan 没变，重启纯属多余。偏好照常保存，用户可以先配好、开启
HTTPS 时即生效。"
```

---

### Task 6: 界面复选框与双端点显示

**Files:**
- Modify: `src/lib/api/types.ts:543-569`（`RouteProxyStatus`、`RouteProxyHttpsStatus`）
- Modify: `src/lib/api/client.ts`（新增 `setRouteProxyHttpsSeparatePort`）
- Modify: `src/lib/i18n.tsx`（en 与 zh 各三条新文案）
- Modify: `src/components/settings/route-proxy-https-settings.tsx`
- Test: `tests/SettingsScreen.test.tsx`

**Interfaces:**
- Consumes: Task 5 的命令 `set_route_proxy_https_separate_port`（参数 `separatePort`）、状态字段 `separatePort` / `httpBaseUrl` / `proxyBaseUrl`
- Produces: 无（终端任务）

**序列化大小写的不对称**——这是最容易写错的一处：`RouteProxyStatus` 上**没有** `#[serde(rename_all = "camelCase")]`（`route_proxy_service.rs:137`），所以它的新字段序列化为 `https_port` / `https_base_url` / `https_error`（snake_case，与既有的 `bind_host` / `base_url` 一致）；而 `RouteProxyHttpsStatus` **有**该属性（`models/route_proxy_https.rs:56`），其新字段是 `separatePort` / `httpBaseUrl`。两者必须分别照抄各自的现状。

- [ ] **Step 1: 写失败的测试**

加到 `tests/SettingsScreen.test.tsx`（紧跟现有的 `shows untrusted guidance without blocking HTTPS controls` 之后）。`httpsStatusFixture` 需先补 `separatePort: true` 与 `httpBaseUrl: null` 两个键：

```tsx
  it("shows both endpoints when HTTPS runs on its own port", async () => {
    // 并存模式下客户端配置写的是 HTTP，用户必须能看出这一点，否则无法判断
    // 该把哪个地址粘到 curl 或客户端里。
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
      ...httpsStatusFixture,
      enabled: true,
      certReady: true,
      trustStatus: "systemTrusted" as const,
      separatePort: true,
      httpBaseUrl: "http://127.0.0.1:19527",
      proxyBaseUrl: "https://127.0.0.1:19528",
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
    expect(await screen.findByText("http://127.0.0.1:19527")).toBeInTheDocument();
    expect(screen.getByText("https://127.0.0.1:19528")).toBeInTheDocument();
  });

  it("toggling the separate-port checkbox sends the new preference", async () => {
    vi.mocked(getSettings).mockResolvedValue(settingsFixture);
    vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
      ...httpsStatusFixture,
      enabled: true,
      certReady: true,
      separatePort: true,
    });

    render(
      <QueryClientProvider client={createQueryClient()}>
        <I18nProvider initialLanguage="zh-CN">
          <SettingsScreen />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
    await userEvent.click(screen.getByRole("checkbox", { name: "启用独立端口" }));

    await waitFor(() =>
      expect(setRouteProxyHttpsSeparatePort).toHaveBeenCalledWith(false),
    );
  });
```

同文件顶部的 `vi.mock("../src/lib/api/client", ...)` 块里补 `setRouteProxyHttpsSeparatePort: vi.fn()`，并在 import 列表加入该名字（照现有 `enableRouteProxyHttps` 的写法）。

- [ ] **Step 2: 运行测试确认失败**

Run: `cd "D:/Repos/xyito/open/ai-switch" && pnpm test:run tests/SettingsScreen.test.tsx`
Expected: FAIL，`setRouteProxyHttpsSeparatePort is not exported` 或 `Unable to find an element with the text: http://127.0.0.1:19527`

- [ ] **Step 3: 补前端类型与 API**

`src/lib/api/types.ts` 的 `RouteProxyStatus`（`:543-548`）——注意 snake_case：

```ts
export type RouteProxyStatus = {
  running: boolean;
  bind_host: string;
  port?: number | null;
  base_url?: string | null;
  https_port?: number | null;
  https_base_url?: string | null;
  https_error?: string | null;
};
```

`RouteProxyHttpsStatus`（`:557-569`）——camelCase：

```ts
export type RouteProxyHttpsStatus = {
  enabled: boolean;
  separatePort: boolean;
  certReady: boolean;
  trustStatus: RouteProxyTrustStatus;
  trustAdapter?: string | null;
  rootFingerprint?: string | null;
  expiresAt?: string | null;
  certificateDir: string;
  rootCertificatePath?: string | null;
  proxyBaseUrl?: string | null;
  httpBaseUrl?: string | null;
  message?: string | null;
  manualInstructions: string[];
};
```

`src/lib/api/client.ts` 在 `disableRouteProxyHttps`（`:227-229`）之后加：

```ts
export function setRouteProxyHttpsSeparatePort(
  separatePort: boolean,
): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("set_route_proxy_https_separate_port", { separatePort });
}
```

- [ ] **Step 4: 补文案**

`src/lib/i18n.tsx` 英文块（`settings.https.*` 附近）加：

```ts
  "settings.https.separatePort": "Use a separate port",
  "settings.https.separatePortHint":
    "HTTPS listens on its own port while HTTP keeps serving. Client configs keep using the HTTP address, so clients that cannot read the system trust store still work.",
  "settings.https.httpEndpoint": "HTTP endpoint (used by client configs)",
```

中文块加：

```ts
  "settings.https.separatePort": "启用独立端口",
  "settings.https.separatePortHint":
    "HTTPS 监听自己的端口，HTTP 继续服务。客户端配置仍使用 HTTP 地址，因此读不到系统信任库的客户端也能正常工作。",
  "settings.https.httpEndpoint": "HTTP 端点（客户端配置使用）",
```

- [ ] **Step 5: 改组件**

`src/components/settings/route-proxy-https-settings.tsx`：

import 加 `setRouteProxyHttpsSeparatePort`，并在 `disableMutation`（`:79-82`）之后加：

```tsx
    const separatePortMutation = useMutation({
      mutationFn: setRouteProxyHttpsSeparatePort,
      onSuccess: (outcome) => syncOutcome(queryClient, outcome),
    });
```

`isMutating`（`:107`）的 `||` 链里加上 `separatePortMutation.isPending`。

在现有 `enabled` 复选框的 `</label>` 之后插入新复选框（沿用同样的类名与结构）：

```tsx
          <label className="inline-flex items-center gap-2 rounded-xl border border-stone-200 bg-stone-50 px-3 py-2 text-[12px] font-medium text-stone-700">
            <input
              aria-label={t("settings.https.separatePort")}
              checked={https.separatePort}
              disabled={isMutating}
              onChange={(event) => {
                separatePortMutation.mutate(event.target.checked);
              }}
              type="checkbox"
            />
            {t("settings.https.separatePort")}
          </label>
          <p className="px-1 text-[11px] text-stone-500">{t("settings.https.separatePortHint")}</p>
```

端点行（`:199-202`）之后插入 HTTP 端点行。仅并存模式下 `httpBaseUrl` 有值，所以用它自身作为条件；`select-text` 让地址可复制——它不再写进客户端配置，必须手动粘：

```tsx
            {https.httpBaseUrl ? (
              <p className="min-w-0 select-text break-all sm:col-span-2">
                <span className="font-semibold text-stone-800">
                  {t("settings.https.httpEndpoint")}:
                </span>{" "}
                {https.httpBaseUrl}
              </p>
            ) : null}
```

同时给现有的 HTTPS 端点行加上 `select-text`（同一原因）。

`https_error` 的呈现无需新代码：Task 5 已让 `set_separate_port` 返回的 `https` 状态经 `status_for_state` 产出，而并存模式下 HTTPS 起不来时 `message` 会带上原因，现有的琥珀色告示区（本次会话早前改为按 `manualInstructions.length > 0` 判断）与灰色 `message` 区已能显示。

- [ ] **Step 6: 运行前端测试与类型检查**

```bash
cd "D:/Repos/xyito/open/ai-switch"
pnpm typecheck
pnpm test:run tests/SettingsScreen.test.tsx
```

Expected: typecheck 无输出；两个新测试 PASS。

- [ ] **Step 7: 全量校验并提交**

```bash
cd "D:/Repos/xyito/open/ai-switch"
pnpm test:run
(cd src-tauri && cargo test --lib)
git add src/lib/api/types.ts src/lib/api/client.ts src/lib/i18n.tsx src/components/settings/route-proxy-https-settings.tsx tests/SettingsScreen.test.tsx
git commit -m "feat(https): 设置页新增「启用独立端口」复选框

并存模式下同时显示 HTTP 与 HTTPS 两个端点，并标注 HTTP 才是客户端配置在用的
那个——否则用户无法判断该把哪个地址粘到 curl 或客户端里。两个端点都加
select-text，因为 HTTPS 地址不再写入客户端配置，只能手动复制。

注意两个状态类型的序列化大小写不同：RouteProxyStatus 没有 rename_all，新字段
是 https_base_url 等 snake_case；RouteProxyHttpsStatus 有，故为 separatePort、
httpBaseUrl。"
```

---

## 自查

**1. 规格覆盖**

| 设计文档章节 | 对应任务 |
|---|---|
| 配置与迁移（三态迁移表） | Task 1 |
| 双监听器（`ProxyListener`、`RouteProxyInner`） | Task 2 |
| 三态 `RouteProxyTransportPlan` 与启动顺序 | Task 3 |
| 状态与数据流（`base_url` 不变量表） | Task 2（结构）+ Task 5（HTTPS 状态） |
| 界面（复选框、双端点、可复制、i18n） | Task 6 |
| 错误处理（并存非致命 / 替换致命 / 端口耗尽） | Task 3（实现）+ Task 4（测试） |
| 测试清单第 1–6 条 | 1→T1、2→T3、3→T3+T5、4→T3、5→T4、6→T4 |
| 「已写入客户端配置的失效提示」 | 无需代码：`config_write_is_stale` 以 `base_url` 为入参，`AccountsScreen:3080` 已喂实时值；Task 5 的 `syncOutcome` 更新 query cache 后提示自动出现 |

**2. 占位符扫描**：无 TBD / TODO / "类似 Task N" / 无代码的代码步骤。

**3. 类型一致性**：`RouteProxyTransportPlan` 三个变体的字段名 `certificate_pem_path` / `private_key_pem_path` 在 Task 3、4、5 中一致；`separate_port`(Rust) ↔ `separatePort`(TS) 的映射由 `rename_all` 保证；`https_base_url`(Rust) ↔ `https_base_url`(TS) 因 `RouteProxyStatus` 无 `rename_all` 而保持 snake_case，已在 Task 6 显式指出。

**4. 已知风险**：Task 3 删除 `RouteProxyTransport` 会波及约 25 处调用点（含 20 余处测试），是本计划最大的一次机械改动。若想减小单次提交体积，可先加 `RouteProxyTransportPlan` 并保留 `RouteProxyTransport` 作为 `#[deprecated]` 转换层，分两次提交完成迁移。
