# Route Proxy HTTPS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an optional HTTPS-only transport for the local route proxy, backed by an application-managed Root CA, automatic current-user trust-store installation/removal on Windows, macOS, and Linux, and a Settings UI named `HTTPS`.

**Architecture:** Persist one small HTTPS preference file and certificate metadata under the app data directory. A certificate service owns Root CA/leaf certificate lifecycle; a trust service owns platform-specific import/removal commands; the existing route proxy chooses either its current HTTP listener or an `axum-server` rustls listener. HTTPS transport changes are orchestrated transactionally so a failed TLS activation restores a usable HTTP proxy, and existing managed route configurations are rewritten only for platforms that already own an AI Switch proxy key.

**Tech Stack:** Rust, Tauri 2, Axum 0.7, `axum-server` 0.7 (`tls-rustls`), `rcgen` 0.13, `x509-parser`, React 18, TypeScript, TanStack Query, Vitest.

## Global Constraints

- Work directly on `main`; do not create or switch branches/worktrees.
- Do not revert unrelated dirty worktree changes.
- Scope is only the loopback route proxy. Do not add HTTPS to Web Service.
- The proxy stays bound to `127.0.0.1`; the only leaf SAN values are `localhost` and `127.0.0.1`.
- HTTPS enabled means TLS-only. Never run an HTTP listener beside the TLS listener.
- Store all certificate material under `~/.ai-switch/certs/route-proxy/` through `AppPaths`; never return private-key bytes through Tauri/Web APIs or error text.
- On Unix, write the Root and leaf private-key PEM files with mode `0o600`; keep the certificate directory inside the current user's application-data directory on every platform.
- Root trust installation is current-user only on Windows and macOS. Never request machine-wide elevation there.
- Linux must attempt `p11-kit`, Debian/Ubuntu, RHEL/Fedora, and optional NSS current-user adapters; unavailable tools or insufficient permissions return exact manual steps.
- Every OS process invocation uses a fixed executable plus argument array. Do not pass user-controlled values to a shell, `sh -c`, PowerShell `-Command`, or string-built command line.
- Root removal must use the managed Root certificate SHA-1 thumbprint and the exact generated certificate file/nickname. It must never enumerate and delete unrelated certificates.
- Enabling HTTPS must preserve a usable HTTP proxy if TLS generation or startup fails. Uninstall failure must restore the previous TLS proxy and keep HTTPS enabled.
- On transport changes, rewrite only configurations for platforms already present in `route_proxy_keys`; do not create keys or write files for unused platforms.

## File Map

| File | Responsibility |
|---|---|
| `src-tauri/Cargo.toml` | TLS, certificate generation, certificate parsing, SHA-1 dependencies |
| `src-tauri/src/paths.rs` | Dedicated HTTPS config and certificate paths |
| `src-tauri/src/models/route_proxy_https.rs` | Persisted preference, public status, operation outcome, trust-state types |
| `src-tauri/src/models/mod.rs` | Export HTTPS model module |
| `src-tauri/src/services/route_proxy_https_service.rs` | Certificate material lifecycle, status, transport orchestration |
| `src-tauri/src/services/route_proxy_https_trust.rs` | Pure platform command construction, controlled trust-store execution, and managed Root trust inspection |
| `src-tauri/src/services/route_proxy_service.rs` | HTTP/TLS listener selection and HTTPS-only runtime behavior |
| `src-tauri/src/services/route_config_service.rs` | Rewrite only existing managed configurations after a scheme change |
| `src-tauri/src/database/repositories/route_proxy_key_repository.rs` | List existing configured platforms without creating keys |
| `src-tauri/src/services/mod.rs` | Export HTTPS services |
| `src-tauri/src/commands/route_proxy_https_commands.rs` | Tauri command wrappers for HTTPS actions and folder opening |
| `src-tauri/src/commands/route_proxy_commands.rs` | Start proxy using persisted HTTP/HTTPS preference |
| `src-tauri/src/commands/mod.rs` | Export HTTPS command module |
| `src-tauri/src/lib.rs` | Register Tauri HTTPS commands |
| `src-tauri/src/web/handlers/mod.rs` | Add HTTPS commands to Web Transport dispatch and start proxy with preference |
| `src/lib/api/types.ts` | HTTPS status/outcome TypeScript types |
| `src/lib/api/client.ts` | HTTPS API client functions |
| `src/components/settings/route-proxy-https-settings.tsx` | HTTPS settings card and mutation lifecycle |
| `src/screens/SettingsScreen.tsx` | Add HTTPS feature entry and section rendering |
| `src/lib/i18n.tsx` | English and Chinese HTTPS copy |
| `tests/SettingsScreen.test.tsx` | Settings HTTPS UI/mutation tests |

---

### Task 1: Persist HTTPS Preferences and Certificate Material

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/paths.rs`
- Create: `src-tauri/src/models/route_proxy_https.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/services/route_proxy_https_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: unit tests in `paths.rs` and `route_proxy_https_service.rs`

**Interfaces:**
- Produces `RouteProxyHttpsConfig { enabled: bool }`, persisted in `AppPaths.route_proxy_https_config_file`.
- Produces `RouteProxyHttpsMaterial { root_certificate_pem, root_fingerprint_sha256, root_thumbprint_sha1, server_certificate_pem, server_private_key_pem, expires_at }` without serializing private keys into public status.
- Produces `RouteProxyHttpsService::{load_config, save_config, ensure_material, status, delete_material}`.
- Later tasks consume `RouteProxyHttpsService::transport(paths) -> Result<RouteProxyTransport, AppError>`.

- [ ] **Step 1: Add the minimal TLS/certificate dependencies and write failing certificate tests**

Add the following entries under `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
axum-server = { version = "0.7", features = ["tls-rustls"] }
rcgen = { version = "0.13", features = ["x509-parser"] }
x509-parser = "0.16"
sha1 = "0.10"
time = "0.3"
```

Add the certificate service tests before implementation:

```rust
#[tokio::test]
async fn ensure_material_generates_root_and_loopback_leaf_without_exposing_private_key() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::from_data_dir(temp.path().to_path_buf());

    let material = RouteProxyHttpsService::ensure_material(&paths)
        .await
        .expect("certificate material");
    let leaf = tokio::fs::read(&material.server_certificate_pem)
        .await
        .expect("leaf pem");
    let (_, pem) = x509_parser::pem::parse_x509_pem(&leaf).expect("pem");
    let (_, certificate) = x509_parser::parse_x509_certificate(&pem.contents).expect("x509");
    let san = certificate
        .subject_alternative_name()
        .expect("san extension")
        .expect("san value");
    let san_values = san.value.general_names.iter().map(ToString::to_string).collect::<Vec<_>>();

    assert!(material.root_certificate_pem.exists());
    assert!(material.server_certificate_pem.exists());
    assert!(material.server_private_key_pem.exists());
    assert!(material.root_fingerprint_sha256.len() == 64);
    assert!(material.root_thumbprint_sha1.len() == 40);
    assert!(san_values.iter().any(|value| value.contains("localhost")));
    assert!(san_values.iter().any(|value| value.contains("127.0.0.1")));

    let status = RouteProxyHttpsService::status(&paths, None).await.expect("status");
    assert!(status.cert_ready);
    assert!(status.root_fingerprint.is_some());
    assert!(serde_json::to_string(&status).expect("status json").contains("rootFingerprint"));
    assert!(!serde_json::to_string(&status).expect("status json").contains("PRIVATE KEY"));
}

#[tokio::test]
async fn delete_material_rejects_enabled_https_and_removes_only_the_managed_directory() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
    RouteProxyHttpsService::ensure_material(&paths).await.expect("material");
    RouteProxyHttpsService::save_config(&paths, &RouteProxyHttpsConfig { enabled: true })
        .await
        .expect("save");

    let error = RouteProxyHttpsService::delete_material(&paths).await.expect_err("enabled error");
    assert!(error.to_string().contains("Disable HTTPS"));

    RouteProxyHttpsService::save_config(&paths, &RouteProxyHttpsConfig { enabled: false })
        .await
        .expect("save disabled");
    RouteProxyHttpsService::delete_material(&paths).await.expect("delete material");
    assert!(!paths.route_proxy_https_dir.exists());
}
```

- [ ] **Step 2: Run the focused tests and verify they fail because the new API does not exist**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_https_service -- --nocapture
```

Expected: compilation failure naming missing `route_proxy_https` model/service symbols.

- [ ] **Step 3: Add paths, public models, and an atomic certificate-material implementation**

Extend `AppPaths` with exactly these fields and create the directories in `ensure()`:

```rust
pub route_proxy_https_config_file: PathBuf,
pub route_proxy_https_dir: PathBuf,
```

Initialize them in `from_data_dir`:

```rust
route_proxy_https_config_file: data_dir.join("route-proxy-https.json"),
route_proxy_https_dir: data_dir.join("certs").join("route-proxy"),
```

`AppPaths::ensure()` must create `data_dir` and its ordinary application directories, but **must not** pre-create `route_proxy_https_dir`. The certificate service owns that leaf directory so it can atomically rename a fully written sibling directory into place. It may create `route_proxy_https_dir.parent()` immediately before generating material.

Create `models/route_proxy_https.rs` with these serializable public types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl Default for RouteProxyHttpsConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RouteProxyTrustStatus {
    SystemTrusted,
    NssTrusted,
    PartiallyTrusted,
    Untrusted,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsStatus {
    pub enabled: bool,
    pub cert_ready: bool,
    pub trust_status: RouteProxyTrustStatus,
    pub trust_adapter: Option<String>,
    pub root_fingerprint: Option<String>,
    pub expires_at: Option<String>,
    pub certificate_dir: String,
    pub root_certificate_path: Option<String>,
    pub proxy_base_url: Option<String>,
    pub message: Option<String>,
    pub manual_instructions: Vec<String>,
}
```

In `route_proxy_https_service.rs`, use the following fixed file names:

```rust
const ROOT_CERTIFICATE_FILE: &str = "root-ca.pem";
const ROOT_PRIVATE_KEY_FILE: &str = "root-ca-key.pem";
const SERVER_CERTIFICATE_FILE: &str = "server-cert.pem";
const SERVER_PRIVATE_KEY_FILE: &str = "server-key.pem";
const METADATA_FILE: &str = "metadata.json";
const ROOT_COMMON_NAME: &str = "AI Switch Route Proxy Root CA";
const SERVER_COMMON_NAME: &str = "AI Switch Route Proxy localhost";
```

Generate the Root and leaf in a temporary sibling directory, write all PEM files plus metadata, then rename the directory into place only after every write succeeds. Use `rcgen` parameters with these exact properties:

```rust
root_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
root_params.key_usages = vec![
    rcgen::KeyUsagePurpose::KeyCertSign,
    rcgen::KeyUsagePurpose::CrlSign,
    rcgen::KeyUsagePurpose::DigitalSignature,
];
root_params.distinguished_name.push(rcgen::DnType::CommonName, ROOT_COMMON_NAME);

let mut leaf_params = rcgen::CertificateParams::new(vec![
    "localhost".to_string(),
    "127.0.0.1".to_string(),
])?;
leaf_params.distinguished_name.push(rcgen::DnType::CommonName, SERVER_COMMON_NAME);
leaf_params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
leaf_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];

let root_key = rcgen::KeyPair::generate()?;
let root_certificate = root_params.self_signed(&root_key)?;
let leaf_key = rcgen::KeyPair::generate()?;
let leaf_certificate = leaf_params.signed_by(&leaf_key, &root_certificate, &root_key)?;
```

Write `root_certificate.pem()`, `root_key.serialize_pem()`, `leaf_certificate.pem()`, and `leaf_key.serialize_pem()` into the temporary sibling directory. Calculate `root_fingerprint_sha256` from `root_certificate.der()` with `Sha256` and `root_thumbprint_sha1` with `Sha1`, uppercase neither value, and persist only those hashes, public paths, expiry, and trust record in `metadata.json`. `RouteProxyHttpsStatus` reads metadata and returns no private key field.

`delete_material` must reject enabled config, a running TLS proxy, and a Root CA whose latest inspected trust status is `SystemTrusted`, `NssTrusted`, or `PartiallyTrusted`; instruct the caller to uninstall the managed Root first so the app never loses the exact identity required for safe removal. Only then call `tokio::fs::remove_dir_all`. It must canonicalize `paths.route_proxy_https_dir.parent()` and the deletion target, require that the canonical target is a direct child named `route-proxy` of the canonical certificate parent, and reject symlinks or paths outside that parent. This prevents a replacement symlink from redirecting deletion outside the managed certificate directory.

Export the model and service modules:

```rust
// src-tauri/src/models/mod.rs
pub mod route_proxy_https;

// src-tauri/src/services/mod.rs
pub mod route_proxy_https_service;
pub mod route_proxy_https_trust;
```

- [ ] **Step 4: Run material and path tests to verify the implementation passes**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml ensure_material_generates_root_and_loopback_leaf_without_exposing_private_key -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml delete_material_rejects_enabled_https_and_removes_only_the_managed_directory -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml app_paths_include_route_proxy_https_paths -- --nocapture
```

Expected: all three tests pass; no serialized status includes PEM private-key content.

- [ ] **Step 5: Commit the persisted material foundation**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/paths.rs src-tauri/src/models/mod.rs src-tauri/src/models/route_proxy_https.rs src-tauri/src/services/mod.rs src-tauri/src/services/route_proxy_https_service.rs
git commit -m "feat: add managed route proxy HTTPS certificates"
```

