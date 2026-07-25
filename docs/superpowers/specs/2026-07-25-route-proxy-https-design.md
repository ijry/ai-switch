# Route Proxy HTTPS Design

Date: 2026-07-25  
Status: Approved for planning

## Goal

Add optional HTTPS to the local route proxy (the local capacity-pool endpoint). The application generates and manages its own Root CA and loopback server certificate, installs or removes the Root CA from the current user's trusted stores on Windows, macOS, and Linux, and writes the active proxy URL with the correct scheme into managed agent configurations.

## Non-Goals

- HTTPS support for the Web Service remote-access server
- User-supplied certificate/key import
- Machine-wide Windows certificate installation requiring elevation
- Public DNS certificates or Internet-exposed HTTPS endpoints
- Running HTTP and HTTPS route proxy listeners concurrently

## Confirmed Decisions

1. Scope is only the local route proxy / capacity-pool endpoint.
2. Use an application-managed Root CA plus a server leaf certificate, not a directly self-signed server certificate.
3. Prefer automatic Root CA installation; when installation fails, retain the certificate files and show exact manual guidance.
4. Support Root CA removal from the UI.
5. Enabling HTTPS runs the route proxy in HTTPS-only mode. HTTP is not kept as a compatibility listener.
6. Automatic install and uninstall must support Windows, macOS, and Linux. Linux uses the available distribution trust-store tools plus optional NSS support.

## Current Behavior

- `RouteProxyService` binds `127.0.0.1` to a random port using `TcpListener` and `axum::serve`.
- The active proxy `base_url` is always `http://127.0.0.1:<port>`.
- `write_route_proxy_configs` consumes the runtime base URL, so a route proxy scheme change can flow into managed Codex, Claude, and Grok configurations without platform-specific branches.
- Settings currently contains a Web Service section but no local route-proxy transport settings.

## Architecture

### HTTPS Certificate Service

Introduce a dedicated backend service responsible for certificate material, trust-store integration, and persisted HTTPS settings. It owns four concerns:

1. Generate or validate a local Root CA and server leaf certificate.
2. Persist certificate material and metadata in a dedicated application-data directory, for example `certs/route-proxy/`.
3. Install and remove only the application-generated Root CA from the current user's platform trust stores.
4. Return safe status data for the UI and route proxy runtime.

Certificate material:

- Root CA private key and certificate.
- Server private key and certificate signed by the Root CA.
- Leaf certificate SAN values: `127.0.0.1` and `localhost`.
- Persisted metadata: certificate fingerprint, generation time, expiry, trust-install outcomes, and user preference for HTTPS enabled.

Private keys are never returned to the UI or included in log/error details.

### Route Proxy TLS Runtime

`RouteProxyService` keeps its existing router, request handling, proxy-key cache, random-port allocation, shutdown mechanism, and status shape. Its listener implementation gains a transport selection:

- HTTPS disabled: current HTTP `axum::serve` behavior and an `http://` base URL.
- HTTPS enabled: serve the same router through a Rust TLS acceptor using the managed leaf certificate and return an `https://` base URL.

The proxy remains loopback-only. Enabling HTTPS does not create a second HTTP listener. When HTTPS is enabled, the route proxy exposes only `https://127.0.0.1:<port>`.

### Settings Integration

Add an `HTTPS` section in Settings, separate from Web Service settings. It controls only the local route proxy.

The section displays:

- Enable/disable HTTPS for the local capacity pool.
- Certificate readiness.
- Trust state: `system_trusted`, `nss_trusted`, `partially_trusted`, `untrusted`, or `unknown`.
- Current HTTPS base URL when the proxy is running.
- Root CA fingerprint, expiry, and certificate directory.
- Readable platform-specific error and manual-install instructions when trust setup fails.

Actions:

- Enable HTTPS (generates and attempts to trust certificates when needed).
- Disable HTTPS (returns the proxy to HTTP).
- Generate and import Root CA.
- Re-import Root CA.
- Regenerate certificate material.
- Uninstall Root CA.
- Open the certificate directory.
- Delete local certificate material, guarded by confirmation.

### Configuration Flow

Managed route configuration writers keep consuming the route proxy runtime `base_url`. When the proxy restarts into HTTPS mode, writers receive the new `https://` base URL and rewrite managed local agent configuration as they already do for endpoint changes.

The UI warns that manually maintained external client configurations cannot be rewritten by the application and must be updated with the new address.

## Certificate Lifecycle

### Generate

On first HTTPS enable, the certificate service validates existing material. If it is missing, invalid, expired, or lacks the required SAN values, it generates a new Root CA and leaf certificate.

`Regenerate` first attempts to remove the old application Root CA if it was installed, then replaces the Root CA and leaf certificate and attempts installation of the new Root CA. Failure to remove the old Root CA is surfaced explicitly and does not claim success.

### Trust Installation

Root CA installation is best-effort and does not prevent TLS proxy startup. The system state determines the UI status:

- `system_trusted`: platform system trust store contains the Root CA.
- `nss_trusted`: user NSS trust database contains the Root CA but no system-store confirmation exists.
- `partially_trusted`: one required or attempted store accepted the certificate while another did not.
- `untrusted`: installation failed or was not performed.
- `unknown`: store state cannot be inspected reliably.

Platform adapters:

