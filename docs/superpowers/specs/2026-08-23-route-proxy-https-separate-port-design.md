# 本地算力池 HTTPS 独立端口设计

## 背景

现在启用本地算力池 HTTPS 是**把唯一那个监听端口从 HTTP 换成 HTTPS**，客户端配置里写入的
`base_url` 随之变成 `https://`。这条路径有一个结构性问题：它强迫**所有**客户端走 HTTPS，
而其中相当一部分根本读不到本地根证书。

根证书只装在操作系统的信任库里（Windows 的当前用户 Root 存储、macOS 的 login keychain）。
自带 CA bundle 的客户端看不到那里：

- macOS 与 Linux 的 curl 走 LibreSSL/OpenSSL，带自己的证书源，不读 login keychain。实测报
  `curl: (60) SSL certificate problem: unable to get local issuer certificate`，即 OpenSSL 的
  verify error 20：找不到签发者。
- Node 用内置的 Mozilla 根列表，既不读 Windows 存储也不读 macOS 钥匙串。因此 Node 版的
  Claude Code 被指向 `https://127.0.0.1:<port>` 时会以 `UNABLE_TO_VERIFY_LEAF_SIGNATURE`
  失败。仓库中搜不到 `NODE_EXTRA_CA_CERTS`，没有为这类客户端提供根证书路径的途径。

也就是说：**用户一旦开启 HTTPS，这些客户端就直接不可用**，而且失败信息指向证书，很难联想到
是"开关"导致的。

## 目标

让 HTTPS 成为**增量能力**而不是替换：需要它的场景用它，读不到本地根证书的客户端继续走 HTTP，
两者同时可用。同时不改变已经启用 HTTPS 的用户的行为。

## 非目标

- 不解决"如何把根证书送到各客户端的信任库"。为客户端配置写入 `NODE_EXTRA_CA_CERTS` /
  `SSL_CERT_FILE` 是另一个独立功能，不在本设计范围。
- 不改变 `BIND_HOST`。两个监听器都只听 `127.0.0.1`。
- 不引入用户自定义端口。见"已考虑但排除的方案"。

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 客户端配置写哪个地址 | **HTTP** | 所有客户端照常工作；HTTPS 端点留给愿意处理根证书的场景手动填 |
| 是否保留原「替换」模式 | **保留**，新复选框默认开 | 默认路径变正确，老用户零感知 |
| HTTPS 端口来源 | **`HTTP 端口 + 1`**，被占用则继续上扫 | 常态即 `19527`/`19528`，相邻好记；复用现有"固定起点 + 上扫"逻辑，不新增机制 |
| 双监听器实现 | 运行时持两个 `ProxyListener` | 一把锁、一次原子启停，且允许 HTTPS 失败而 HTTP 存活 |

## 配置与迁移

`src-tauri/src/models/route_proxy_https.rs`：

```rust
pub struct RouteProxyHttpsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub separate_port: bool,
}

impl Default for RouteProxyHttpsConfig {
    fn default() -> Self {
        Self { enabled: false, auto_start: false, separate_port: true }
    }
}
```

字段默认（`bool::default()` 即 `false`）与结构体默认（`true`）刻意不一致，这个不对称本身就是
迁移逻辑，不需要额外代码：

| 情形 | 走哪条路 | `separate_port` | 效果 |
|---|---|---|---|
| 配置文件不存在（新装） | `Default::default()` | `true` | 新用户默认并存模式 |
| 文件存在但无该字段（老用户） | `#[serde(default)]` | `false` | 保持替换模式，其客户端配置里的 `https://` 仍有效 |
| 显式 `false` | serde 读到 | `false` | 尊重用户选择 |

**不照抄隔壁 `auto_start` 的原始 JSON 探测**（`load_config` 中的
`raw.get("autoStart").is_none()`）：那里的迁移值是计算出来的（`config.auto_start =
config.enabled`），字段默认值表达不了；本字段要的是常量，serde 默认足够。

## 双监听器

`src-tauri/src/services/route_proxy_service.rs` 把现在平铺的四个字段收进一个小结构：

```rust
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
    /// 并存模式下 HTTPS 未能启动的原因；此时 HTTP 仍在服务。
    https_error: Option<String>,
}
```

原有的 `running: bool` 删除，改为从 `http.is_some() || https.is_some()` 推导，消掉一个可能与
现实不一致的状态。

传输方式由二选一改为三态，配置到行为的映射穷举无遗漏：

```rust
pub enum RouteProxyTransportPlan {
    HttpOnly,                     // enabled = false
    HttpsOnly { cert, key },      // enabled = true,  separate_port = false（现有行为）
    HttpAndHttps { cert, key },   // enabled = true,  separate_port = true （新增）
}
```

`RouteProxyHttpsService::transport()` 即这张真值表的实现。不用 `Vec<RouteProxyTransport>`：
那会允许"两个 HTTPS"这类无意义组合。

启动顺序：

1. 按现有规则从 `19527` 起上扫绑 HTTP（`HttpsOnly` 跳过此步）。
2. 需要 HTTPS 时，从 `http_port + 1` 起上扫，**同一个 axum app** 再服务一遍。`ProxyAppState`
   是 `Clone`，两个监听器共享同一份凭据池缓存与活动登记，不会出现两套计数。
