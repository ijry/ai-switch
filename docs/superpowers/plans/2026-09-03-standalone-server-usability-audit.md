# 独立服务器（ai-switch-server）可用性修复计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让从 GitHub Release 下载 `ai-switch-server` 的用户解压即能在浏览器里正常打开并使用，且默认配置不泄露凭据。

**Architecture:** 三条独立的修复线。一是发布物：`release.yml` 的 server 压缩包只打了一个裸二进制，前端资源根本没进包，浏览器打开拿到的是一段 JSON 404——这是"正常打开都报错"的直接原因。二是失败可诊断性：静态资源缺失时服务端不告警、不给可读页面，且把缺失的 JS 资源用 `index.html` 兜底，让浏览器报出与真实原因无关的 MIME 错误。三是独立服务器与桌面端的行为落差：空令牌等于完全不鉴权、路由代理不自启、无优雅停机，以及前端若干桌面专属入口在浏览器里静默抛异常。

**Tech Stack:** Rust（axum 0.7 / axum-server / sqlx）、React 18 + TanStack Query、Vitest、GitHub Actions（pwsh）。

**Spec:** 本文件「审计结论」一节即规格依据；每条结论都附了 2026-09-03 在 `src-tauri/target-codex/release/ai-switch-server.exe` 上实测得到的证据。

## Global Constraints

- AI 侧执行任何 cargo 命令必须在 `src-tauri` 工作目录下带 `CARGO_TARGET_DIR=target-codex`（`AGENTS.md:6-10`）。禁止新建第三个 target 目录。
- 跑 cargo 前需确认两个产物存在：`src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe` 与仓库根 `dist/`，否则 `build.rs` 报与代码无关的 `resource path` 错误。
- 前端检查命令：`pnpm typecheck`、`pnpm test:run`。Rust 检查命令：`pnpm rust:check`、`pnpm rust:test`。
- 不得修改 `src-tauri/migrations/` 下任何已发布的 `.sql`：sqlx 按文件字节做 SHA-384，改动会让已升级用户的账号列表被隔离。
- 命令名与参数名的双通道契约由 `tests/transport/command-contract.test.ts` 守卫，新增命令必须同时登记到 `src-tauri/src/lib.rs` 的 `generate_handler!` 与 `src-tauri/src/web/handlers/mod.rs` 的 `dispatch_command`，否则该测试失败。
- 文档默认中文；`docs-site/docs/en/**` 是对应英文版，改中文页必须同步改英文页。

---

## 审计结论

实测环境：`pnpm build` 产出 `dist/`，`cargo build --release --bin ai-switch-server`，把二进制单独放进空目录后启动（完全模拟解压发布包），`AI_SWITCH_HOST=127.0.0.1`。

| # | 严重度 | 现象 | 实测证据 | 根因位置 |
| --- | --- | --- | --- | --- |
| 1 | 阻断 | 浏览器打开首页得到一段 JSON：`{"code":"web.error","message":"AI Switch web assets not found",...}` | `GET /` → `404`，`content-type: application/json` | `.github/workflows/release.yml:251-254` 只把 `$env:SERVER_BIN` 这一个文件压进 zip；而桌面安装包靠 `src-tauri/tauri.conf.json` 的 `"../dist": "web/"` 资源映射才有前端 |
| 2 | 阻断 | 服务端对此毫无提示，启动只打印一行 `listening on ...` | 同一次启动的完整 stdout 就一行 | `src-tauri/src/server.rs:228` 拿到的是 `resolve_static_dir()` 的兜底值 `PathBuf::from("web")`，没有人检查它是否真的存在 |
| 3 | 高 | 部署残缺时浏览器报的是 MIME 错误，不指向真实原因 | `GET /assets/index-DOESNOTEXIST.js` → `200 text/html`（返回了 `index.html`） | `src-tauri/src/web/static_assets.rs:76-77` 对任何未命中路径都做 SPA 兜底，不区分资源请求 |
| 4 | 高（安全） | 不设 `AI_SWITCH_TOKEN` 时，无任何凭据即可读出全部账号的明文 API Key | 无 `Authorization` 头 `POST /api/list_route_credentials` → `200`，`secret_payload_json` 内含 `"api_key": "sk-…"` | `src-tauri/src/web/auth.rs:30-32` 与 `:41-43` 把"令牌为空"当作"放行"；`src-tauri/src/server.rs:218` 允许令牌为空且不告警。同一份数据的 `export_route_credentials` 是敏感命令，同一次实测返回 401——闸门被普通命令绕过了 |
| 5 | 中 | 独立服务器重启后路由代理不会自己起来 | 启动后 `get_route_proxy_status` → `{"running":false}` | 桌面端 `src-tauri/src/lib.rs:417-429` 会按 `route-proxy-https.json` 的 `auto_start` 恢复代理，`server.rs:256-263` 只搬了 `RouteRecoveryService`，漏了这一段 |
| 6 | 中 | 代理未运行时每个打开的标签页每秒发一次请求，永不停止 | `src/screens/AccountsScreen.tsx:2755` `refetchInterval: (query) => (query.state.data?.running ? false : 1000)` | 同上；与 #5 叠加后成为常态 |
| 7 | 中 | 无优雅停机：进程退出不回收 PTY 子进程，也不关 tailscale sidecar | `server.rs:306-308` 直接 `axum::serve(...).await`；`terminals.kill_all()` 只挂在 Tauri 的 `RunEvent::Exit` 上 | `src-tauri/src/server.rs:289-309` |
| 8 | 中 | 浏览器里点「选择账号 JSON 文件」等按钮无任何反应，控制台一条 unhandled rejection | `open()` 来自 `@tauri-apps/plugin-dialog`，浏览器里 `window.__TAURI_INTERNALS__` 不存在 | 无 `isDesktop()` 守卫也无 try/catch：`src/screens/AccountsScreen.tsx:4642`、`:4658`、`src/screens/VibeScreen.tsx:1361`；`src/screens/UpdatesScreen.tsx:61`、`:116` 有 try/catch 但把原始 JS 报错直接显示给用户；`src/screens/SessionsScreen.tsx:311` 未检查 `navigator.clipboard` 是否存在（明文 HTTP 的非环回来源不是 secure context） |
| 9 | 低 | 事件 WebSocket 断线后不重连，实时活动与状态从此静默停更 | `src/lib/transport/web-transport.ts:152-154` `onclose` 只把 socket 置空，没有 `onerror`，也没有退避重连；`:142` 的 `JSON.parse` 无保护 | 同处 |
| 10 | 低 | 音频资源以 `application/octet-stream` 下发 | `GET /assets/hologram-tap-*.wav` → `application/octet-stream` | `src-tauri/src/web/router.rs:188-211` 的 `content_type_for` 没有 `wav`/`mp3`/`ogg` 分支 |
| 11 | 低（文档） | `README.md:158` 声明的 `AI_SWITCH_DATA_DIR` 在代码里不存在 | 全仓库无该环境变量的读取点 | `README.md:158`；`docs-site/docs/deploy/standalone-server.md:62` 已正确指出它未实现，两处自相矛盾 |

已确认**不是**问题的部分，避免后续重复排查：命令名在三个通道（`client.ts` / `generate_handler!` / `dispatch_command`）完全对齐，仅 `open_route_proxy_https_certificate_dir`、`open_session_terminal`、`save_route_credential_export` 三个桌面专属命令按设计缺席，且已由 `src/lib/api/commandSupport.ts` 与契约测试守住；参数名两侧一致（`e8a5c7b` 已修）；无任何 Tauri 模块在 import 期抛错，所以浏览器里首屏 JS 不会整体崩掉；令牌正确时实测 24 个只读命令全部 `200`；`/ws/events` 无令牌 `401`、带查询令牌 `101`。

### 需要决策的两处（建议已给出，落地前请确认）