- Windows: install/remove in the current user's `Root` certificate store, matched by the application Root CA fingerprint.
- macOS: install/remove in the current user's `login` Keychain, matched by fingerprint.
- Linux: select an available supported adapter in this order where practical:
  - `p11-kit` / `trust anchor`
  - Debian/Ubuntu CA store with `/usr/local/share/ca-certificates` and `update-ca-certificates`
  - RHEL/Fedora CA store with `/etc/pki/ca-trust/source/anchors` and `update-ca-trust`
  - current-user NSS database with `certutil` when available

No platform adapter uses string-built shell commands. Commands receive validated executable paths and argument arrays. When tooling, permissions, or policies prevent installation, the status includes the attempted adapter and exact manual steps using the generated certificate path.

### Uninstall

`Uninstall Root CA` only removes a Root CA that matches the application-managed fingerprint and subject identity. It never enumerates/removes unrelated certificates.

When HTTPS is active, uninstall first stops the TLS proxy without persisting a transport change, then attempts Root CA removal. A successful removal disables HTTPS and restarts the proxy in HTTP mode. A failed removal restarts the TLS proxy and retains the enabled state. Certificate files stay on disk so the user can re-import without regeneration. Local certificate deletion is a separate explicitly confirmed operation.

## Runtime State Transitions

### Enable HTTPS

1. Validate or generate certificate material.
2. Attempt Root CA installation and record trust outcome.
3. Stop a running HTTP route proxy.
4. Start the route proxy with TLS using the leaf certificate.
5. Store HTTPS enabled only after the TLS proxy starts successfully.
6. Rewrite managed configurations using the resulting HTTPS base URL.

If certificate generation fails, leave the running HTTP proxy and persisted enabled state unchanged. If TLS startup fails after stopping HTTP, restart HTTP and report that HTTPS did not activate.

### Disable HTTPS

1. Stop the running TLS route proxy.
2. Persist HTTPS disabled.
3. Restart the route proxy in HTTP mode.
4. Rewrite managed configurations with the resulting HTTP base URL.

If HTTP restart fails, report the proxy as stopped; never report a usable endpoint that was not successfully bound.

### Uninstall Root CA

1. Stop the running TLS route proxy without changing persisted HTTPS settings.
2. Remove the application Root CA from the active platform stores.
3. On success, persist HTTPS disabled and restart the proxy in HTTP mode when it was previously running.
4. On failure, restart the TLS proxy and retain HTTPS enabled.
5. Preserve certificate files unless the user separately selects deletion.

If uninstall fails, preserve the HTTPS state and report the failure rather than presenting a false success state.

## Error Handling

All certificate and trust operations return structured data containing:

- operation name
- selected platform adapter
- resulting trust state
- safe user-readable message
- optional diagnostics
- certificate directory or exported Root CA path for manual recovery

Specific failure behavior:

- Root trust import failure does not block HTTPS serving; the UI explains that strict clients and browsers may reject the endpoint until the Root CA is trusted.
- Leaf certificate read/parse failure prevents HTTPS enable and leaves current proxy behavior unchanged.
- TLS bind/serve failure triggers an HTTP restart attempt to avoid leaving the capacity pool unavailable.
- Missing external Linux tooling produces manual instructions, not a generic failure.
- Deletion refuses to remove material while HTTPS is enabled or the TLS proxy is using it.

## Testing

### Rust Unit Tests

1. Generated leaf certificate contains `localhost` and `127.0.0.1` SAN values.
2. Certificate metadata returns fingerprint and expiry without exposing private-key content.
3. HTTPS settings state transitions persist correctly.
4. HTTPS route proxy status uses an `https://` base URL.
5. HTTP route proxy status retains the existing `http://` URL behavior.
6. Platform adapter command builders match only the managed Root CA fingerprint and use argument arrays.
7. TLS startup failure causes the expected HTTP fallback/recovery behavior.

### Rust Integration Tests

1. A TLS client trusting the generated Root CA can call the route proxy and reach the existing handler.
2. HTTPS-only mode does not expose an HTTP listener.
3. Existing HTTP proxy request behavior remains unchanged when HTTPS is disabled.

### Frontend Tests

1. HTTPS settings section renders all states and current base URL.
2. Enable flow exposes trust progress and untrusted/manual guidance.
3. Regenerate, re-import, uninstall, and delete operations use confirmations where required.
4. The UI refreshes route proxy status after a transport transition.

### Manual Verification

Verify certificate generation, automatic trust installation, TLS route proxy access, and Root CA removal on:

- Windows
- macOS
- Debian/Ubuntu Linux
- Fedora/RHEL Linux

Also verify at least one system-trust consumer and one NSS/browser consumer where NSS integration is available.

## Acceptance Criteria

1. Settings contains an `HTTPS` section dedicated to the local route proxy.
2. A user can enable local route proxy HTTPS with one action; the app generates an application-specific Root CA and loopback leaf certificate as needed.
3. The route proxy returns an `https://127.0.0.1:<port>` base URL and serves only TLS while enabled.
4. Managed agent configurations receive the active route proxy URL without platform-specific HTTPS branches.
5. Windows, macOS, and Linux all have automatic Root CA import and uninstall paths; unsupported or restricted cases expose exact manual instructions.
6. Root CA uninstall only targets the application-generated certificate and disables HTTPS on success.
7. TLS activation failures do not permanently break an already usable HTTP capacity pool.
8. Private keys never appear in the UI, logs, status responses, or manual instructions.