3. `stop()` 依次关闭两者，两个 `shutdown` 通道各发一次。

## 状态与数据流

`RouteProxyStatus` 新增 `https_port`、`https_base_url`、`https_error` 三个可选字段。

核心不变量：**`base_url` 恒为"客户端该用的地址"**

| plan | `base_url` | `https_base_url` |
|---|---|---|
| `HttpOnly` | `http://127.0.0.1:19527` | — |
| `HttpsOnly` | `https://127.0.0.1:19527` | — |
| `HttpAndHttps` | `http://127.0.0.1:19527` | `https://127.0.0.1:19528` |

这条不变量使下列消费方**无需改动**：`route_config_service`（写客户端配置）、
`route_model_test_proxy_base_url`（`src-tauri/src/web/handlers/mod.rs`）、前端端点显示。

`RouteProxyHttpsStatus.proxy_base_url` 语义收紧为"HTTPS 端点（若有）"：并存模式取
`https_base_url`，替换模式取 `base_url`（那本就是 https）。对老用户无行为变化。

## 界面

`src/components/settings/route-proxy-https-settings.tsx`：

- 新增「启用独立端口」复选框，绑定 `separatePort`，沿用现有 `enabled` 复选框的写法与
  `isMutating` 禁用逻辑。
- 并存模式下**同时显示 HTTP 与 HTTPS 两个端点**，否则用户无法判断客户端实际在用哪个。
- 端点需可选中复制：它不再写入客户端配置，必须手动粘贴到 curl 或客户端。
- `https_error` 复用现有琥珀色告示区渲染。
- i18n 中英两份都要补（`src/lib/i18n.tsx` 的 `settings.https.*`）。

新增 Tauri 命令 `set_route_proxy_https_separate_port`，流程与 `enable`/`disable` 一致：存配置
→ 按新 plan 重启代理 → 返回 outcome。

**HTTPS 关闭时该复选框的语义**：值照常保存，但**不重启代理**——`enabled = false` 时两种取值
都映射到 `HttpOnly`，plan 没有变化，重启纯属多余。复选框保持可点，用户可以先配好、之后开启
HTTPS 时即生效；不采用"禁用控件"，因为那会让人以为设置没被记住。

**必须接上的联动**：切换该复选框使 `base_url` 在 `http://` 与 `https://` 间变化，**已写入的
客户端配置随之失效**。新命令须触发仓库现有的"配置变更后提示需重新写入客户端配置"机制，不能让
用户配置默默失配。仅当 `enabled = true`（即 plan 真的变了）时才需要该提示。

## 错误处理

- `HttpAndHttps` 下 HTTPS 未能启动（端口绑不上、证书读不了）：记入 `https_error`，HTTP 继续
  服务，`start()` 返回 `Ok`。**这是本功能的核心保证** —— 证书问题不得再拖垮可用的那条路。
- `HttpsOnly` 下失败仍然致命：没有备用监听器，返回 `Ok` 却无人监听是撒谎。
- 端口耗尽由现有 `bind_route_proxy_listener_from` 处理（扫至 `u16::MAX` 后带详情报错）。
- `regenerate_certificates` 现有的失败回滚按同样方式用旧材料重建 plan，无需特殊分支。

## 测试

Rust（同文件内联 `#[cfg(test)] mod tests`，符合仓库惯例）：

1. 配置迁移三态：无文件 → `true`；有文件无字段 → `false`；显式 `false` → `false`。
2. `transport()` 真值表：三种配置组合各产出对应 plan 变体。
3. `base_url` 不变量：并存模式为 http，替换模式为 https。
4. 双监听器实起：两个端口都能应答，且 `https_port == http_port + 1`。断言是**相对**的，不
   假定 HTTP 落在哪个端口——服务级测试本就如此容忍端口（开发机上应用本体常占着 19527）。
5. **HTTPS 非致命**：证书文件缺失时并存模式仍返回 `Ok`、HTTP 可用、`https_error` 有值。
6. 相邻端口规则：直接测 `bind_route_proxy_listener_from(P)`——先占住 `P`，断言其返回 `P + 1`。
   这是上扫规则的本体，不必绕过服务去控制端口。

前端（vitest）：复选框渲染与切换调用命令、并存模式下两个端点均显示、`https_error` 渲染为
警告。

## 已考虑但排除的方案

**用户自定义 HTTPS 端口**：需要输入框加一套校验（范围、占用、不得与 HTTP 端口相同），而绝大
多数用户不会修改。当前设计不阻碍后续添加：端口来源集中在一处计算，届时替换即可。

**按客户端类型决定写 HTTP 还是 HTTPS**：更贴合实际，但需要为每个平台维护一份"能否读系统信任
库"的判断，而该判断随客户端版本变化（如 Node 22+ 新增 `--use-system-ca`）。维护成本高于收益。

**一个 task 内用 `tokio::join!` 跑两个 server**：`RouteProxyInner` 改动更小，但两个 server 同
处一个 task，一个失败会带走另一个，直接违背"HTTPS 失败不影响 HTTP"这条核心要求。

**为 HTTPS 监听器单开一个 service**：隔离最彻底，但要复制整套生命周期逻辑，且"代理是否运行"
会有两个真相来源，容易出现一个在跑一个没跑的中间态。对该规模的功能不划算。
