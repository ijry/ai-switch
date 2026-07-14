# Built-in Tailscale Sidecar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace system Tailscale CLI integration with an app-owned `ai-switch-tsnet` sidecar that exposes the local web service over a private tailnet using OAuth or auth key login.

**Architecture:** Keep the existing Axum web service bound to `127.0.0.1`. Add a Rust `TailscaleService` facade that manages a Go/tsnet sidecar process. The sidecar authenticates to Tailscale, serves on the tailnet, and reverse-proxies to the local web backend. Frontend settings keep one secure-network section with OAuth, auth key, status, and remote URLs.

**Tech Stack:** Rust 2021, Tauri 2, Axum 0.7, React 18, TypeScript, TanStack Query, Go `tsnet`/`libtailscale`, Vitest, cargo test

## Global Constraints

- Built-in node only; never shell out to system `tailscale`.
- Independent state dir: `~/.ai-switch/tailscale/`.
- Local web remains on `127.0.0.1` by default.
- Remote access goes only through the embedded node reverse proxy.
- AI Switch bearer token is still required over Tailscale.
- No silent login on app startup.
- Auth keys must not be logged, echoed after submit, or stored in `web-service.json`.
- UI copy uses product language: Secure network / Remote access / Auth key.
- Work on `main` only unless the user explicitly asks for a branch.

**Spec:** `docs/superpowers/specs/2026-07-14-ai-switch-libtailscale-sidecar-design.md`

---

## File Map

| File | Responsibility |
|---|---|
| `src-tauri/src/paths.rs` | Add `tailscale_dir` |
| `src-tauri/src/services/tailscale_types.rs` | Shared status/login/start request types |
| `src-tauri/src/services/tailscale_sidecar.rs` | Process discovery, control client, runtime state |
| `src-tauri/src/services/tailscale_service.rs` | Public facade used by commands/handlers |
| `src-tauri/src/services/web_service.rs` | Start/stop hooks and config fields |
| `src-tauri/src/commands/web_service_commands.rs` | Tauri commands including auth key |
| `src-tauri/src/web/handlers/mod.rs` | Web API parity for new commands |
| `src-tauri/src/app_state.rs` | Hold sidecar runtime state |
| `src-tauri/src/lib.rs` | Register new command |
| `sidecar/ai-switch-tsnet/` | Go sidecar source |
| `src/lib/api/types.ts` | Frontend status/config types |
| `src/lib/api/client.ts` | Frontend API wrappers |
| `src/components/settings/tailscale-settings.tsx` | Secure network UI |
| `src/components/settings/web-service-settings.tsx` | Remote URL display wiring |
| `src/lib/i18n.tsx` | EN/ZH copy |
| `tests/...` | Frontend and transport tests |
| `src-tauri/tauri.conf.json` / packaging notes | Bundle sidecar binary |

---

### Task 1: Paths and config model

**Files:**
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/services/web_service.rs`
- Test: `src-tauri/src/paths.rs` unit tests or adjacent test module
- Test: `src-tauri/src/services/web_service.rs` normalize/default tests

**Interfaces:**
- Produces:
  - `AppPaths.tailscale_dir: PathBuf`
  - `WebServiceConfig { tailscale_hostname: Option<String>, tailscale_auth_key_present: bool, ...existing }`

- [ ] **Step 1: Write failing tests for path and config defaults**

```rust
#[test]
fn app_paths_include_tailscale_dir() {
    let paths = AppPaths::from_data_dir(PathBuf::from("C:/tmp/ai-switch-data"));
    assert_eq!(paths.tailscale_dir, PathBuf::from("C:/tmp/ai-switch-data/tailscale"));
}

