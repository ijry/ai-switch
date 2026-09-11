---
title: Web Service Mode
description: Turn on the HTTP server built into the AI Switch desktop app and manage the same account setup from a phone or another computer's browser. Covers setup steps, the default bind address, token auth, and the security rules you must know.
---

# Web Service Mode

The desktop app and the browser run **the same React UI**. The only difference is the transport: the desktop window calls the Rust core over Tauri IPC, while the browser calls the same commands over HTTP and WebSocket. The frontend detects its environment at startup and picks a transport automatically, so features, layout, and interactions are identical. There is no second UI to learn.

Web Service mode fits a few situations: switching accounts from your phone; checking usage from another machine on the same network; keeping the config UI on one always-on machine and reaching it from everything else with just a browser.

## Enabling it from the desktop app

1. Open **Settings** in the desktop app.
2. Select the **Web Service** panel.
3. Under **Shared service port**, choose `127.0.0.1` (local only) or `0.0.0.0` (all interfaces) from the Host dropdown and set the port. The default port is `19527`.
4. Confirm the **Access Token**. A random UUID is generated the first time the config is written; keep it or replace it with your own string.
5. Click **Save**, then **Start service port** when needed.

Once it is up, open `http://127.0.0.1:19527` (or whatever address you configured) in a browser. The first visit asks for the access token, which is then stored in `localStorage` under the key `ai-switch.webToken`, so the same browser will not ask again.

Compute-pool routing is no longer a separate Settings toggle; manage it from the pool toolbar. When routing is enabled but the shared port is stopped, the toolbar shows a start button. Settings still provides **Enable secure network** for exposing the service to your own devices — or the public internet — through Tailscale, with an **Access mode** of either private-only or public. See [Remote Access and HTTPS](/en/deploy/remote-access) for the details.

## The three browser-facing endpoints

| Method and path | Purpose | Auth |
| --- | --- | --- |
| `POST /api/:command` | Single entry point for every command; command name in the path, arguments in the JSON body | Token required |
| `GET /ws/events` | WebSocket event stream: account status, usage, terminal output, and other live events | Token required |
| `GET /health` | Health check for reverse proxies and monitoring | No token |

The token can travel two ways. HTTP requests use the `Authorization: Bearer <token>` header. WebSockets cannot set custom headers, so `/ws/events` also accepts a `?token=<token>` query parameter (it accepts the Bearer header too). Token comparison is constant-time to avoid a timing side channel.

API responses always carry `Cache-Control: no-store`, and the request body limit is 12 MiB (skill-package installs need the headroom). CORS allows GET/POST/OPTIONS from any origin, so other frontends can call the API — but without a token they still get nothing.

A manual call, for reference:

```bash
curl -X POST http://127.0.0.1:19527/api/list_accounts \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

```powershell
curl.exe -X POST http://127.0.0.1:19527/api/list_accounts `
  -H "Authorization: Bearer YOUR_TOKEN" `
  -H "Content-Type: application/json" `
  -d '{}'
```

## The config file

Web service settings persist to `~/.ai-switch/web-service.json`. Every control in the UI maps to a field there, plus three fields that currently have no UI at all:

| Field | Default | Notes |
| --- | --- | --- |
| `host` | `127.0.0.1` | Desktop Settings offers `127.0.0.1` and `0.0.0.0`; the latter binds every interface |
| `port` | `19527` | Listening port |
| `token` | Random UUID written on first config creation | Access token |
| `routeAccessEnabled` | `false` | Whether compute-pool model routes are accepted, controlled from the pool toolbar. Legacy `autoStart: true` migrates to `true` |
| `tailscaleEnabled` | `false` | Whether to expose the service through Tailscale |
| `tailscaleExposureMode` | `private` | `private` (tailnet only) or `public` (Funnel) |
| `tlsEnabled` | `false` | Enable TLS. **No UI toggle; file only** |
| `tlsCertPath` | empty | Path to the certificate chain PEM. **No UI field** |
| `tlsKeyPath` | empty | Path to the private key PEM. **No UI field** |

Restart the web service after editing the file. `tlsCertPath` and `tlsKeyPath` must be supplied together; providing only one fails startup with `web.tls_paths_incomplete`.

## Bind address and plaintext HTTP

The default `127.0.0.1` is local-only. Desktop Settings can select `0.0.0.0` so other devices on the LAN can connect directly over HTTP; the desktop app no longer refuses startup merely because TLS is disabled.

With `0.0.0.0`, every Web command — including terminal, credential export, proxy-key, MCP, and skill operations — is callable over plaintext HTTP. The access token is the only protection. Use a strong random token, expose it only on a trusted LAN, and configure the firewall yourself; prefer Tailscale or TLS across untrusted networks.

This relaxation applies only to the desktop app. The standalone server still rejects non-loopback plaintext HTTP by default unless TLS is configured or `AI_SWITCH_ALLOW_INSECURE_HTTP=1` is explicitly set.

## Commands that are unavailable in a browser

Two categories do not reach the browser.

**Desktop-only commands** (three of them) need native desktop capabilities and return a "desktop only" result over HTTP: opening the certificate directory, launching a session in your system terminal app, and exporting credentials through a native save dialog.

**Sensitive commands** include exporting credentials, previewing and performing credential imports, reading the proxy key, installing MCP servers, changing local MCP configuration, and saving/deleting/installing skills. On a direct desktop listener bound to `127.0.0.1` or `0.0.0.0`, they are available once the service is running and access-token authentication succeeds; plaintext `0.0.0.0` does not downgrade their permissions.

When the desktop stays on loopback and Tailscale provides the external path, the runtime gate still follows the sidecar state:

- loopback HTTP is available while Tailscale is off;
- public Tailscale access becomes available after its HTTPS endpoint is ready;
- while the external path is not ready, sensitive commands temporarily return 404 "Web command is not available".

The desktop Web service refuses to start with an empty or too-short token, so the token is mandatory in practice.

Terminal commands (create session, write input, resize, kill session, list sessions) **are** available over the web API. That means anyone holding the token can open a shell on your machine. Protect the token the way you would protect an SSH private key.

## Security notes

::: warning Before you turn this on
- **Set an access token, and make it random.** Every `/api/*` and `/ws/events` request needs it, and the service rejects empty or too-short tokens at startup.
- **Do not bind `0.0.0.0` casually.** The default `127.0.0.1` is local-only. `0.0.0.0` exposes every Web command directly to the LAN, and without TLS the token is the only protection.
- **The token is equivalent to shell access.** The web API exposes terminal session commands, so a leaked token means command execution on that machine, not just config disclosure.
- **The token is stored in browser localStorage.** After using a shared or public device, sign out and clear site data.
- **Rotation is manual.** After changing the token, restart the service and re-enter it in every browser.
- **Think hard before exposing this publicly.** Prefer a Tailscale private network for remote access, and enable Funnel only when you genuinely need it. In both cases AI Switch's own token check still applies.
:::

## Next steps

- No desktop environment on your server? Run `ai-switch-server` — see [Standalone Server](/en/deploy/standalone-server).
- Need access from outside your network, or HTTPS for the local proxy? See [Remote Access and HTTPS](/en/deploy/remote-access).
- Wondering what desktop and web share? See [Desktop](/en/deploy/desktop).
- Curious how one command layer serves both transports? See [Architecture](/en/dev/architecture).