- **空令牌行为**：建议改成**拒绝启动**并给出明确报错，与本仓库既有的「非环回监听没有 TLS 就拒绝启动」保持同一种失败姿态。备选是自动生成随机令牌打印到 stdout，但那样每次重启令牌都变，浏览器里存的旧令牌全部失效。计划按「拒绝启动」写。
- **凭据明文是否继续随列表下发**：本计划只堵鉴权口子（令牌为空不再等于放行），不改列表返回体。彻底的做法是列表里掩码 `secret_payload_json`、编辑弹窗改用敏感命令 `get_route_credential` 单独取全量，但那会牵动 `AccountsScreen` 的编辑链路，且 `list_route_credentials` 若直接进敏感名单，会让桌面端「局域网 HTTP 访问」这一模式下的账号列表整体 404。建议作为独立后续项。

---

## Task 1: 发布包携带前端资源与 sidecar

对应审计 #1、#11。这是唯一一个不改一行运行时代码就能让"打开就报错"消失的修复，先做。

**Files:**
- Modify: `.github/workflows/release.yml:249-254`
- Modify: `README.md:155-161`
- Modify: `docs-site/docs/deploy/standalone-server.md:35-42`
- Modify: `docs-site/docs/en/deploy/standalone-server.md:35-42`
- Test: `tests/ReleaseWorkflow.test.ts`

**Interfaces:**
- Consumes: 工作流已有的 `$env:SERVER_BIN`、`$env:SIDECAR_BIN`、`$env:EXE_SUFFIX`、`$env:UPDATER_PLATFORM`、`$dest`、`$tag`（均在 `Compute platform variables` 步骤里写入 `GITHUB_ENV`），以及 `Build frontend assets` 步骤产出的仓库根 `dist/`。
- Produces: 发布物 `ai-switch-server_<tag>_<platform>.zip` 的内部结构变为 `ai-switch-server[.exe]` + `ai-switch-tsnet[.exe]` + `web/`（`web/index.html` 必须存在）。后续任务不依赖它。

- [ ] **Step 1: 写失败的测试**

在 `tests/ReleaseWorkflow.test.ts` 的 `describe` 内追加一个新的 `describe` 块。该文件已有 `readFileSync` 读取工作流的写法，沿用它：

```ts
describe("standalone server release archive", () => {
  const workflow = readFileSync(resolve(process.cwd(), ".github/workflows/release.yml"), "utf8");

  it("stages the frontend bundle next to the server binary", () => {
    // resolve_static_dir() 只认可执行文件同级的 web/ ，裸二进制在浏览器里必然 404。
    expect(workflow).toContain('Copy-Item dist (Join-Path $serverStage "web") -Recurse');
    expect(workflow).toContain('Compress-Archive -Path "$serverStage/*"');
  });

  it("fails the build instead of shipping a server archive without the UI", () => {
    expect(workflow).toContain('Join-Path $serverStage "web/index.html"');
    expect(workflow).toContain('throw "Server bundle is missing web/index.html"');
  });

  it("ships the tailscale sidecar under the name the server looks for", () => {
    expect(workflow).toContain('"ai-switch-tsnet$env:EXE_SUFFIX"');
  });
});
```

`readFileSync` 与 `resolve` 是否已在文件头 import，先确认；缺哪个补哪个。

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run tests/ReleaseWorkflow.test.ts`
Expected: 三条新用例全部 FAIL（当前工作流里没有 `$serverStage`）。

- [ ] **Step 3: 改工作流**

把 `.github/workflows/release.yml` 里这四行

```yaml
          $serverArchive = Join-Path $dest "ai-switch-server_${tag}_$env:UPDATER_PLATFORM.zip"
          $sidecarArchive = Join-Path $dest "ai-switch-tsnet_${tag}_$env:UPDATER_PLATFORM.zip"
          Compress-Archive -Path $env:SERVER_BIN -DestinationPath $serverArchive -Force
          Compress-Archive -Path $env:SIDECAR_BIN -DestinationPath $sidecarArchive -Force
```

替换为：

```yaml
          # 独立服务器没有 Tauri 的资源映射，前端必须随包走：resolve_static_dir()
          # 找的是可执行文件同级的 web/ ，只打二进制的话浏览器首页就是一段 JSON 404。
          $serverStage = "server-bundle"
          if (Test-Path $serverStage) { Remove-Item -Recurse -Force $serverStage }
          New-Item -ItemType Directory -Force $serverStage | Out-Null
          Copy-Item $env:SERVER_BIN (Join-Path $serverStage (Split-Path $env:SERVER_BIN -Leaf))
          Copy-Item $env:SIDECAR_BIN (Join-Path $serverStage "ai-switch-tsnet$env:EXE_SUFFIX")
          Copy-Item dist (Join-Path $serverStage "web") -Recurse
          if (-not (Test-Path (Join-Path $serverStage "web/index.html"))) {
            throw "Server bundle is missing web/index.html"
          }

          $serverArchive = Join-Path $dest "ai-switch-server_${tag}_$env:UPDATER_PLATFORM.zip"
          $sidecarArchive = Join-Path $dest "ai-switch-tsnet_${tag}_$env:UPDATER_PLATFORM.zip"
          Compress-Archive -Path "$serverStage/*" -DestinationPath $serverArchive -Force
          Compress-Archive -Path $env:SIDECAR_BIN -DestinationPath $sidecarArchive -Force
```

单独的 `ai-switch-tsnet_*.zip` 保留不动：桌面端用户可能已经在按它升级 sidecar。

- [ ] **Step 4: 运行测试确认通过**

Run: `pnpm vitest run tests/ReleaseWorkflow.test.ts`
Expected: PASS（含该文件原有的 5 条发布说明用例）。

- [ ] **Step 5: 同步文档**

`README.md` 的环境变量清单里删掉不存在的一项，并说明发布包结构。把 `README.md:157-161` 那一段改为：

```markdown
- `AI_SWITCH_TOKEN` required for API and WebSocket access; the server refuses to start without it
- `AI_SWITCH_STATIC_DIR` frontend `dist` directory for browser UI (only needed if you moved it)

The release archive `ai-switch-server_<tag>_<platform>.zip` already contains the
binary, the Tailscale sidecar and a sibling `web/` directory, so unzip-and-run
serves the browser UI with no extra configuration. Installed desktop builds ship
the same assets under `web/` next to the executable.
```

在 `docs-site/docs/deploy/standalone-server.md` 的「构建」一节末尾（第 42 行「如果不想自己编译…」那句之后）补一段：

````markdown
发布包解压后的结构就是下面「前端资源怎么找」推荐的布局，`ai-switch-server`、`ai-switch-tsnet` 与 `web/` 已经放在一起，不需要再设 `AI_SWITCH_STATIC_DIR`：

```text
ai-switch-server_v0.7.3_windows-x86_64/
├── ai-switch-server.exe
├── ai-switch-tsnet.exe
└── web/
    ├── index.html
    └── assets/...