#[test]
fn web_service_config_defaults_keep_auth_key_absent() {
    let config = WebServiceConfig::default();
    assert_eq!(config.tailscale_enabled, false);
    assert_eq!(config.tailscale_auth_key_present, false);
    assert!(config.tailscale_hostname.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
cd src-tauri
cargo test app_paths_include_tailscale_dir web_service_config_defaults_keep_auth_key_absent -- --nocapture
```

Expected: compile/link failure because fields do not exist yet.

- [ ] **Step 3: Implement path and config fields**

```rust
// paths.rs
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database_file: PathBuf,
    pub settings_file: PathBuf,
    pub web_service_file: PathBuf,
    pub backups_dir: PathBuf,
    pub imports_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub tailscale_dir: PathBuf,
}

impl AppPaths {
    pub fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            database_file: data_dir.join("ai-switch.db"),
            settings_file: data_dir.join("settings.json"),
            web_service_file: data_dir.join("web-service.json"),
            backups_dir: data_dir.join("backups"),
            imports_dir: data_dir.join("imports"),
            logs_dir: data_dir.join("logs"),
            tailscale_dir: data_dir.join("tailscale"),
            data_dir,
        }
    }

    pub async fn ensure(&self) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.data_dir).await?;
        tokio::fs::create_dir_all(&self.backups_dir).await?;
        tokio::fs::create_dir_all(&self.imports_dir).await?;
        tokio::fs::create_dir_all(&self.logs_dir).await?;
        tokio::fs::create_dir_all(&self.tailscale_dir).await?;
        Ok(())
    }
}
```

```rust
// web_service.rs
pub struct WebServiceConfig {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
    pub auto_start: bool,
    pub tailscale_enabled: bool,
    pub tailscale_hostname: Option<String>,
    pub tailscale_auth_key_present: bool,
}
```

Normalize must preserve unknown-compatible defaults and never invent a raw auth key field in JSON.

- [ ] **Step 4: Re-run tests**

Run:
```powershell
cd src-tauri
cargo test app_paths_include_tailscale_dir web_service_config_defaults_keep_auth_key_absent -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/paths.rs src-tauri/src/services/web_service.rs
git commit -m "feat: add tailscale path and web service config fields"
```

---

### Task 2: Sidecar protocol types and fake client

**Files:**
- Create: `src-tauri/src/services/tailscale_types.rs`
- Create: `src-tauri/src/services/tailscale_sidecar.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: unit tests inside `tailscale_sidecar.rs`

**Interfaces:**
- Produces:
  - `TailscaleStatus`
  - `TailscaleLogin`
  - `TailscaleStartRequest { state_dir, hostname, auth_key, backend_addr, serve_port }`
  - `SidecarControlClient` trait with `start`, `login_oauth`, `stop`, `logout`, `status`
  - `FakeSidecarControlClient` for tests

- [ ] **Step 1: Write failing tests for status mapping and fake client**

```rust
#[tokio::test]
async fn fake_sidecar_reports_needs_login_until_oauth_completes() {
    let client = FakeSidecarControlClient::default();
    let status = client.status().await.unwrap();
    assert_eq!(status.state, "needsLogin");

    let login = client.login_oauth().await.unwrap();
    assert!(login.login_url.unwrap().starts_with("https://"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```powershell
cd src-tauri
cargo test fake_sidecar_reports_needs_login_until_oauth_completes -- --nocapture
```

Expected: FAIL because modules/types are missing.

- [ ] **Step 3: Implement types and fake client**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub state: String,
    pub device_name: Option<String>,
    pub tailnet_ip: Option<String>,
    pub magic_dns_name: Option<String>,
    pub login_url: Option<String>,
    pub access_urls: Vec<String>,
    pub serving: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleLogin {
    pub login_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStartRequest {
    pub state_dir: String,
    pub hostname: String,
    pub auth_key: Option<String>,
    pub backend_addr: String,
    pub serve_port: u16,
}

#[async_trait::async_trait]
pub trait SidecarControlClient: Send + Sync {
    async fn start(&self, request: TailscaleStartRequest) -> Result<TailscaleStatus, String>;
    async fn login_oauth(&self) -> Result<TailscaleLogin, String>;
    async fn stop(&self) -> Result<TailscaleStatus, String>;
    async fn logout(&self) -> Result<TailscaleStatus, String>;
    async fn status(&self) -> Result<TailscaleStatus, String>;
}
```

Also implement binary discovery helper:

```rust
pub fn resolve_sidecar_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AI_SWITCH_TSNET_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let sibling = exe.with_file_name(if cfg!(windows) {
        "ai-switch-tsnet.exe"
    } else {
        "ai-switch-tsnet"
    });
    sibling.exists().then_some(sibling)
}
```