---

### Task 2: Add Cross-Platform Root Trust Adapters

**Files:**
- Create: `src-tauri/src/services/route_proxy_https_trust.rs`
- Modify: `src-tauri/src/services/route_proxy_https_service.rs`
- Test: unit tests in `route_proxy_https_trust.rs`

**Interfaces:**
- Consumes `RouteProxyHttpsMaterial` and managed metadata (`root_thumbprint_sha1`, Root PEM path, a fixed NSS nickname).
- Produces `RouteProxyTrustOutcome { status, adapter, message, manual_instructions }`.
- Produces `RouteProxyHttpsTrustExecutor::{install, uninstall, inspect}`; the production `SystemRouteProxyHttpsTrustExecutor` and test fakes implement this trait, and both production operations execute only argument-array commands.
- Later orchestration uses these methods without knowing OS command syntax.

- [ ] **Step 1: Write failing pure command-construction tests for every supported adapter**

Create a testable platform enum and pure command shape:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustPlatform {
    WindowsCurrentUser,
    MacOsLoginKeychain,
    LinuxP11Kit,
    LinuxDebian,
    LinuxRhel,
    LinuxNss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrustCommand {
    program: String,
    args: Vec<String>,
}
```

Add these tests:

```rust
#[test]
fn windows_commands_install_and_remove_only_the_managed_thumbprint() {
    let material = fixture_material();
    let install = install_commands(TrustPlatform::WindowsCurrentUser, &material, None);
    let uninstall = uninstall_commands(TrustPlatform::WindowsCurrentUser, &material, None);

    assert_eq!(install, vec![TrustCommand {
        program: "certutil.exe".to_string(),
        args: vec!["-user", "-addstore", "Root", material.root_certificate_pem.to_str().unwrap()],
    }]);
    assert_eq!(uninstall, vec![TrustCommand {
        program: "certutil.exe".to_string(),
        args: vec!["-user", "-delstore", "Root", material.root_thumbprint_sha1.as_str()],
    }]);
}

#[test]
fn macos_commands_target_only_the_login_keychain_and_thumbprint() {
    let material = fixture_material();
    let uninstall = uninstall_commands(TrustPlatform::MacOsLoginKeychain, &material, None);

    assert_eq!(uninstall[0].program, "security");
    assert_eq!(uninstall[0].args, vec![
        "delete-certificate".to_string(),
        "-Z".to_string(),
        material.root_thumbprint_sha1.clone(),
        std::env::var("HOME").unwrap_or_default() + "/Library/Keychains/login.keychain-db",
    ]);
}

#[test]
fn linux_commands_use_fixed_store_paths_and_never_shell_syntax() {
    let material = fixture_material();
    let commands = install_commands(TrustPlatform::LinuxDebian, &material, Some("pkexec"));

    assert_eq!(commands[0].program, "pkexec");
    assert_eq!(commands[0].args[0], "install");
    assert!(commands.iter().all(|command| !command.args.iter().any(|arg| arg.contains("sh -c"))));
    assert!(commands.iter().all(|command| !command.args.iter().any(|arg| arg.contains(";"))));
    assert!(commands.iter().any(|command| command.args.iter().any(|arg| arg == "update-ca-certificates")));
}

#[test]
fn nss_commands_use_the_fixed_ai_switch_nickname() {
    let material = fixture_material();
    let commands = install_commands(TrustPlatform::LinuxNss, &material, None);

    assert_eq!(commands[0].program, "certutil");
    assert!(commands[0].args.windows(2).any(|args| args == ["-n", "AI Switch Route Proxy Root CA"]));
    assert!(commands[0].args.windows(2).any(|args| args == ["-t", "C,,"]));
}
```

- [ ] **Step 2: Run the trust tests and verify they fail before the adapter exists**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_https_trust -- --nocapture
```

Expected: compilation failure naming missing trust adapter symbols.

- [ ] **Step 3: Implement pure commands, controlled execution, status, and manual guidance**

Use this exact platform behavior:

```text
Windows install:   certutil.exe -user -addstore Root <root-ca.pem>
Windows uninstall: certutil.exe -user -delstore Root <sha1-thumbprint>
macOS install:     security add-trusted-cert -d -r trustRoot -k <login-keychain> <root-ca.pem>
macOS uninstall:   security delete-certificate -Z <sha1-thumbprint> <login-keychain>
p11-kit install:   trust anchor <root-ca.pem>
p11-kit uninstall: trust anchor --remove <root-ca.pem>
Debian install:    [pkexec] install -Dm644 <root-ca.pem> /usr/local/share/ca-certificates/ai-switch-route-proxy-<sha256>.crt; [pkexec] update-ca-certificates
Debian uninstall:  [pkexec] rm -f /usr/local/share/ca-certificates/ai-switch-route-proxy-<sha256>.crt; [pkexec] update-ca-certificates --fresh
RHEL install:      [pkexec] install -Dm644 <root-ca.pem> /etc/pki/ca-trust/source/anchors/ai-switch-route-proxy-<sha256>.crt; [pkexec] update-ca-trust extract
RHEL uninstall:    [pkexec] rm -f /etc/pki/ca-trust/source/anchors/ai-switch-route-proxy-<sha256>.crt; [pkexec] update-ca-trust extract
NSS install:       certutil -A -d sql:<HOME>/.pki/nssdb -n "AI Switch Route Proxy Root CA" -t C,, -i <root-ca.pem>
NSS uninstall:     certutil -D -d sql:<HOME>/.pki/nssdb -n "AI Switch Route Proxy Root CA"
```

Define this executor boundary before the production adapter so lifecycle tests never touch the real user's Root store:

```rust
#[async_trait::async_trait]
pub trait RouteProxyHttpsTrustExecutor: Send + Sync {
    async fn install(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome;
    async fn uninstall(
        &self,
        material: &RouteProxyHttpsMaterial,
    ) -> Result<RouteProxyTrustOutcome, AppError>;
    async fn inspect(&self, material: &RouteProxyHttpsMaterial) -> RouteProxyTrustOutcome;
}

pub struct SystemRouteProxyHttpsTrustExecutor;
```

`RouteProxyHttpsService::enable`, `reimport_root_ca`, `regenerate_certificates`, and `uninstall_root_ca` construct `SystemRouteProxyHttpsTrustExecutor`; private `*_with_trust` helpers accept `&dyn RouteProxyHttpsTrustExecutor` so unit tests can pass a deterministic fake. Do not use global mutable test flags, environment variables, or real system trust commands in lifecycle tests.

Implement `run_command` with `tokio::process::Command` only:

```rust
async fn run_command(command: &TrustCommand) -> Result<(), String> {
    let output = tokio::process::Command::new(&command.program)
        .args(&command.args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("Could not run {}: {error}", command.program))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(if stderr.is_empty() { stdout } else { stderr })
}
```

Detect the compiled platform with `cfg!(target_os = ...)`. On Linux, try an executable `trust` adapter first; if it cannot be run, parse `/etc/os-release` and choose Debian/Ubuntu for `ID`/`ID_LIKE` containing `debian` or `ubuntu`, RHEL/Fedora for values containing `rhel`, `fedora`, `centos`, or `suse`; then attempt the user NSS database only when both `certutil` and `$HOME/.pki/nssdb` exist. For Debian/RHEL adapters, first try direct commands; retry the same argument arrays prefixed with `pkexec` only after a permission failure and only when `pkexec` exists.

Use the result rules below:

```rust
// At least one system adapter succeeded and no attempted adapter failed.
RouteProxyTrustStatus::SystemTrusted
// NSS succeeded but no system adapter succeeded.
RouteProxyTrustStatus::NssTrusted
// One attempted adapter succeeded and another attempted adapter failed.
RouteProxyTrustStatus::PartiallyTrusted
// Every attempted adapter failed or none was available.
RouteProxyTrustStatus::Untrusted
```

After every successful install or uninstall attempt, invoke `inspect` before persisting the resulting trust record. Inspection must verify the managed SHA-1 thumbprint and Root common name before reporting `SystemTrusted`: Windows uses `certutil.exe -user -store Root <sha1-thumbprint>`; macOS uses `security find-certificate -Z -c "AI Switch Route Proxy Root CA" <login-keychain>` and matches the reported SHA-1; Debian/RHEL verify the exact app-owned anchor file name and fingerprint; NSS reads the fixed nickname and verifies its exported certificate fingerprint. If an adapter cannot establish its state safely, report `Unknown` rather than `SystemTrusted`.

When no adapter succeeds, return a `manual_instructions` vector with the exact command syntax above and the absolute Root PEM path. Persist the last trust outcome in certificate metadata so `status()` survives app restart, but have `RouteProxyHttpsService::status` call `inspect` best-effort when material exists so manual certificate removal is reflected without needing a re-import. Use `Unknown` only when legacy/missing metadata or safe inspection prevents a determination.

- [ ] **Step 4: Run trust tests and format the new trust module**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_https_trust -- --nocapture
```

Expected: all command-construction tests pass and no command contains a shell fragment.

- [ ] **Step 5: Commit the cross-platform trust adapters**

```powershell
git add src-tauri/src/services/route_proxy_https_trust.rs src-tauri/src/services/route_proxy_https_service.rs
git commit -m "feat: add cross-platform route proxy root trust adapters"
```

---

### Task 3: Serve the Route Proxy Through TLS When HTTPS Is Enabled

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: unit/integration tests in `route_proxy_service.rs`

**Interfaces:**
- Consumes `RouteProxyTransport::{Http, Https { certificate_pem_path, private_key_pem_path }}`.
- Changes `RouteProxyService::start(state, pool, transport)` to select HTTP or TLS.
- Produces unchanged request routing plus `RouteProxyStatus.base_url` with `http://` or `https://`.
- Later tasks resolve `RouteProxyTransport` from persisted HTTPS preference before starting/restarting.

- [ ] **Step 1: Write failing HTTP/TLS listener tests**

Add this transport enum and use it from the tests:

```rust
#[derive(Debug, Clone)]
pub enum RouteProxyTransport {
    Http,
    Https {
        certificate_pem_path: PathBuf,
        private_key_pem_path: PathBuf,
    },
}
```

Add the following focused integration test. It intentionally calls `/v1/models`, which the existing handler answers locally after resolving the platform API key and does not need an upstream credential.

```rust
#[tokio::test]
async fn https_transport_serves_the_existing_route_proxy_handler_and_rejects_plain_http() {
    let temp = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
    let material = RouteProxyHttpsService::ensure_material(&paths).await.expect("material");
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool).await.expect("migrations");
    let key = RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-ai-switch-test")
        .await
        .expect("proxy key");
    let runtime = RouteProxyRuntimeState::default();

    let status = RouteProxyService::start(
        &runtime,
        pool,
        RouteProxyTransport::Https {
            certificate_pem_path: material.server_certificate_pem.clone(),
            private_key_pem_path: material.server_private_key_pem.clone(),
        },
    )
    .await
    .expect("start tls");
    let root = reqwest::Certificate::from_pem(
        &tokio::fs::read(&material.root_certificate_pem).await.expect("root pem"),
    )
    .expect("root certificate");
    let client = reqwest::Client::builder().add_root_certificate(root).build().expect("client");
    let tls_response = client
        .get(format!("{}/v1/models", status.base_url.as_deref().expect("base url")))
        .bearer_auth(key)
        .send()
        .await
        .expect("tls request");

    assert_eq!(status.base_url.as_deref().map(|value| value.starts_with("https://")), Some(true));
    assert_eq!(tls_response.status(), reqwest::StatusCode::OK);
    let plain_error = reqwest::get(format!(
        "http://127.0.0.1:{}/v1/models",
        status.port.expect("port")
    ))
    .await
    .expect_err("plain HTTP must not be served by the TLS listener");
    assert!(plain_error.is_request() || plain_error.is_connect() || plain_error.is_decode());

    RouteProxyService::stop(&runtime).await.expect("stop");
}

#[tokio::test]
async fn http_transport_retains_the_existing_http_base_url() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool).await.expect("migrations");
    let runtime = RouteProxyRuntimeState::default();

    let status = RouteProxyService::start(&runtime, pool, RouteProxyTransport::Http)
        .await
        .expect("start http");

    assert_eq!(status.base_url.as_deref().map(|value| value.starts_with("http://")), Some(true));
    RouteProxyService::stop(&runtime).await.expect("stop");
}
```

- [ ] **Step 2: Run the listener tests and verify they fail because `start` has no transport parameter**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml https_transport_serves_the_existing_route_proxy_handler_and_rejects_plain_http -- --nocapture
```

Expected: compilation failure naming the missing `RouteProxyTransport` and three-argument `start` call.

- [ ] **Step 3: Implement TLS selection while retaining the HTTP server behavior**

Change the `start` signature:

```rust
pub async fn start(
    state: &RouteProxyRuntimeState,
    pool: SqlitePool,
    transport: RouteProxyTransport,
) -> Result<RouteProxyStatus, AppError>
```

Keep the existing `TcpListener::bind((BIND_HOST, 0))` so both transports retain random local ports. Build the existing `Router` exactly once. Derive the scheme before spawning:

```rust
let scheme = match &transport {
    RouteProxyTransport::Http => "http",
    RouteProxyTransport::Https { .. } => "https",
};
let base_url = format!("{scheme}://{BIND_HOST}:{port}");
```

For `Http`, retain the existing `axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())` and existing one-shot graceful shutdown.

For `Https`, load the files before mutating runtime state and use `axum-server` with the already-bound random listener:

```rust
let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
    &certificate_pem_path,
    &private_key_pem_path,
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
let server = axum_server::from_tcp_rustls(std_listener, rustls_config)
    .handle(handle.clone())
    .serve(app.into_make_service_with_connect_info::<SocketAddr>());