```
````

`docs-site/docs/en/deploy/standalone-server.md` 同位置补对应英文段落。

- [ ] **Step 6: 提交**

```bash
git add .github/workflows/release.yml tests/ReleaseWorkflow.test.ts README.md docs-site/docs/deploy/standalone-server.md docs-site/docs/en/deploy/standalone-server.md
git commit -m "fix: 独立服务器发布包携带前端资源与 sidecar"
```

---

## Task 2: 前端资源缺失时说人话

对应审计 #2、#3、#10。Task 1 修好的是发布物，这一条修的是"用户自己搭错了"时的可诊断性——包括手动部署、`AI_SWITCH_STATIC_DIR` 指错、以及 `web/` 与二进制版本不匹配这三种情况。

**Files:**
- Modify: `src-tauri/src/web/static_assets.rs:3-19`（新增 `locate_static_dir`、`static_bundle_present`、`looks_like_asset_request`，改 `resolve_static_file`）
- Modify: `src-tauri/src/web/router.rs:165-211`（`static_fallback` 与 `content_type_for`）
- Modify: `src-tauri/src/server.rs:228`（启动告警）
- Test: `src-tauri/src/web/static_assets.rs` 的 `mod tests`、`src-tauri/src/web/router.rs` 的 `mod tests`

**Interfaces:**
- Consumes: 现有 `resolve_static_dir() -> PathBuf`（签名保持不变，`WebService::start` 与 `run_from_env` 都在用）。
- Produces:
  - `pub fn locate_static_dir() -> Option<PathBuf>`——命中才返回 `Some`。
  - `pub fn static_bundle_present(dir: &Path) -> bool`——目录里有 `index.html`。
  - `pub fn static_dir_candidates_report() -> String`——多行文本，列出按顺序尝试过的候选路径，供启动日志使用。
  - `resolve_static_file` 语义收窄：命中文件返回 `Some(路径)`；**带扩展名**且未命中的请求返回 `None`（不再 SPA 兜底）；其余未命中返回 `Some(index.html)`。

- [ ] **Step 1: 写失败的测试（静态资源解析）**

在 `src-tauri/src/web/static_assets.rs` 的 `mod tests` 里追加：

```rust
    #[test]
    fn missing_asset_requests_do_not_fall_back_to_index() {
        let dir = tempdir().unwrap();
        let web = dir.path().join("web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("index.html"), "<html></html>").unwrap();

        // 用 index.html 回答一个 .js 请求，浏览器只会报 MIME 错误，
        // 完全遮住"部署残缺"这个真实原因。
        assert!(resolve_static_file(&web, "/assets/index-missing.js").is_none());
        assert!(resolve_static_file(&web, "/assets/app.css").is_none());
        // 客户端路由没有扩展名，仍然要兜底。
        assert!(resolve_static_file(&web, "/settings/web").unwrap().ends_with("index.html"));
        assert!(resolve_static_file(&web, "/index.html").unwrap().ends_with("index.html"));
    }

    #[test]
    fn bundle_presence_and_candidate_report_are_observable() {
        let dir = tempdir().unwrap();
        assert!(!static_bundle_present(dir.path()));
        fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
        assert!(static_bundle_present(dir.path()));
        assert!(static_dir_candidates_report().contains("AI_SWITCH_STATIC_DIR"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run（工作目录 `src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib web::static_assets`
Expected: 编译失败，`cannot find function static_bundle_present`。

- [ ] **Step 3: 改 static_assets.rs**

把 `resolve_static_dir` 拆成"定位"与"兜底"两步，并新增三个查询函数：

```rust
/// 按顺序找出第一个真的含 `index.html` 的目录。没有就是 None——调用方据此
/// 决定是告警还是继续。
pub fn locate_static_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("AI_SWITCH_STATIC_DIR") {
        let path = PathBuf::from(value);
        if has_index(&path) {
            return Some(path);
        }
    }

    candidate_static_dirs()
        .into_iter()
        .find(|candidate| has_index(candidate))
}

pub fn resolve_static_dir() -> PathBuf {
    // 兜底值是相对路径，只有在进程工作目录下恰好有 web/ 时才会命中；
    // 它的存在是为了让路由层拿到一个 PathBuf，不代表资源真的在。
    locate_static_dir().unwrap_or_else(|| PathBuf::from("web"))
}

pub fn static_bundle_present(dir: &Path) -> bool {
    has_index(dir)
}

/// 启动日志用：把试过的候选路径原样列出来，用户一眼能看出该往哪放。
pub fn static_dir_candidates_report() -> String {
    let mut lines = vec![format!(
        "  AI_SWITCH_STATIC_DIR = {}",
        std::env::var("AI_SWITCH_STATIC_DIR").unwrap_or_else(|_| "<unset>".to_string())
    )];
    for candidate in candidate_static_dirs() {
        lines.push(format!("  {}", candidate.display()));
    }
    lines.join("\n")
}
```

`resolve_static_file` 的 SPA 兜底改成只对无扩展名（或 `.html`）的路径生效，并新增判定函数：

```rust
    // 带扩展名的请求是资源请求，不是客户端路由。用 index.html 回答它会让
    // 浏览器抛一个与真实原因（部署残缺／版本不匹配）无关的 MIME 错误。
    if looks_like_asset_request(trimmed) {
        return None;
    }

    Some(static_root.join("index.html"))
}

fn looks_like_asset_request(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(_, extension)| !extension.eq_ignore_ascii_case("html"))
}
```

- [ ] **Step 4: 写路由层的失败测试**

在 `src-tauri/src/web/router.rs` 的 `mod tests` 里追加。注意 `spawn_test_router` 用的 `temp.path()` 里没有 `index.html`，正好就是"资源缺失"场景：

```rust
    #[tokio::test]
    async fn missing_web_assets_answer_with_a_readable_page() {
        let (address, server) = spawn_test_router(true).await;
        let response = reqwest::get(format!("http://{address}/")).await.unwrap();

        // 浏览器里一段 JSON 什么也说明不了；至少要是一页能读的 HTML。
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/html; charset=utf-8")
        );
        let body = response.text().await.unwrap();
        assert!(body.contains("AI_SWITCH_STATIC_DIR"));
        // 未鉴权的调用方不该拿到服务器上的真实路径。
        assert!(!body.contains(&address.to_string()));
        server.abort();
    }

    #[tokio::test]
    async fn audio_assets_get_a_playable_content_type() {
        assert_eq!(
            content_type_for(std::path::Path::new("beep.wav")),
            "audio/wav"
        );
        assert_eq!(
            content_type_for(std::path::Path::new("beep.mp3")),
            "audio/mpeg"
        );
    }
```

- [ ] **Step 5: 运行确认失败**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib web::router`
Expected: `missing_web_assets_answer_with_a_readable_page` FAIL（当前返回 404 + JSON），`audio_assets_get_a_playable_content_type` FAIL（当前是 `application/octet-stream`）。

- [ ] **Step 6: 改 router.rs**

`static_fallback` 区分"整包没部署"和"单个资源找不到"：

```rust
async fn static_fallback(State(context): State<WebServerContext>, uri: Uri) -> Response {
    let Some(file_path) = resolve_static_file(&context.static_dir, uri.path()) else {
        if static_bundle_present(&context.static_dir) {
            return error_response(StatusCode::NOT_FOUND, "AI Switch web asset not found");
        }
        return missing_assets_page();
    };
    // ……以下读文件与响应构造保持原样
```

新增页面函数（放在 `content_type_for` 之前）。**故意不输出任何服务器路径**：这个响应对未鉴权的调用方也可见，具体候选路径只打到启动日志里：

```rust
/// 整个前端包都没找到。这个响应会被人用浏览器看到，所以给一页能读的 HTML，
/// 而不是一段 JSON。
fn missing_assets_page() -> Response {
    let body = concat!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
        "<title>AI Switch — web assets missing</title></head><body>",
        "<h1>AI Switch web assets are not installed</h1>",
        "<p>The API is running, but the browser UI was not found next to this server.</p>",
        "<p>Put the frontend build in a <code>web/</code> directory beside the ",
        "executable, or point <code>AI_SWITCH_STATIC_DIR</code> at a directory that ",
        "contains <code>index.html</code>. The release archive ships that directory ",
        "already; see the standalone-server deployment guide.</p>",
        "<p>The server log printed every path it tried at startup.</p>",
        "</body></html>"
    );
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| error_response(StatusCode::SERVICE_UNAVAILABLE, "web assets missing"))
}
```

`content_type_for` 的 `match` 里补三个分支（前端 `dist/assets` 目前只有 `.wav`，另两个是为将来的皮肤音频留的）：

```rust
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
```

`use` 一行改为 `use crate::web::static_assets::{resolve_static_file, static_bundle_present};`。

- [ ] **Step 7: 启动时告警**

`src-tauri/src/server.rs` 把第 228 行 `let static_dir = resolve_static_dir();` 换成：

```rust
    let static_dir = match locate_static_dir() {
        Some(dir) => {
            println!("Serving AI Switch web assets from {}", dir.display());
            dir
        }
        None => {
            // 只打一行 "listening on ..." 的话，用户在浏览器里看到 404
            // 完全无从判断是自己少放了文件还是服务坏了。
            eprintln!(
                "WARNING: no AI Switch web assets found; the browser UI will not load.\n\
                 The HTTP API still works. Paths tried, in order:\n{}",
                static_dir_candidates_report()
            );
            resolve_static_dir()
        }
    };
```

`use crate::web::static_assets::resolve_static_dir;` 改为 `use crate::web::static_assets::{locate_static_dir, resolve_static_dir, static_dir_candidates_report};`。

- [ ] **Step 8: 运行确认通过**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib web::`
Expected: PASS。
再跑 `CARGO_TARGET_DIR=target-codex cargo test` 确认没有别处依赖旧的 SPA 兜底语义。

- [ ] **Step 9: 手动验收**

```bash
# 仓库根
pnpm build
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo build --release --bin ai-switch-server && cd ..
mkdir -p /tmp/aisw-check && cp src-tauri/target-codex/release/ai-switch-server.exe /tmp/aisw-check/
cd /tmp/aisw-check && AI_SWITCH_TOKEN=0123456789abcdef AI_SWITCH_PORT=13093 ./ai-switch-server.exe
```

Expected: stdout 出现 `WARNING: no AI Switch web assets found`，并列出候选路径；`curl -i http://127.0.0.1:13093/` 返回 `503` + `text/html`。随后 `cp -r <repo>/dist /tmp/aisw-check/web` 并重启，改为打印 `Serving AI Switch web assets from ...`，`GET /` 返回 200 HTML，`GET /assets/index-DOESNOTEXIST.js` 返回 404（不再是 200 text/html）。收尾 `rm -rf /tmp/aisw-check`。

- [ ] **Step 10: 提交**

```bash
git add src-tauri/src/web/static_assets.rs src-tauri/src/web/router.rs src-tauri/src/server.rs
git commit -m "fix: 前端资源缺失时给出启动告警与可读错误页"
```

---

## Task 3: 空令牌不再等于放行

对应审计 #4。当前 `AI_SWITCH_TOKEN` 未设置时，任何人无凭据 `POST /api/list_route_credentials` 就能读到全部账号的明文 API Key，而同一份数据的 `export_route_credentials` 是敏感命令、会被拒——闸门被普通命令绕开了。两处同时改：鉴权层不再把"空令牌"解释成"放行"，启动层直接拒绝空令牌。

**Files:**
- Modify: `src-tauri/src/web/auth.rs:29-51`（`is_authorized`、`is_query_token_authorized`）、`:111-117`（`authorize_api_request`）
- Modify: `src-tauri/src/server.rs:218`（新增 `resolve_server_token`）
- Modify: `src-tauri/src/services/web_service.rs:687-693`（`validate_start_config`）
- Modify: `docs-site/docs/deploy/standalone-server.md:52,60,148`、`docs-site/docs/en/deploy/standalone-server.md` 同位置
- Test: `src-tauri/src/web/auth.rs` 的 `mod tests`、`src-tauri/src/web/router.rs` 的 `mod tests`、`src-tauri/src/server.rs` 的 `mod tests`

**Interfaces:**
- Consumes: `is_sensitive_command`（`src-tauri/src/web/handlers/mod.rs:58`）、`AppError::Validation`。
- Produces: `pub(crate) fn resolve_server_token(raw: Option<String>) -> Result<String, AppError>`——去空白后为空或短于 16 个字符即报错。`is_authorized` / `is_query_token_authorized` 语义反转：空令牌一律 `false`。

- [ ] **Step 1: 写失败的测试（鉴权层）**

`src-tauri/src/web/auth.rs` 的 `mod tests` 追加：

```rust
    #[test]
    fn an_empty_token_authorizes_nobody() {
        // 空令牌曾经等于"放行"，于是未鉴权的 list_route_credentials
        // 直接吐出明文 api_key。失败必须朝关闭的方向。
        assert!(!is_authorized(&HeaderMap::new(), ""));
        assert!(!is_query_token_authorized(Some("token=anything"), ""));
        assert!(!is_query_token_authorized(None, ""));

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer anything"),
        );
        assert!(!is_authorized(&headers, ""));
    }
```

`src-tauri/src/web/router.rs` 的 `mod tests` 追加：

```rust
    #[tokio::test]
    async fn credential_listing_is_never_readable_without_a_token() {
        let (address, server) = spawn_test_router_with_token(true, "").await;
        for command in ["list_route_credentials", "list_route_credentials_page", "get_settings"] {
            let response = reqwest::Client::new()
                .post(format!("http://{address}/api/{command}"))
                .json(&json!({ "platform": "codex" }))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{command}");
        }
        server.abort();
    }
```

- [ ] **Step 2: 运行确认失败**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib web::auth web::router`
Expected: 两条新用例 FAIL——`is_authorized(_, "")` 现在返回 `true`，列表命令现在返回 `200`。

- [ ] **Step 3: 改 auth.rs**

两个判定函数的空令牌分支反转，并去掉中间件里的同义放行条件：

```rust
pub fn is_authorized(headers: &HeaderMap, token: &str) -> bool {
    // 没有配置令牌意味着没有访问控制，不意味着允许访问。普通命令里
    // 就有能读出明文凭据的（list_route_credentials），必须失败关闭。
    if token.is_empty() {
        return false;
    }
    ...  // 其余不变
}

pub fn is_query_token_authorized(query: Option<&str>, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    ...  // 其余不变
}
```

`authorize_api_request` 的判定条件从

```rust
    if primary_authorized || (!sensitive && (auth.primary_token.is_empty() || mobile_authorized)) {
```

改为

```rust
    if primary_authorized || (!sensitive && mobile_authorized) {
```

`primary_authorized` 的 `!auth.primary_token.is_empty() &&` 前缀可以保留（`is_authorized` 已经自己挡住了），但删掉更直白，二选一即可。

- [ ] **Step 4: 写并实现启动期校验**

`src-tauri/src/server.rs` 的 `mod tests` 追加：

```rust
    #[test]
    fn a_usable_token_is_required_before_startup() {
        assert_eq!(
            resolve_server_token(Some("  0123456789abcdef  ".to_string())).unwrap(),
            "0123456789abcdef"
        );
        for raw in [None, Some(String::new()), Some("   ".to_string())] {
            let error = resolve_server_token(raw).unwrap_err();
            assert!(matches!(
                error,
                AppError::Validation { code: "web.token_required", .. }
            ));
        }
        let error = resolve_server_token(Some("short".to_string())).unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation { code: "web.token_too_short", .. }
        ));
    }
```

实现放在 `server.rs` 的 `normalize_tls_paths` 之后：

```rust
/// 独立服务器没有第二道访问控制，令牌是唯一的一道。缺了就别启动——
/// 这和"非环回监听没有 TLS 就拒绝启动"是同一种失败姿态。
pub(crate) fn resolve_server_token(raw: Option<String>) -> Result<String, AppError> {
    const MINIMUM_TOKEN_LENGTH: usize = 16;

    let token = raw.unwrap_or_default().trim().to_string();
    if token.is_empty() {
        return Err(AppError::Validation {
            code: "web.token_required",
            message: "AI_SWITCH_TOKEN must be set: it is the only access control the \
                      standalone server has, and ordinary commands can read stored API keys"
                .to_string(),
            details: None,
            recoverable: false,
        });
    }
    if token.chars().count() < MINIMUM_TOKEN_LENGTH {
        return Err(AppError::Validation {
            code: "web.token_too_short",
            message: format!("AI_SWITCH_TOKEN must be at least {MINIMUM_TOKEN_LENGTH} characters"),
            details: None,
            recoverable: false,
        });
    }
    Ok(token)
}
```

`run_from_env` 里第 218 行

```rust
    let token = std::env::var("AI_SWITCH_TOKEN").unwrap_or_default();
```

改为

```rust
    let token = resolve_server_token(std::env::var("AI_SWITCH_TOKEN").ok())
        .map_err(|error| error.to_string())?;
```

放在 `let host = ...` 之后、TLS 校验之前，让配置问题在开任何文件、连任何数据库之前就报出来。

- [ ] **Step 5: 桌面端 Web 服务同样不许空令牌**

`src-tauri/src/services/web_service.rs` 的 `validate_start_config`（第 687 行）加一句校验。默认配置本来就会生成 UUID 令牌（`:633`），所以这只影响手工把 `web-service.json` 的 `token` 清空的情况——那种情况下现在会变成"什么都 401"，不如直接拒绝启动并说清原因：

```rust
fn validate_start_config(
    config: &WebServiceConfig,
) -> Result<Option<(PathBuf, PathBuf)>, AppError> {
    let tls_paths = validate_enabled_tls_paths(config)?;
    validate_sensitive_web_transport(&config.host, config.tls_enabled)?;
    crate::server::resolve_server_token(config.token.clone())?;
    Ok(tls_paths)
}
```

- [ ] **Step 6: 运行确认通过**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test`
Expected: PASS。特别确认 `export_requires_a_configured_bearer_token` 与 `percent_encoded_export_command_cannot_bypass_sensitive_auth` 依然通过（它们本来就断言 401），以及 `web_service` 的启停用例没有依赖空令牌。若某条用例构造的 `WebServiceConfig` 用了空令牌，把它改成一个 16 位以上的假令牌，不要放宽校验。

- [ ] **Step 7: 手动验收**

```bash
cd /tmp/aisw-check   # 沿用 Task 2 的目录，或重新按那里的步骤准备
AI_SWITCH_PORT=13094 ./ai-switch-server.exe; echo "exit=$?"
```

Expected: 打印 `AI_SWITCH_TOKEN must be set: ...`，退出码 `1`，不监听端口。
再带 `AI_SWITCH_TOKEN=0123456789abcdef` 启动，`curl -s -o /dev/null -w '%{http_code}' -X POST -d '{"platform":"codex"}' -H 'Content-Type: application/json' http://127.0.0.1:13094/api/list_route_credentials` 返回 `401`；带上 `-H 'Authorization: Bearer 0123456789abcdef'` 返回 `200`。

- [ ] **Step 8: 同步文档**

`docs-site/docs/deploy/standalone-server.md` 环境变量表里 `AI_SWITCH_TOKEN` 那一行改为：

```markdown
| `AI_SWITCH_TOKEN` | 无 | **是** | 访问令牌，至少 16 个字符。未设置或过短时服务拒绝启动 |
```

第 60 行开头那条「名义上可选、实际上必填」的说明改成：

```markdown
- **`AI_SWITCH_TOKEN` 是必填项。** 未设置、只有空白字符、或短于 16 个字符时服务直接拒绝启动并打印原因。这是有意的：普通命令里就有能读出账号明文 API Key 的（如 `list_route_credentials`），没有令牌等于把凭据库对所有能访问该端口的人开放。
```

第 148 行安全提示里的「一定要设置」改为「必须设置（缺失时服务不会启动）」。`docs-site/docs/en/deploy/standalone-server.md` 同步改三处。

- [ ] **Step 9: 提交**

```bash
git add src-tauri/src/web/auth.rs src-tauri/src/server.rs src-tauri/src/services/web_service.rs src-tauri/src/web/router.rs docs-site/docs/deploy/standalone-server.md docs-site/docs/en/deploy/standalone-server.md
git commit -m "fix: 空令牌不再放行普通命令并在启动时拒绝"
```

---

## Task 4: 独立服务器恢复路由代理自启并优雅停机

对应审计 #5、#7。桌面端 `setup()` 里起了三个后台任务，`run_from_env` 只搬了其中的 `RouteRecoveryService`。缺的那个是路由代理自启——对一台无人值守的服务器来说，重启后代理不起来意味着所有指向它的 CLI 全部失败，而且没人会去点浏览器里的启动按钮。停机侧则是反向的同一个洞：Tauri 靠 `RunEvent::Exit` 回收 PTY 子进程和 tailscale sidecar，独立服务器没有这个钩子。

**Files:**
- Modify: `src-tauri/src/services/route_proxy_https_service.rs`（新增 `restore_auto_started_proxy`）
- Modify: `src-tauri/src/lib.rs:417-429`（改为调用共享函数）
- Modify: `src-tauri/src/server.rs:256-309`（自启 + 优雅停机 + 收尾）
- Modify: `src-tauri/Cargo.toml:29`（tokio 增加 `signal` feature）
- Test: `src-tauri/src/services/route_proxy_https_service.rs` 的 `mod tests`

**Interfaces:**
- Consumes: `RouteProxyHttpsService::load_config(&AppPaths)`、`RouteProxyHttpsService::start_proxy(&AppState)`、`TailscaleService::shutdown(&TailscaleRuntimeState)`、`TerminalManager::kill_all()`（用法见 `src-tauri/src/lib.rs:573-577`）。
- Produces: `pub async fn restore_auto_started_proxy(state: &AppState)`——读配置，`auto_start` 为真则启动代理，失败只打日志不返回错误（启动期不该因为代理起不来而拒绝提供 API）。

- [ ] **Step 1: 写失败的测试**

在 `src-tauri/src/services/route_proxy_https_service.rs` 的 `mod tests` 里追加。该文件已有 `async fn test_state() -> TestState`（第 1397 行，`TestState { _temp, state: AppState }`），直接用它：

```rust
    #[tokio::test]
    async fn auto_start_flag_decides_whether_the_proxy_comes_back() {
        // 无人值守的服务器重启后没人会去点启动按钮；代理不自启就等于
        // 所有指向它的 CLI 全部失败。
        let fixture = test_state().await;
        let state = &fixture.state;

        RouteProxyHttpsService::save_config(
            &state.paths,
            &RouteProxyHttpsConfig { auto_start: false, ..Default::default() },
        )
        .await
        .unwrap();
        restore_auto_started_proxy(state).await;
        assert!(!RouteProxyService::status(&state.route_proxy).await.running);

        RouteProxyHttpsService::save_config(
            &state.paths,
            &RouteProxyHttpsConfig { auto_start: true, ..Default::default() },
        )
        .await
        .unwrap();
        restore_auto_started_proxy(state).await;
        assert!(RouteProxyService::status(&state.route_proxy).await.running);

        RouteProxyService::stop(&state.route_proxy).await.ok();
    }
```

`RouteProxyHttpsConfig` 来自 `crate::models::route_proxy_https`（第 8 行），`save_config` 是 `RouteProxyHttpsService` 的关联函数（第 101 行）。若 `mod tests` 尚未 `use` 到 `RouteProxyService`，补上 `use crate::services::route_proxy_service::RouteProxyService;`。


- [ ] **Step 2: 运行确认失败**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib route_proxy_https`
Expected: 编译失败，`cannot find function restore_auto_started_proxy`。

- [ ] **Step 3: 实现共享的自启函数**

在 `route_proxy_https_service.rs` 的 `impl RouteProxyHttpsService` 之外（模块级）加：

```rust
/// 按持久化的 `auto_start` 恢复路由代理。桌面端 setup 与独立服务器共用一份，
/// 免得再出现"某一条启动路径漏了一个后台任务"这种落差。
pub async fn restore_auto_started_proxy(state: &AppState) {
    let Ok(config) = RouteProxyHttpsService::load_config(&state.paths).await else {
        return;
    };
    if !config.auto_start {
        return;
    }
    if let Err(error) = RouteProxyHttpsService::start_proxy(state).await {
        eprintln!("failed to restore route proxy: {error}");
    }
}
```

`src-tauri/src/lib.rs:417-429` 那个 `tauri::async_runtime::spawn` 块的函数体替换为 `services::route_proxy_https_service::restore_auto_started_proxy(&route_proxy_state).await;`，行为不变但只留一份判定逻辑。

- [ ] **Step 4: 独立服务器调用它并加优雅停机**

`src-tauri/Cargo.toml` 的 tokio features 加 `"signal"`：

```toml
tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "net", "sync", "process", "signal", "time"] }
```

`server.rs` 在现有的 recovery 任务块之后追加自启：

```rust
    // 桌面端在 setup() 里做同一件事；无人值守的服务器更需要它。
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            crate::services::route_proxy_https_service::restore_auto_started_proxy(&state).await;
        });
    }