- [ ] **Step 4: Re-run tests**

Run:
```powershell
cd src-tauri
cargo test fake_sidecar_reports_needs_login_until_oauth_completes resolve_sidecar_path -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/tailscale_types.rs src-tauri/src/services/tailscale_sidecar.rs src-tauri/src/services/mod.rs src-tauri/Cargo.toml
git commit -m "feat: add tailscale sidecar control client contracts"
```

---

### Task 3: Replace TailscaleService facade

**Files:**
- Modify: `src-tauri/src/services/tailscale_service.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: tests in `tailscale_service.rs`

**Interfaces:**
- Consumes: `SidecarControlClient`, `AppPaths.tailscale_dir`, web backend addr/port
- Produces:
  - `TailscaleService::status(state, paths, web_status)`
  - `TailscaleService::start_login(...)`
  - `TailscaleService::start_with_auth_key(...)`
  - `TailscaleService::disconnect(...)`
  - `TailscaleRuntimeState` stored on `AppState`

- [ ] **Step 1: Write failing facade tests**

```rust
#[tokio::test]
async fn status_is_error_when_sidecar_binary_missing() {
    let runtime = TailscaleRuntimeState::default();
    let paths = AppPaths::from_data_dir(tempdir());
    let status = TailscaleService::status_with_client(
        &runtime,
        &paths,
        None,
        MissingSidecarFactory,
        None,
    )
    .await;
    assert_eq!(status.state, "error");
    assert!(status.message.unwrap().contains("built-in network component"));
}