```

Pin `server`, wait on either `shutdown_rx` or the server result, and on shutdown call `handle.graceful_shutdown(Some(Duration::from_secs(5)))` before awaiting the pinned server future. Do not mark `inner.running = true` until the TLS configuration has loaded and the task has been constructed.

Update every existing internal caller in this task to pass `RouteProxyTransport::Http`; Task 4 replaces command/web callers with persisted transport resolution.

- [ ] **Step 4: Run listener tests plus existing proxy tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml https_transport_serves_the_existing_route_proxy_handler_and_rejects_plain_http -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml http_transport_retains_the_existing_http_base_url -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service -- --nocapture
```

Expected: TLS request succeeds only with the generated Root CA; plain HTTP fails on that TLS port; existing HTTP request-routing tests still pass.

- [ ] **Step 5: Commit TLS route proxy support**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: serve local route proxy over HTTPS"
```

---

### Task 4: Orchestrate HTTPS Actions, Config Rewrites, and Both Command Transports

**Files:**
- Modify: `src-tauri/src/services/route_proxy_https_service.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src-tauri/src/services/route_config_service.rs`
- Modify: `src-tauri/src/database/repositories/route_proxy_key_repository.rs`
- Create: `src-tauri/src/commands/route_proxy_https_commands.rs`
- Modify: `src-tauri/src/commands/route_proxy_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Test: unit tests in `route_config_service.rs`, `route_proxy_https_service.rs`, and `web/handlers/mod.rs`