```

新增停机信号与收尾（放在 `run_from_env` 之前）：

```rust
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Tauri 靠 RunEvent::Exit 回收这些；独立服务器没有那个钩子，退出前得自己来，
/// 否则 PTY 子进程与 tailscale sidecar 会留在系统里。
async fn shutdown_runtime(state: &Arc<AppState>) {
    crate::services::tailscale_service::TailscaleService::shutdown(&state.tailscale).await;
    state.terminals.kill_all();
}
```

最后把两条 serve 分支都接上停机路径。HTTP 分支用 axum 自带的：

```rust
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|error| format!("Server error: {error}"));
        shutdown_runtime(&state).await;
        result
```

HTTPS 分支的 `axum_server` 没有 `with_graceful_shutdown`，用它的 `Handle`：

```rust
        let handle = axum_server::Handle::new();
        {
            let handle = handle.clone();
            tokio::spawn(async move {
                shutdown_signal().await;
                handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
            });
        }
        let result = axum_server::from_tcp_rustls(listener, rustls_config)
            .handle(handle)
            .serve(router.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .map_err(|error| format!("HTTPS server error: {error}"));
        shutdown_runtime(&state).await;
        result
```

注意 `state` 在第 265 行被 `build_router(state, ...)` move 掉了，所以要先 `let shutdown_state = Arc::clone(&state);`（放在 `build_router` 调用之前），后面用 `shutdown_state`。

- [ ] **Step 5: 运行确认通过**

Run（`src-tauri`）：`CARGO_TARGET_DIR=target-codex cargo test --lib route_proxy_https` 然后 `CARGO_TARGET_DIR=target-codex cargo test`
Expected: PASS。

- [ ] **Step 6: 手动验收**

在 `/tmp/aisw-check` 里带令牌启动，另开一个终端：

```bash
curl -s -H 'Authorization: Bearer 0123456789abcdef' -X POST -d '{}' \
  -H 'Content-Type: application/json' http://127.0.0.1:13094/api/get_route_proxy_status
```

Expected: 若 `~/.ai-switch/route-proxy-https.json` 的 `auto_start` 为 `true`，返回 `{"running":true,...}`（改动前恒为 `false`）。随后对服务进程发 Ctrl+C，确认进程正常退出且没有残留的 `ai-switch-tsnet` 进程（`tasklist | grep ai-switch-tsnet` 为空）。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/server.rs src-tauri/src/lib.rs src-tauri/src/services/route_proxy_https_service.rs
git commit -m "fix: 独立服务器恢复路由代理自启并在退出时回收子进程"
```

---

## Task 5: 浏览器里收口桌面专属入口

对应审计 #8。六个调用点在浏览器里必然失败：四个 `@tauri-apps/plugin-dialog` 的 `open()`、更新页的 `check()`/`relaunch()`、以及会话页未做存在性检查的 `navigator.clipboard`。其中三个 `open()` 连 `try/catch` 都没有，用户点了按钮什么都不发生，只在控制台留一条 unhandled rejection。`src/components/skills/SkillsToolbar.tsx:25,64` 是仓库里已有的正确写法（取 `desktop` prop、提前返回、按钮 `disabled`），照它统一。

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:4641-4670`（两个 `open()`）、`:7065`（按钮）、`src/components/accounts/ExternalClientImportPanel.tsx:133,152`（按钮）
- Modify: `src/screens/VibeScreen.tsx:1361`、`:2792`、`:3504`
- Modify: `src/screens/UpdatesScreen.tsx:55-70`、`:110-125`
- Modify: `src/screens/SessionsScreen.tsx:307-314`
- Modify: `src/lib/i18n.tsx`（新增 `common.desktopOnly` 两种语言）
- Test: `tests/AccountsScreen.test.tsx`、`tests/UpdatesScreen.test.tsx`、`tests/SessionsScreen.test.tsx`

**Interfaces:**
- Consumes: `isDesktop()`（`src/lib/transport`）、`useI18n()`。
- Produces: 无跨任务接口；每个改动点自洽。

- [ ] **Step 1: 加 i18n 文案**

`src/lib/i18n.tsx` 英文词表里加一行（挨着现有的 `vibe.errorClipboardUnavailable`，约第 660 行）：

```ts
  "common.desktopOnly": "Available in the desktop app only.",
```

中文词表同位置（约第 1359 行）：

```ts
  "common.desktopOnly": "此功能仅桌面端可用。",
```

- [ ] **Step 2: 写失败的测试**

`tests/UpdatesScreen.test.tsx` 追加（该文件已有 mock `@tauri-apps/plugin-updater` 的写法，沿用；关键是把 `isDesktop` mock 成 `false`）：

```ts
  it("does not call the updater in a browser and says why", async () => {
    // 浏览器里 check() 抛的是 "Cannot read properties of undefined"，
    // 把这句原文塞进错误条对用户毫无意义。
    vi.mocked(isDesktop).mockReturnValue(false);
    render(<UpdatesScreen />);
    const button = screen.getByRole("button", { name: /检查更新|check/i });
    expect(button).toBeDisabled();
    expect(screen.getByText(/仅桌面端可用|desktop app only/i)).toBeInTheDocument();
    expect(check).not.toHaveBeenCalled();
  });
```

`tests/SessionsScreen.test.tsx` 追加：

```ts
  it("reports an unavailable clipboard instead of throwing", async () => {
    const original = navigator.clipboard;
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    render(<SessionsScreen platform={null} onBack={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: /复制|copy/i }));
    expect(await screen.findByText(/无法访问剪切板|clipboard is unavailable/i)).toBeInTheDocument();
    Object.defineProperty(navigator, "clipboard", { value: original, configurable: true });
  });