#[tokio::test]
async fn auth_key_connect_sets_connected_and_access_urls() {
    // fake client returns connected with tailnet ip
    // service should build accessUrls using web port
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
cd src-tauri
cargo test status_is_error_when_sidecar_binary_missing auth_key_connect_sets_connected_and_access_urls -- --nocapture
```

Expected: FAIL on missing APIs.

- [ ] **Step 3: Implement facade**

Key behaviors:

```rust
impl TailscaleService {
    pub async fn status(...) -> TailscaleStatus
    pub async fn start_login(...) -> TailscaleLogin
    pub async fn start_with_auth_key(..., auth_key: String) -> TailscaleStatus
    pub async fn disconnect(...) -> TailscaleStatus
}
```

Rules:
- If feature disabled in config, return `state = "disabled"`.
- If binary missing, `state = "error"` and do not call system CLI.
- Build `access_urls` only when connected and web port known:
  - `http://{tailnet_ip}:{port}`
  - `http://{magic_dns_name}:{port}` when present
- Persist auth key only under `paths.tailscale_dir`, never in `web-service.json`.
- Update `tailscale_auth_key_present` in config when a key is saved/cleared.

Remove all `Command::new("tailscale")` usage.

- [ ] **Step 4: Wire AppState**

```rust
pub struct AppState {
    pub paths: AppPaths,
    pub pool: SqlitePool,
    pub route_proxy: RouteProxyRuntimeState,
    pub web_service: WebServiceRuntimeState,
    pub tailscale: TailscaleRuntimeState,
    pub terminals: TerminalManager,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
}
```

- [ ] **Step 5: Re-run tests**

Run:
```powershell
cd src-tauri
cargo test status_is_error_when_sidecar_binary_missing auth_key_connect_sets_connected_and_access_urls -- --nocapture
```

Expected: PASS

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/services/tailscale_service.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs
git commit -m "feat: replace tailscale CLI facade with sidecar service"
```

---

### Task 4: Commands, web handlers, and frontend API

**Files:**
- Modify: `src-tauri/src/commands/web_service_commands.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Test: `tests/transport/transport.test.ts` or new API type tests if present
- Test: handler unit coverage via existing dispatch tests if available; otherwise add focused Rust tests for arg parsing

**Interfaces:**
- Produces commands:
  - `get_tailscale_status`
  - `start_tailscale_login`
  - `start_tailscale_with_auth_key { authKey: string }`
  - `disconnect_tailscale`
- Frontend:
  - `startTailscaleWithAuthKey(authKey: string)`
  - extended `TailscaleStatus` fields

- [ ] **Step 1: Write failing frontend type/client expectations**

```ts
export type TailscaleStatus = {
  state: string;
  deviceName?: string | null;
  tailnetIp?: string | null;
  magicDnsName?: string | null;
  loginUrl?: string | null;
  accessUrls?: string[];
  serving?: boolean;
  message?: string | null;
};

export function startTailscaleWithAuthKey(authKey: string): Promise<TailscaleStatus> {
  return invoke("start_tailscale_with_auth_key", { authKey });
}
```

Add a transport test that posts `start_tailscale_with_auth_key` with bearer token.

- [ ] **Step 2: Run frontend test to verify failure**

Run:
```powershell
pnpm test:run tests/transport/transport.test.ts
```

Expected: FAIL until client/command surface exists and is exported.

- [ ] **Step 3: Implement commands and handler branches**

```rust
#[tauri::command]
pub async fn start_tailscale_with_auth_key(
    state: State<'_, AppState>,
    auth_key: String,
) -> Result<TailscaleStatus, ApiError> {
    // load web config/status, call TailscaleService::start_with_auth_key
}
```

Handler match arms:

```rust
"start_tailscale_with_auth_key" => {
    let auth_key = required_string_arg(&args, "authKey")?;
    to_value(TailscaleService::start_with_auth_key(...).await.map_err(to_error)?)
}
```

Update existing status/login/disconnect arms to pass `AppState` paths/runtime/web status.

- [ ] **Step 4: Re-run tests**

Run:
```powershell
pnpm test:run tests/transport/transport.test.ts
cd src-tauri
cargo test start_tailscale -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands/web_service_commands.rs src-tauri/src/web/handlers/mod.rs src-tauri/src/lib.rs src/lib/api/types.ts src/lib/api/client.ts tests/transport/transport.test.ts
git commit -m "feat: expose tailscale auth key command across transports"
```

---

### Task 5: Web service lifecycle hooks

**Files:**
- Modify: `src-tauri/src/services/web_service.rs`
- Modify: `src-tauri/src/commands/web_service_commands.rs`
- Test: Rust tests for start/stop interaction with fake sidecar

**Interfaces:**
- Consumes: `TailscaleService`, `WebServiceConfig.tailscale_enabled`
- Produces: starting web may auto-start sidecar when auth already present; stopping web always stops sidecar

- [ ] **Step 1: Write failing lifecycle tests**

```rust
#[tokio::test]
async fn stopping_web_service_stops_sidecar() {
    // start fake connected sidecar via service
    // call WebService::stop path that also disconnects tailscale
    // assert sidecar stop called
}

#[tokio::test]
async fn starting_web_with_saved_auth_starts_sidecar() {
    // config.tailscale_enabled = true and auth key present
    // start web
    // assert sidecar start called with backend 127.0.0.1:port
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
cd src-tauri
cargo test stopping_web_service_stops_sidecar starting_web_with_saved_auth_starts_sidecar -- --nocapture
```

Expected: FAIL

- [ ] **Step 3: Implement hooks**

In `start_web_server` success path:
- if `config.tailscale_enabled` and saved auth/node state exists, call `TailscaleService::ensure_started(...)`

In `stop_web_server`:
- always call `TailscaleService::disconnect(...)` best-effort after web stop

Do not auto OAuth. Only auto-start when credentials/state already exist.

- [ ] **Step 4: Re-run tests**

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/services/web_service.rs src-tauri/src/commands/web_service_commands.rs src-tauri/src/services/tailscale_service.rs
git commit -m "feat: couple web service lifecycle to built-in tailscale node"
```

---

### Task 6: Settings UI and i18n

**Files:**
- Modify: `src/components/settings/tailscale-settings.tsx`
- Modify: `src/components/settings/web-service-settings.tsx`
- Modify: `src/lib/i18n.tsx`
- Test: `tests/settings/tailscale-settings.test.tsx` (create if missing)

**Interfaces:**
- Consumes: extended `TailscaleStatus`, `startTailscaleWithAuthKey`
- Produces: compact secure-network panel with OAuth, auth key, remote URLs

- [ ] **Step 1: Write failing UI tests**

```tsx
it("submits auth key and clears the input", async () => {
  // render TailscaleSettings enabled
  // type key, click connect
  // expect startTailscaleWithAuthKey called
  // expect input cleared
});

it("renders remote access urls when connected", async () => {
  // mock status with accessUrls
  // expect urls visible
});
```

- [ ] **Step 2: Run UI tests to verify they fail**

Run:
```powershell
pnpm test:run tests/settings/tailscale-settings.test.tsx
```

Expected: FAIL

- [ ] **Step 3: Implement UI**

Required UI pieces:
- status badge from `state`
- device/ip/magicdns lines
- remote access URL list + copy
- OAuth button
- auth key password input + connect button
- disconnect + refresh
- product copy only

i18n keys to add (EN + ZH):
- `settings.tailscale.authKey`
- `settings.tailscale.authKeyPlaceholder`
- `settings.tailscale.connectAuthKey`
- `settings.tailscale.remoteAccess`
- `settings.tailscale.copyUrl`
- `settings.tailscale.webRequired`
- `settings.tailscale.componentMissing`

- [ ] **Step 4: Re-run UI tests and typecheck**

Run:
```powershell
pnpm test:run tests/settings/tailscale-settings.test.tsx
pnpm typecheck
```

Expected: PASS

- [ ] **Step 5: Commit**

```powershell
git add src/components/settings/tailscale-settings.tsx src/components/settings/web-service-settings.tsx src/lib/i18n.tsx tests/settings/tailscale-settings.test.tsx
git commit -m "feat: add secure network auth key and remote url UI"
```

---

### Task 7: Go sidecar minimal implementation

**Files:**
- Create: `sidecar/ai-switch-tsnet/go.mod`
- Create: `sidecar/ai-switch-tsnet/main.go`
- Create: `sidecar/ai-switch-tsnet/control.go`
- Create: `sidecar/ai-switch-tsnet/proxy.go`
- Create: `sidecar/ai-switch-tsnet/README.md`
- Optional test: `sidecar/ai-switch-tsnet/control_test.go`

**Interfaces:**
- Produces binary `ai-switch-tsnet(.exe)` implementing control API from the spec

- [ ] **Step 1: Write failing control handler tests in Go**

```go
func TestStatusDefaultsToNeedsLogin(t *testing.T) {
    s := newServer()
    // GET /control/status => needsLogin
}
```

- [ ] **Step 2: Run Go tests to verify failure**

Run:
```powershell
cd sidecar/ai-switch-tsnet
go test ./...
```

Expected: FAIL because package/files are incomplete.

- [ ] **Step 3: Implement minimal sidecar**

`main.go` responsibilities:
- parse flags: `--control-addr 127.0.0.1:0` (write chosen addr to stdout line `CONTROL <addr>`)
- run localhost control HTTP server
- on start:
  - configure `tsnet.Server{ Dir: stateDir, Hostname: hostname, AuthKey: authKey }`
  - if auth present, bring node up
  - when online, `srv.Listen("tcp", ":servePort")`
  - reverse proxy to `backendAddr`
- on login-oauth:
  - return auth URL from tsnet auth path
- on stop/logout:
  - close listeners and server

Keep code small. Prefer official `tailscale.com/tsnet`.

- [ ] **Step 4: Build Windows binary**

Run:
```powershell
cd sidecar/ai-switch-tsnet
go test ./...
go build -o ai-switch-tsnet.exe .
```

Expected: tests pass, binary created.

- [ ] **Step 5: Commit**

```powershell
git add sidecar/ai-switch-tsnet
git commit -m "feat: add ai-switch-tsnet sidecar control and proxy"
```

---

### Task 8: Real sidecar client process management

**Files:**
- Modify: `src-tauri/src/services/tailscale_sidecar.rs`
- Modify: `src-tauri/src/services/tailscale_service.rs`
- Test: process integration test gated when binary exists

**Interfaces:**
- Produces: `HttpSidecarControlClient` that:
  1. spawns `ai-switch-tsnet`
  2. reads `CONTROL 127.0.0.1:port`
  3. calls control endpoints with reqwest

- [ ] **Step 1: Write failing spawn/parse test**

```rust
#[test]
fn parse_control_addr_line() {
    assert_eq!(
        parse_control_addr_line("CONTROL 127.0.0.1:4567"),
        Some("127.0.0.1:4567".to_string())
    );
}
```

- [ ] **Step 2: Implement HTTP client + process supervisor**

```rust
pub struct SidecarProcess {
    child: Mutex<Option<Child>>,
    control_base: Mutex<Option<String>>,
}
```

Requirements:
- kill child on stop/disconnect/drop
- only bind control on localhost
- timeout on start/status calls
- map transport failures to `state = "error"`

- [ ] **Step 3: Optional integration smoke**

If `AI_SWITCH_TSNET_PATH` points at built binary:

```powershell
$env:AI_SWITCH_TSNET_PATH = "D:\Repos\xyito\open\ai-switch\sidecar\ai-switch-tsnet\ai-switch-tsnet.exe"
cd src-tauri
cargo test sidecar_process_starts_and_reports_status -- --nocapture --ignored
```

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/src/services/tailscale_sidecar.rs src-tauri/src/services/tailscale_service.rs
git commit -m "feat: manage ai-switch-tsnet process from rust"
```

---

### Task 9: Packaging and manual acceptance

**Files:**
- Modify: `src-tauri/tauri.conf.json` if bundle resources/externalBin is used
- Modify: package scripts or README only if needed for build path
- Create/update short operator notes only if repo already documents packaging; otherwise keep changes in code/config

**Interfaces:**
- Desktop install dir contains both:
  - `ai-switch.exe`
  - `ai-switch-tsnet.exe`

- [ ] **Step 1: Wire binary into desktop package**

Prefer Tauri `bundle.externalBin` or copy step into install/package output:

```json
"bundle": {
  "externalBin": [
    "binaries/ai-switch-tsnet"
  ]
}
```

On Windows, ensure the built sidecar is available at the expected externalBin path before `pnpm tauri:build`.

- [ ] **Step 2: Build desktop package**

Run:
```powershell
# build sidecar first
cd sidecar/ai-switch-tsnet
go build -o ..\..\src-tauri\binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe .

cd ..\..
pnpm tauri:build
```

Expected: installer/app directory includes sidecar binary.

- [ ] **Step 3: Manual acceptance checklist**

1. Launch desktop app; UI loads without `127.0.0.1:1420` dependency.
2. Start web service on `127.0.0.1:<port>`.
3. Enable secure network.
4. Connect with auth key or OAuth.
5. Status shows connected + remote URL.
6. Same-tailnet client opens remote URL and can load UI after token.
7. Request without token returns 401.
8. Disconnect; remote URL becomes unreachable.
9. Localhost URL still works while web is running.
10. Uninstall/absence of system Tailscale does not break the flow.

- [ ] **Step 4: Commit packaging changes**

```powershell
git add src-tauri/tauri.conf.json src-tauri/binaries sidecar/ai-switch-tsnet
git commit -m "chore: package built-in tailscale sidecar with desktop app"
```

---

## Self-Review Against Spec

| Spec requirement | Task |
|---|---|
| Built-in node via sidecar, not system CLI | Tasks 2, 3, 7, 8 |
| OAuth + Auth Key | Tasks 3, 4, 6, 7 |
| Independent state dir | Tasks 1, 3 |
| Reverse proxy exposure to local web | Tasks 5, 7, 8 |
| Localhost web remains | Tasks 5, 9 |
| Token still required | unchanged web auth + Task 9 acceptance |
| No silent startup login | Tasks 3, 5 |
| Settings UI remote URLs + product copy | Task 6 |
| Package sidecar | Task 9 |
| Later FFI path preserved via facade | Task 3 keeps `TailscaleService` boundary |

Placeholder scan: none intentionally left.
Type consistency:
- `authKey` camelCase across frontend/backend
- `accessUrls`, `magicDnsName`, `serving` shared by Rust/TS/Go JSON

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-14-ai-switch-libtailscale-sidecar.md`.

Two execution options:

1. **Subagent-Driven (recommended)**  
   Fresh subagent per task, review between tasks, faster iteration.

2. **Inline Execution**  
   Execute tasks in this session with executing-plans and checkpoints.

Which approach?