**Interfaces:**
- Produces `RouteProxyHttpsOperationOutcome { https, route_proxy, config_writes }`.
- Produces Tauri/Web commands: `get_route_proxy_https_status`, `enable_route_proxy_https`, `disable_route_proxy_https`, `reimport_route_proxy_root_ca`, `regenerate_route_proxy_https_certificates`, `uninstall_route_proxy_root_ca`, `delete_route_proxy_https_certificates`.
- `start_route_proxy` becomes preference-aware and starts the persisted transport.
- Produces `RouteConfigService::write_existing_configs(paths, pool, base_url)` without creating new platform keys.

- [ ] **Step 1: Write failing orchestration, rewrite, and Web dispatch tests**

Add a repository helper and test it first:

```rust
#[tokio::test]
async fn list_platforms_returns_existing_keys_without_creating_new_rows() {
    let pool = crate::database::create_memory_pool().await.expect("pool");
    crate::database::run_migrations(&pool).await.expect("migrations");
    RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex").await.expect("codex");
    RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-grok").await.expect("grok");

    assert_eq!(RouteProxyKeyRepository::list_platforms(&pool).await.expect("platforms"), vec![
        "codex".to_string(),
        "grok".to_string(),
    ]);
}

#[tokio::test]
async fn enable_https_restarts_a_running_http_proxy_and_rewrites_only_existing_platforms() {
    let fixture = app_state_fixture().await;
    RouteProxyKeyRepository::ensure_platform_key(&fixture.pool, "codex", "sk-codex")
        .await
        .expect("codex key");
    RouteProxyService::start(&fixture.route_proxy, fixture.pool.clone(), RouteProxyTransport::Http)
        .await
        .expect("http proxy");

    let outcome = RouteProxyHttpsService::enable(&fixture).await.expect("enable https");

    assert!(outcome.https.enabled);
    assert_eq!(outcome.route_proxy.base_url.as_deref().map(|value| value.starts_with("https://")), Some(true));
    assert_eq!(outcome.config_writes.len(), 1);
    assert_eq!(outcome.config_writes[0].target_key, "codex");
}

#[tokio::test]
async fn failed_root_uninstall_restarts_tls_and_keeps_https_enabled() {
    let fixture = app_state_fixture().await;
    let trust = FakeRouteProxyHttpsTrustExecutor::failing_remove();
    RouteProxyHttpsService::enable_with_trust(&fixture, &trust)
        .await
        .expect("enable https");

    let error = RouteProxyHttpsService::uninstall_root_ca_with_trust(&fixture, &trust)
        .await
        .expect_err("uninstall error");

    assert!(error.to_string().contains("remove"));
    assert!(RouteProxyHttpsService::load_config(&fixture.paths).await.expect("config").enabled);
    assert_eq!(RouteProxyService::status(&fixture.route_proxy).await.base_url.as_deref().map(|url| url.starts_with("https://")), Some(true));
}
```

Add a Web Transport dispatch test that exercises the exact command name:

```rust
#[tokio::test]
async fn dispatch_get_route_proxy_https_status_returns_a_serializable_status() {
    let state = web_app_state_fixture().await;
    let value = dispatch_command(state, "get_route_proxy_https_status", serde_json::json!({}))
        .await
        .expect("status");

    assert_eq!(value["enabled"], serde_json::json!(false));
    assert!(value.get("certificateDir").is_some());
}
```