```

按钮的无障碍名以各测试文件里现有的查询方式为准；若现有用例用的是 `getByTitle` 或 `getByLabelText`，跟着用同一种。

- [ ] **Step 3: 运行确认失败**

Run: `pnpm vitest run tests/UpdatesScreen.test.tsx tests/SessionsScreen.test.tsx`
Expected: 两条新用例 FAIL（按钮当前未 disabled，剪切板当前抛 TypeError）。

- [ ] **Step 4: 改四个 open() 调用点**

`src/screens/AccountsScreen.tsx` 里在现有的一组错误 state 旁（`:2317` 的 `officialFilePaths` 之后）新增 `const [officialPickerError, setOfficialPickerError] = useState<string | null>(null);`，并在组件顶部加 `const desktop = isDesktop();`（第 172 行的 import 改为 `import { getTransport, isDesktop, isTauriRuntime } from "../lib/transport";`）。两个函数各加守卫与 try/catch：

```tsx
  const chooseOfficialFiles = async () => {
    if (!desktop) return;
    setOfficialPickerError(null);
    try {
      const selected = await open({
        multiple: true,
        title: "选择账号 JSON 文件",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (Array.isArray(selected)) {
        setOfficialFilePaths(selected);
        return;
      }
      if (typeof selected === "string") {
        setOfficialFilePaths([selected]);
      }
    } catch (error) {
      setOfficialPickerError(error instanceof Error ? error.message : t("errors.operationFailed"));
    }
  };
```

`chooseExternalClientSource` 同样加 `if (!desktop) return;` 与 `try/catch`（catch 里用 `setOfficialPickerError` 之外的、该面板自己的错误 state，没有就同样新增一个）。按钮侧：`AccountsScreen.tsx:7064-7070` 的「导入 JSON 文件」按钮加 `disabled={!desktop}` 与 `title={!desktop ? t("common.desktopOnly") : undefined}`，并在 `:7071` 的已选文件提示旁渲染 `{officialPickerError && <p className="text-[12px] text-red-700">{officialPickerError}</p>}`。`ExternalClientImportPanel.tsx:152` 的 `disabled={loading}` 改成 `disabled={loading || !props.desktop}`（该面板需新增 `desktop: boolean` prop，由 `AccountsScreen.tsx:6752` 传 `desktop={desktop}`）。

`src/screens/VibeScreen.tsx:1361` 的 `chooseFolder` 同样处理（该文件也需新增 `import { isDesktop } from "../lib/transport";`）；它的两个入口分别是 `:2792` 的 `__choose_folder__` 选项（浏览器里不渲染该 option）和 `:3504` 的按钮（加 `disabled`）。该文件已有 `vibe.errorClipboardUnavailable` 的用法（`:1445`），错误提示沿用同一套 state。

- [ ] **Step 5: 改更新页**

`src/screens/UpdatesScreen.tsx` 顶部加 `import { isDesktop } from "../lib/transport";` 与 `const desktop = isDesktop();`，`handleCheck` 与 `handleRelaunch` 首行加 `if (!desktop) return;`，检查按钮 `disabled={!desktop || checking}`，并在按钮附近渲染 `{!desktop && <p className="text-[12px] text-stone-500">{t("common.desktopOnly")}</p>}`。自动更新本来就只在桌面端生效（`src/components/updates/AutoUpdatePrompt.tsx:36` 已有守卫），这里只是让整页与之一致。

- [ ] **Step 6: 改会话页剪切板**

`src/screens/SessionsScreen.tsx:307` 的 `copyText` 与仓库其他调用点对齐（参照 `src/screens/VibeScreen.tsx:1445-1448`）：

```tsx
  const copyText = async (value: string | null | undefined, marker: string) => {
    if (!value) {
      return;
    }
    // 明文 HTTP 的非环回来源不是 secure context，navigator.clipboard 直接是 undefined。
    if (!navigator.clipboard?.writeText) {
      setCopyError(t("vibe.errorClipboardUnavailable"));
      return;
    }
    try {
      await navigator.clipboard.writeText(value);
      setCopiedValue(marker);
      setCopyMenuOpen(false);
    } catch (error) {
      setCopyError(error instanceof Error ? error.message : t("errors.operationFailed"));
    }
  };
```

`copyError` 是新增的 state，渲染在复制按钮所在的那一块附近。

- [ ] **Step 7: 运行确认通过**

Run: `pnpm typecheck && pnpm test:run`
Expected: PASS。

- [ ] **Step 8: 提交**

```bash
git add src/screens/AccountsScreen.tsx src/screens/VibeScreen.tsx src/screens/UpdatesScreen.tsx src/screens/SessionsScreen.tsx src/components/accounts/ExternalClientImportPanel.tsx src/lib/i18n.tsx tests/UpdatesScreen.test.tsx tests/SessionsScreen.test.tsx
git commit -m "fix: 浏览器模式下禁用桌面专属入口而不是静默失败"
```

---

## Task 6: 代理未运行时的轮询退避

对应审计 #6。`get_route_proxy_status` 在代理未运行时每秒轮询且没有上限，每个打开的标签页一条。Task 4 让代理会自启，但"用户主动停掉代理"仍是常态，那时这个每秒请求会一直打到服务端。

**Files:**
- Modify: `src/screens/AccountsScreen.tsx:2752-2756`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: TanStack Query 的 `refetchInterval` 回调签名 `(query) => number | false`。
- Produces: 无跨任务接口。

- [ ] **Step 1: 写失败的测试**

`tests/AccountsScreen.test.tsx` 追加一条纯函数测试，并把间隔计算抽成可导出的函数（这样不必在测试里推进 60 秒的假时钟）：

```ts
import { routeProxyPollInterval } from "../src/screens/AccountsScreen";

describe("route proxy status polling", () => {
  it("stops once the proxy is running", () => {
    expect(routeProxyPollInterval({ running: true }, 0)).toBe(false);
  });

  it("backs off instead of hammering a stopped proxy forever", () => {
    // 每个标签页每秒一发、永不停止，对一台共享服务器是持续负载。
    expect(routeProxyPollInterval(undefined, 0)).toBe(1000);
    expect(routeProxyPollInterval({ running: false }, 3)).toBe(4000);
    expect(routeProxyPollInterval({ running: false }, 50)).toBe(15000);
  });
});
```

- [ ] **Step 2: 运行确认失败**

Run: `pnpm vitest run tests/AccountsScreen.test.tsx`
Expected: FAIL，`routeProxyPollInterval` 未导出。

- [ ] **Step 3: 实现**

在 `AccountsScreen.tsx` 里（组件之外，与文件里其它模块级常量放在一起）加：

```tsx
/** 代理停着的时候按失败次数退避到 15 秒，别让每个标签页每秒都发一发。 */
export function routeProxyPollInterval(
  data: { running?: boolean } | undefined,
  failureOrFetchCount: number,
) {
  if (data?.running) {
    return false as const;
  }
  return Math.min(1000 * 2 ** Math.min(failureOrFetchCount, 4), 15000);
}
```

查询改为：

```tsx
  const routeProxyQuery = useQuery({
    queryKey: ["route-proxy-status"],
    queryFn: getRouteProxyStatus,
    refetchInterval: (query) =>
      routeProxyPollInterval(query.state.data, query.state.dataUpdateCount),
  });
```

`dataUpdateCount` 是 `@tanstack/query-core` 5.101.2 里 `QueryState` 的字段（已确认存在），随每次成功轮询单调递增。

- [ ] **Step 4: 运行确认通过**

Run: `pnpm typecheck && pnpm vitest run tests/AccountsScreen.test.tsx`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "fix: 路由代理状态轮询在代理停止时退避"
```

---

## Task 7: 事件 WebSocket 断线重连

对应审计 #9。`WebTransport.ensureSocket` 的 `onclose` 只把 socket 置空，没有 `onerror`，也没有重连；一次网络抖动之后账号活动、状态变更就永久停更，界面看起来像"数据不动了"却没有任何报错。`onmessage` 里的 `JSON.parse` 也没有保护，一个非 JSON 帧就能让 handler 整体抛出。

**Files:**
- Modify: `src/lib/transport/web-transport.ts:129-155`
- Test: `tests/transport/transport.test.ts`

**Interfaces:**
- Consumes: `getWebAccessToken()`、`websocketUrl()`（同文件已有）。
- Produces: `WebTransport` 私有字段 `reconnectAttempts`、`reconnectTimer`；`destroy()` 需要一并清掉定时器。无跨任务接口。

- [ ] **Step 1: 写失败的测试**

`tests/transport/transport.test.ts` 追加。该文件已有 WebSocket 的替身写法，沿用它；要点是记录构造次数：

```ts
  it("reconnects the event socket after an unexpected close", async () => {
    vi.useFakeTimers();
    const sockets: FakeWebSocket[] = [];
    stubWebSocket((socket) => sockets.push(socket));
    const transport = new WebTransport("http://localhost:3090");

    await transport.subscribe("route-credential-status", () => {});
    expect(sockets).toHaveLength(1);

    // 抖动一次就永久停更，界面看起来只是"数据不动了"。
    sockets[0].onclose?.(new CloseEvent("close"));
    await vi.advanceTimersByTimeAsync(1000);
    expect(sockets).toHaveLength(2);

    transport.destroy();
    await vi.advanceTimersByTimeAsync(60000);
    expect(sockets).toHaveLength(2);
    vi.useRealTimers();
  });

  it("ignores a frame that is not JSON instead of throwing", async () => {
    const sockets: FakeWebSocket[] = [];
    stubWebSocket((socket) => sockets.push(socket));
    const transport = new WebTransport("http://localhost:3090");
    const handler = vi.fn();
    await transport.subscribe("route-credential-status", handler);

    expect(() => sockets[0].onmessage?.({ data: "not json" } as MessageEvent)).not.toThrow();
    expect(handler).not.toHaveBeenCalled();
    transport.destroy();
  });
```

`stubWebSocket` / `FakeWebSocket` 若该文件里还没有，就照它现有的 `globalThis.WebSocket` 替换方式补一个最小实现（只需要 `onmessage`、`onclose`、`onerror`、`readyState`、`close()`）。

- [ ] **Step 2: 运行确认失败**

Run: `pnpm vitest run tests/transport/transport.test.ts`
Expected: 重连用例 FAIL（只有 1 个 socket），非 JSON 帧用例 FAIL（抛异常）。

- [ ] **Step 3: 实现**

`src/lib/transport/web-transport.ts` 的类里加两个字段与一个销毁标记：

```ts
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private destroyed = false;
```

`destroy()` 改为：

```ts
  destroy() {
    this.destroyed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.socket?.close();
    this.socket = null;
    this.handlers.clear();
  }
```

`ensureSocket` 的收尾三个回调改为：

```ts
    this.socket.onopen = () => {
      this.reconnectAttempts = 0;
    };
    this.socket.onmessage = (message) => {
      // 一个非 JSON 帧不该让整条订阅链抛出。
      let event: WebEvent;
      try {
        event = JSON.parse(message.data as string) as WebEvent;
      } catch {
        return;
      }
      const handlers = this.handlers.get(event.channel);
      if (!handlers) {
        return;
      }
      for (const handler of handlers) {
        handler(event.payload);
      }
    };
    this.socket.onerror = () => {
      this.socket?.close();
    };
    this.socket.onclose = () => {
      this.socket = null;
      this.scheduleReconnect();
    };
```

新增私有方法：

```ts
  /** 断线不重连的话，实时活动会静默停更，界面只表现为"数据不动了"。 */
  private scheduleReconnect() {
    if (this.destroyed || this.handlers.size === 0 || this.reconnectTimer) {
      return;
    }
    const delay = Math.min(1000 * 2 ** Math.min(this.reconnectAttempts, 4), 15000);
    this.reconnectAttempts += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.ensureSocket();
    }, delay);
  }
```

`subscribe` 返回的取消函数在删掉最后一个 handler 后不需要额外处理——`scheduleReconnect` 已经在 `handlers.size === 0` 时不再排程。

- [ ] **Step 4: 运行确认通过**

Run: `pnpm typecheck && pnpm vitest run tests/transport`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src/lib/transport/web-transport.ts tests/transport/transport.test.ts
git commit -m "fix: 事件 WebSocket 断线后按退避重连"
```

---

## Task 8: 契约守卫补上 web 侧参数名

`e8a5c7b` 已经守住了"前端发 snake_case、Tauri 命令没声明 `rename_all`"这一半。另一半还没人看：web dispatcher 用字面量 key 读参数（`parse_arg(&args, "providerId")` 之类），前端改一个参数名而忘了改 dispatcher，编译通过、请求不报错，命令只会拿到 `None` 走默认值——和被修掉的那个 bug 完全同型。本次审计手工比对过 98 个命令、当前是对齐的，把这个比对固化成测试，别再靠人工。

**Files:**
- Modify: `tests/transport/command-contract.test.ts`

**Interfaces:**
- Consumes: 该文件已有的 `invokeCalls()`、`topLevelKeys()`、`readSource()`。
- Produces: 无。

- [ ] **Step 1: 写测试（预期直接通过，作为回归网）**

在 `describe("command contract", ...)` 内追加：

```ts
  it("reads every argument the client sends under the same name in the web dispatcher", () => {
    // dispatcher 用字面量 key 取参数，改名不会让编译或请求失败——命令只是
    // 静默拿到 None。这正是 e8a5c7b 修的那类 bug 的另一半。
    const dispatcher = readSource("src-tauri/src/web/handlers/mod.rs");
    const arms = dispatcher.split(/^\s*"([a-z0-9_]+)"\s*=>/m);
    const bodyByCommand = new Map<string, string>();
    for (let index = 1; index < arms.length; index += 2) {
      bodyByCommand.set(arms[index], arms[index + 1] ?? "");
    }

    const offenders: string[] = [];
    for (const call of invokeCalls(readSource("src/lib/api/client.ts"))) {
      const body = bodyByCommand.get(call.command);
      if (body === undefined) {
        continue; // 桌面专属命令，已由上面的用例覆盖
      }
      for (const key of topLevelKeys(call.body)) {
        if (!new RegExp(`"${key}"`).test(body)) {
          offenders.push(`${call.command}: web dispatcher never reads "${key}"`);
        }
      }
    }

    expect(offenders).toEqual([]);
  });
```

`invokeCalls` 只匹配带参数对象的调用，`topLevelKeys` 只取最外层键，两者都已在该文件里定义，不要重写。

- [ ] **Step 2: 运行确认通过**

Run: `pnpm vitest run tests/transport/command-contract.test.ts`
Expected: PASS。若报出 offender，说明确实存在遗漏，先修 dispatcher 再继续。

- [ ] **Step 3: 反向验证守卫真的有效**

临时把 `src-tauri/src/web/handlers/mod.rs` 里 `mcp_search_marketplace` 的 `required_string_arg(&args, "providerId")` 改成 `"provider_id"`，重跑上面的测试。
Expected: FAIL，报 `mcp_search_marketplace: web dispatcher never reads "providerId"`。确认后把改动还原（`git checkout -- src-tauri/src/web/handlers/mod.rs`）。

- [ ] **Step 4: 提交**

```bash
git add tests/transport/command-contract.test.ts
git commit -m "test: 契约守卫覆盖 web dispatcher 的参数名"
```

---

## 落地顺序与验收

Task 1 单独就能让"打开就报错"消失，可以先合先发；Task 3 是安全修复，建议与 Task 1 同一版发布。Task 2、4 提升的是"出问题时能看懂"和"无人值守能自恢复"，Task 5~7 是浏览器模式的体验收口，Task 8 是防回归。

全量验收（在一台干净目录里模拟发布包）：

```bash
pnpm install --frozen-lockfile
pnpm typecheck && pnpm test:run
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt --check && CARGO_TARGET_DIR=target-codex cargo test && cd ..
pnpm build
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo build --release --bin ai-switch-server && cd ..
mkdir -p /tmp/aisw-verify/web && cp src-tauri/target-codex/release/ai-switch-server.exe /tmp/aisw-verify/ && cp -r dist/* /tmp/aisw-verify/web/
cd /tmp/aisw-verify && AI_SWITCH_TOKEN=0123456789abcdef AI_SWITCH_PORT=13095 ./ai-switch-server.exe
```

逐条确认：启动日志打印 `Serving AI Switch web assets from ...`；浏览器打开 `http://127.0.0.1:13095/` 出现令牌输入页，填入令牌后进入账号页且账号列表有数据；不带令牌 `POST /api/list_route_credentials` 返回 401；`get_route_proxy_status` 按配置返回 `running: true`；Ctrl+C 后无残留 sidecar 进程。收尾 `rm -rf /tmp/aisw-verify`。




