- [ ] **Step 2: Run the tests and verify they fail because the orchestration API and command routes do not exist**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml enable_https_restarts_a_running_http_proxy_and_rewrites_only_existing_platforms -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml dispatch_get_route_proxy_https_status_returns_a_serializable_status -- --nocapture
```

Expected: compilation failure naming missing `enable`, `list_platforms`, and HTTPS command dispatch branches.

- [ ] **Step 3: Add safe existing-config rewrite support**

Add this repository query without changing `list_all`:

```rust
pub async fn list_platforms(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows = sqlx::query("SELECT platform FROM route_proxy_keys ORDER BY platform ASC")
        .fetch_all(pool)
        .await
        .map_err(|error| AppError::Database {
            code: "database.route_proxy_key_list_platforms",
            message: "Could not load route proxy platforms".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>("platform")).collect())
}
```

Extend `RouteConfigWriteOutcome` with a backward-compatible optional `error` field. Add `get_existing_platform_key` so the scheme-rewrite path can never insert a key, then add this method:

```rust
pub async fn get_existing_platform_key(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Option<String>, AppError> {
    Self::get_by_platform(pool, platform).await
}

pub async fn write_existing_configs(
    paths: &AppPaths,
    pool: &SqlitePool,
    base_url: &str,
) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
    let mut outcomes = Vec::new();
    for platform in RouteProxyKeyRepository::list_platforms(pool).await? {
        match RouteProxyKeyRepository::get_existing_platform_key(pool, &platform).await? {
            Some(route_proxy_key) => match Self::write_existing_config(
                paths,
                base_url,
                &platform,
                &route_proxy_key,
            )
            .await
            {
                Ok(write) => outcomes.push(write),
                Err(error) => outcomes.push(RouteConfigWriteOutcome {
                    target_key: platform,
                    path: String::new(),
                    status: "error".to_string(),
                    route_proxy_key,
                    error: Some(error.to_string()),
                }),
            },
            None => outcomes.push(RouteConfigWriteOutcome {
                target_key: platform,
                path: String::new(),
                status: "skipped".to_string(),
                route_proxy_key: String::new(),
                error: Some("Route proxy key was removed before HTTPS config rewrite".to_string()),
            }),
        }
    }
    Ok(outcomes)
}
```

Add the private `write_existing_config(paths, base_url, platform, route_proxy_key)` helper by extracting the target resolution and `ConfigWriter::write_atomic` portion from `write_configs`. It receives a known key and must not call `ensure_platform_key`. `write_existing_configs` must call only this helper, never `write_configs`, because a concurrent delete between list and write must not recreate a key.

- [ ] **Step 4: Implement HTTPS lifecycle transactions and commands**

Define this operation result in `models/route_proxy_https.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProxyHttpsOperationOutcome {
    pub https: RouteProxyHttpsStatus,
    pub route_proxy: RouteProxyStatus,
    pub config_writes: Vec<RouteConfigWriteOutcome>,
}
```

Implement these service methods with the specified transition behavior:

```rust
pub async fn start_proxy(state: &AppState) -> Result<RouteProxyStatus, AppError>;
pub async fn enable(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError>;
pub async fn disable(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError>;
pub async fn reimport_root_ca(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError>;
pub async fn regenerate_certificates(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError>;
pub async fn uninstall_root_ca(state: &AppState) -> Result<RouteProxyHttpsOperationOutcome, AppError>;
pub async fn delete_certificates(state: &AppState) -> Result<RouteProxyHttpsStatus, AppError>;
pub async fn status_for_state(state: &AppState) -> Result<RouteProxyHttpsStatus, AppError>;
```

Use this exact order for `enable`:

```text
1. Ensure valid certificate material before stopping any listener.
2. Attempt Root CA import; persist its trust outcome even if import fails.
3. Record whether the route proxy is currently running.
4. If it is running, stop HTTP.
5. Start TLS using the managed leaf paths, even when the proxy was initially stopped.
6. If TLS start fails, restart HTTP only when step 3 was true; leave config.enabled false; return the TLS error.
7. Save config.enabled = true.
8. Call write_existing_configs with the running TLS proxy's HTTPS base URL.
9. Return status plus all config-write outcomes.
```

Use this exact order for `disable`:

```text
1. Record whether the proxy is running and stop it when it is.
2. Save config.enabled = false.
3. Start HTTP so disabling HTTPS always restores a usable local capacity-pool endpoint.
4. If HTTP start fails, return an error with HTTPS disabled and proxy stopped.
5. Rewrite existing managed configurations only when HTTP started successfully.
```

Use this exact order for `uninstall_root_ca`:

```text
1. Load config and record whether a TLS proxy is running.
2. Stop the TLS proxy without changing config.enabled.
3. Attempt Root CA removal using the managed trust adapter.
4. If removal fails, restart TLS when it was running, retain config.enabled = true, and return the removal error.
5. If removal succeeds, save config.enabled = false.
6. Restart HTTP when the proxy was running and rewrite existing managed configurations.
```

For `regenerate_certificates`, create replacement material in a sibling temporary directory and retain the prior directory until the new TLS listener starts. When an installed old Root exists, remove it before promoting the replacement. If new TLS start fails, remove the newly imported Root when possible, restore the original directory, re-import the old Root when it was previously trusted, and restart the old TLS proxy. Do not leave the route proxy stopped after a failed regeneration unless restoration itself failed; report both the primary and restoration errors in `AppError.details`. The lifecycle methods use injectable `*_with_trust` helpers with a fake trust executor for all success, fallback, and rollback tests; production commands call the wrappers using `SystemRouteProxyHttpsTrustExecutor`.

Create Tauri commands in `route_proxy_https_commands.rs` that delegate to these service methods. Add `open_route_proxy_https_certificate_dir(app: tauri::AppHandle, state: State<'_, AppState>)`; verify `paths.route_proxy_https_dir` exists, then call `app.shell().open(paths.route_proxy_https_dir.to_string_lossy().as_ref(), None)`. Import `tauri_plugin_shell::ShellExt`. Do not expose this command through Web Transport.

Update desktop `start_route_proxy` and Web dispatch `"start_route_proxy"` to call `RouteProxyHttpsService::start_proxy(&state)` so a saved enabled preference never starts a plain HTTP listener. Add all non-folder HTTPS actions to `tauri::generate_handler![]` and to `dispatch_command` with the exact snake_case names listed in the Interfaces block.

- [ ] **Step 5: Run lifecycle, config rewrite, and Web dispatch tests**

The lifecycle test fixture must construct `AppState` with a temporary `AppPaths`, an in-memory migrated pool, and a fresh `RouteProxyRuntimeState`. It must use `FakeRouteProxyHttpsTrustExecutor`; never invoke Windows/macOS/Linux trust commands or write real `BaseDirs::home_dir()` configuration files during automated tests. Split the existing config writer so `write_existing_config_for_home(home, ...)` accepts a temporary home directory; test HTTPS rewrite output through that helper with a `tempfile::TempDir` rather than `write_configs`' production home-directory lookup.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml list_platforms_returns_existing_keys_without_creating_new_rows -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml enable_https_restarts_a_running_http_proxy_and_rewrites_only_existing_platforms -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml failed_root_uninstall_restarts_tls_and_keeps_https_enabled -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml dispatch_get_route_proxy_https_status_returns_a_serializable_status -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml route_config_service -- --nocapture
```

Expected: the enabled proxy uses HTTPS, only the pre-existing Codex key receives a config write, uninstall failure preserves the TLS endpoint, and Web Transport recognizes the new status command.

- [ ] **Step 6: Commit orchestration and command integration**

```powershell
git add src-tauri/src/database/repositories/route_proxy_key_repository.rs src-tauri/src/services/route_proxy_https_service.rs src-tauri/src/services/route_config_service.rs src-tauri/src/commands/route_proxy_https_commands.rs src-tauri/src/commands/route_proxy_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src-tauri/src/models/route_proxy_https.rs
git commit -m "feat: manage route proxy HTTPS lifecycle"
```

---

### Task 5: Add HTTPS Settings API, UI, and Localized Status

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Create: `src/components/settings/route-proxy-https-settings.tsx`
- Modify: `src/screens/SettingsScreen.tsx`
- Modify: `src/lib/i18n.tsx`
- Modify: `tests/SettingsScreen.test.tsx`

**Interfaces:**
- Consumes backend command names from Task 4.
- Produces `RouteProxyHttpsSettings`, displayed only under a Settings feature entry titled `HTTPS`.
- Uses query keys `route-proxy-https-status` and `route-proxy-status`.
- Does not offer certificate-folder opening in Web Transport; that control appears only when `isDesktop()` is true.

- [ ] **Step 1: Write failing Settings UI tests with API mocks**

Extend the `../src/lib/api/client` mock in `tests/SettingsScreen.test.tsx` with every HTTPS API function. Add this fixture in `beforeEach`:

```ts
vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
  enabled: false,
  certReady: false,
  trustStatus: "untrusted",
  trustAdapter: null,
  rootFingerprint: null,
  expiresAt: null,
  certificateDir: "C:/Users/example/.ai-switch/certs/route-proxy",
  rootCertificatePath: null,
  proxyBaseUrl: null,
  message: null,
  manualInstructions: [],
});
vi.mocked(enableRouteProxyHttps).mockResolvedValue(httpsOutcomeFixture);
```

Add these tests:

```tsx
it("opens the HTTPS settings section and enables the local route proxy", async () => {
  vi.mocked(getSettings).mockResolvedValue(settingsFixture);
  renderSettingsScreen();

  await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
  expect(await screen.findByRole("heading", { name: "HTTPS" })).toBeInTheDocument();
  expect(screen.getByText("本地算力池 HTTPS")).toBeInTheDocument();

  await userEvent.click(screen.getByRole("checkbox", { name: "为本地算力池启用 HTTPS" }));

  await waitFor(() => expect(enableRouteProxyHttps).toHaveBeenCalledTimes(1));
  expect(await screen.findByText(/https:\/\/127\.0\.0\.1:/)).toBeInTheDocument();
});

it("shows untrusted guidance without blocking HTTPS controls", async () => {
  vi.mocked(getSettings).mockResolvedValue(settingsFixture);
  vi.mocked(getRouteProxyHttpsStatus).mockResolvedValue({
    ...httpsStatusFixture,
    enabled: true,
    certReady: true,
    trustStatus: "untrusted",
    manualInstructions: ["certutil.exe -user -addstore Root C:/tmp/root-ca.pem"],
  });
  renderSettingsScreen();

  await userEvent.click(await screen.findByRole("button", { name: /HTTPS/ }));
  expect(await screen.findByText("根证书尚未受信任")).toBeInTheDocument();
  expect(screen.getByText("certutil.exe -user -addstore Root C:/tmp/root-ca.pem")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新导入根证书" })).toBeEnabled();
});
```

- [ ] **Step 2: Run the UI test and verify it fails before API types/component exist**

Run:

```powershell
& "C:\nvm4w\nodejs\pnpm.CMD" vitest run tests/SettingsScreen.test.tsx
```

Expected: TypeScript/Vitest failure naming missing HTTPS API exports and Settings section content.

- [ ] **Step 3: Add TypeScript API types and client functions**

Add these types in `src/lib/api/types.ts`:

```ts
export type RouteProxyTrustStatus =
  | "systemTrusted"
  | "nssTrusted"
  | "partiallyTrusted"
  | "untrusted"
  | "unknown";

export type RouteProxyHttpsStatus = {
  enabled: boolean;
  certReady: boolean;
  trustStatus: RouteProxyTrustStatus;
  trustAdapter?: string | null;
  rootFingerprint?: string | null;
  expiresAt?: string | null;
  certificateDir: string;
  rootCertificatePath?: string | null;
  proxyBaseUrl?: string | null;
  message?: string | null;
  manualInstructions: string[];
};

export type RouteProxyHttpsOperationOutcome = {
  https: RouteProxyHttpsStatus;
  routeProxy: RouteProxyStatus;
  configWrites: RouteConfigWriteOutcome[];
};
```

Update the existing TypeScript `RouteConfigWriteOutcome` in the same change to preserve its current snake_case wire fields and add:

```ts
error?: string | null;
```

Add one direct `invoke` wrapper per backend command in `src/lib/api/client.ts`:

```ts
export function getRouteProxyHttpsStatus(): Promise<RouteProxyHttpsStatus> {
  return invoke("get_route_proxy_https_status");
}
export function enableRouteProxyHttps(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("enable_route_proxy_https");
}
export function disableRouteProxyHttps(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("disable_route_proxy_https");
}
export function reimportRouteProxyRootCa(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("reimport_route_proxy_root_ca");
}
export function regenerateRouteProxyHttpsCertificates(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("regenerate_route_proxy_https_certificates");
}
export function uninstallRouteProxyRootCa(): Promise<RouteProxyHttpsOperationOutcome> {
  return invoke("uninstall_route_proxy_root_ca");
}
export function deleteRouteProxyHttpsCertificates(): Promise<RouteProxyHttpsStatus> {
  return invoke("delete_route_proxy_https_certificates");
}
export function openRouteProxyHttpsCertificateDirectory(): Promise<void> {
  return invoke("open_route_proxy_https_certificate_dir");
}
```

- [ ] **Step 4: Implement the HTTPS Settings card and feature entry**

Create `route-proxy-https-settings.tsx` using `useQuery` for `getRouteProxyHttpsStatus` and `getRouteProxyStatus`. Use `useMutation` per destructive/action operation, and on every successful operation update both cached keys from `RouteProxyHttpsOperationOutcome`:

```ts
queryClient.setQueryData(["route-proxy-https-status"], outcome.https);
queryClient.setQueryData(["route-proxy-status"], outcome.routeProxy);
```

The card must include these controls and exact behavior:

```text
Checkbox label: 为本地算力池启用 HTTPS
Enable checked: enableRouteProxyHttps()
Disable unchecked: disableRouteProxyHttps()
Status values: 未启用 / 证书已就绪 / 根证书已信任 / 根证书尚未受信任 / 部分信任 / 状态未知
Display: proxyBaseUrl, rootFingerprint, expiresAt, certificateDir
Buttons: 生成并导入根证书, 重新导入根证书, 重新生成证书, 卸载根证书, 删除本地证书材料
Desktop-only button: 打开证书目录
Confirm before: 重新生成证书, 卸载根证书, 删除本地证书材料
Untrusted status: render every manualInstructions item in a selectable <code> element
```

Use `isDesktop()` from `src/lib/transport` to hide `打开证书目录` outside Tauri. Do not hide status, manual instructions, or other server-safe settings when opened through Web Service.

In `SettingsScreen.tsx`, import `LockKeyhole` and `RouteProxyHttpsSettings`, extend the entry types with `section: "https"`, and add:

```ts
{
  section: "https",
  titleKey: "settings.https.title",
  descriptionKey: "settings.feature.https",
  icon: LockKeyhole,
}
```

Then render:

```tsx
{activeSection === "https" && <RouteProxyHttpsSettings />}
```

Add matching English and Chinese keys in `i18n.tsx`. The Chinese values must include the exact test labels above; English must express the same states/actions, including that HTTPS applies only to the local capacity pool.

- [ ] **Step 5: Run focused UI tests, typecheck, and full frontend test suite**

Run:

```powershell
& "C:\nvm4w\nodejs\pnpm.CMD" vitest run tests/SettingsScreen.test.tsx
& "C:\nvm4w\nodejs\pnpm.CMD" typecheck
& "C:\nvm4w\nodejs\pnpm.CMD" test:run
```

Expected: Settings tests prove status rendering and enable mutation; typecheck and all existing frontend tests pass without Tauri callback errors.

- [ ] **Step 6: Commit the HTTPS Settings UI**

```powershell
git add src/lib/api/types.ts src/lib/api/client.ts src/components/settings/route-proxy-https-settings.tsx src/screens/SettingsScreen.tsx src/lib/i18n.tsx tests/SettingsScreen.test.tsx
git commit -m "feat: add route proxy HTTPS settings"
```

---

### Task 6: Verify End-to-End Behavior, Document Manual Recovery, and Build

**Files:**
- Modify: `docs/superpowers/specs/2026-07-25-route-proxy-https-design.md` only if implementation names materially differ from the approved interfaces
- Test: existing Rust and frontend suites; manual desktop validation on each supported platform

**Interfaces:**
- Consumes all backend commands, UI actions, generated Root CA, and route proxy status.
- Produces verified HTTP fallback, trusted/untrusted status display, and release-build confidence.

- [ ] **Step 1: Run full automated verification**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
& "C:\nvm4w\nodejs\pnpm.CMD" typecheck
& "C:\nvm4w\nodejs\pnpm.CMD" test:run
```

Expected: all commands exit `0`. Resolve failures before the final commit; do not suppress a failing TLS, Web dispatch, or Settings test.

- [ ] **Step 2: Perform Windows desktop smoke validation**

Start the desktop application and verify this exact sequence:

```text
1. Settings > HTTPS shows disabled, no certificate required.
2. Enable HTTPS generates root-ca.pem/server-cert.pem/server-key.pem under the displayed directory.
3. The status shows an https://127.0.0.1:<port> address and a Root fingerprint.
4. `certutil.exe -user -store Root <sha1-thumbprint>` finds the generated Root CA.
5. Existing generated Codex/Claude/Gemini/Grok route configs reference https:// only for platforms that had a proxy key before the switch.
6. Disable HTTPS changes the running endpoint to http://127.0.0.1:<port>.
7. Re-enable, then uninstall Root CA: `certutil.exe -user -store Root <sha1-thumbprint>` no longer finds that SHA-1 thumbprint and the proxy runs HTTP.
8. Re-enable, force a trust removal failure, and verify the proxy remains HTTPS with config.enabled still true.
```

Use `certutil` with arguments only; do not remove unrelated certificates while testing.

- [ ] **Step 3: Perform macOS and Linux adapter validation**

On macOS validate:

```text
security find-certificate -Z -c "AI Switch Route Proxy Root CA" ~/Library/Keychains/login.keychain-db
```

Then use Settings to uninstall and confirm the matching SHA-1 thumbprint no longer appears.

On Debian/Ubuntu and RHEL/Fedora validate that Settings either reports `systemTrusted` after the selected adapter succeeds or shows a fully copyable manual command using the displayed Root PEM path. On a Linux machine with `$HOME/.pki/nssdb`, validate `certutil -L -d sql:$HOME/.pki/nssdb -n "AI Switch Route Proxy Root CA"` after import and its absence after uninstall.

- [ ] **Step 4: Update the design only for implementation-name drift and commit final verification changes**

If any public command/type name differs from the approved specification, update the exact name in the design document and run `git diff --check`. Otherwise leave the approved design unchanged.

```powershell
git diff --check
git status --short
git add docs/superpowers/specs/2026-07-25-route-proxy-https-design.md
git commit -m "docs: record route proxy HTTPS verification"
```

If the design did not change, do not create an empty commit. Ensure all feature code commits from Tasks 1-5 are present before reporting completion.

## Plan Self-Review

### Spec Coverage

- Settings `HTTPS` section: Task 5.
- Root CA plus leaf certificate SAN for `localhost` and `127.0.0.1`: Task 1.
- Windows/macOS/Linux import and removal: Task 2.
- Automatic import fallback with exact manual guidance: Task 2 and Task 5.
- TLS-only route proxy and HTTPS base URL: Task 3.
- Existing route config scheme refresh without creating unused configs: Task 4.
- HTTP fallback after TLS activation failure and TLS restoration after uninstall failure: Task 4.
- Tauri plus Web Transport command parity: Task 4.
- Private-key redaction, test coverage, and desktop validation: Tasks 1, 3, 5, and 6.

### Placeholder Scan

No `TODO`, `TBD`, deferred implementation marker, or unspecified test action appears in this plan. The terms in this sentence are literal scan targets, not implementation placeholders.

### Type Consistency

- Backend serializes `RouteProxyHttpsStatus` and `RouteProxyHttpsOperationOutcome` with `camelCase`; TypeScript types use the corresponding camelCase fields.
- `AppPaths.route_proxy_https_config_file` is the single config-path name used by Task 1, service persistence, and the global file map.
- `RouteProxyTransport` is created by `RouteProxyHttpsService` and consumed only by `RouteProxyService::start`.
- HTTPS operation commands return `RouteProxyHttpsOperationOutcome`; the folder-open command is intentionally desktop-only and returns `void`.
- `RouteConfigService::write_existing_configs` returns the existing `RouteConfigWriteOutcome` type with only an additive optional `error` field.
- `write_existing_configs` uses `get_existing_platform_key` plus `write_existing_config`, never the key-creating `write_configs` method.
- Lifecycle tests inject `FakeRouteProxyHttpsTrustExecutor`; system trust commands are reserved for manual platform validation.
