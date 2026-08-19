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
3. Fill in **Host** and **Port**. The defaults are `127.0.0.1` and `3090`.
4. Confirm the **Access Token**. A random UUID is generated the first time the config is written; keep it or replace it with your own string.
5. Click **Save**, then **Start Service**.

Once it is up, open `http://127.0.0.1:3090` (or whatever address you configured) in a browser. The first visit asks for the access token, which is then stored in `localStorage` under the key `ai-switch.webToken`, so the same browser will not ask again.

The panel has two more optional toggles:

- **Auto-start on launch**: bring the web service up automatically when the desktop app starts.
- **Enable secure network**: expose the service to your own devices — or the public internet — through Tailscale, with an **Access mode** of either private-only or public. See [Remote Access and HTTPS](/en/deploy/remote-access) for the details.

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
curl -X POST http://127.0.0.1:3090/api/list_accounts \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

```powershell
curl.exe -X POST http://127.0.0.1:3090/api/list_accounts `
  -H "Authorization: Bearer YOUR_TOKEN" `
  -H "Content-Type: application/json" `
  -d '{}'
```

## The config file

Web service settings persist to `~/.ai-switch/web-service.json`. Every control in the UI maps to a field there, plus three fields that currently have no UI at all:

| Field | Default | Notes |
| --- | --- | --- |
| `host` | `127.0.0.1` | Bind address. Non-loopback addresses require TLS — see the next section |
| `port` | `3090` | Listening port |
| `token` | Random UUID written on first config creation | Access token |
| `autoStart` | `false` | Start the service when the desktop app launches |
| `tailscaleEnabled` | `false` | Whether to expose the service through Tailscale |
| `tailscaleExposureMode` | `private` | `private` (tailnet only) or `public` (Funnel) |
| `tlsEnabled` | `false` | Enable TLS. **No UI toggle; file only** |
| `tlsCertPath` | empty | Path to the certificate chain PEM. **No UI field** |
| `tlsKeyPath` | empty | Path to the private key PEM. **No UI field** |

Restart the web service after editing the file. `tlsCertPath` and `tlsKeyPath` must be supplied together; providing only one fails startup with `web.tls_paths_incomplete`.

## The hard rule about bind address and TLS

The default `127.0.0.1` means local-only. To reach the service from other devices on your network you must explicitly set `host` to `0.0.0.0` or a specific LAN IP — and here is the rule that is **enforced in code**:

> When listening on a non-loopback address without TLS enabled, the service **refuses to start** and returns the error `web.sensitive_transport_requires_tls`.

That is a hard validation, not a warning. The reason: some commands export credentials, read proxy keys, and install MCP servers and skills. Exposing those over plaintext HTTP on a network is too risky to allow. So there are three ways to serve non-local clients:

1. Expose through Tailscale and keep `host` on loopback (recommended — see [Remote Access and HTTPS](/en/deploy/remote-access)).
2. Bring your own certificate and set `tlsEnabled` / `tlsCertPath` / `tlsKeyPath` in `web-service.json`.
3. Stay on loopback and put a reverse proxy in front that terminates TLS itself.

## Commands that are unavailable in a browser

Two categories do not reach the browser.

**Desktop-only commands** (three of them) need native desktop capabilities and return a "desktop only" result over HTTP: opening the certificate directory, launching a session in your system terminal app, and exporting credentials through a native save dialog.

**Sensitive commands** sit behind a runtime gate and only open when the transport is judged safe. They are: exporting credentials, previewing a credential import, importing credentials, reading the proxy key, installing an MCP server from the marketplace, adding/updating a local MCP server, setting an MCP server's app bindings, removing an MCP server, and saving/deleting/installing skills. When the gate is closed they return 404 "Web command is not available" rather than 401 — so the response cannot be used to probe which commands exist.

The transport counts as safe when any of these hold:

- the connection is HTTPS;
- the connection is HTTP on a loopback address and Tailscale is off;
- the connection is HTTP on a loopback address and Tailscale is connected in public (Funnel) mode — in which case Tailscale provides HTTPS for the external hop.

One more thing to know: **when the access token is empty, sensitive commands always return 401**, while ordinary commands are not authenticated at all. So although the token is technically optional, in practice it is mandatory.

Terminal commands (create session, write input, resize, kill session, list sessions) **are** available over the web API. That means anyone holding the token can open a shell on your machine. Protect the token the way you would protect an SSH private key.

## Security notes

::: warning Before you turn this on
- **Set an access token, and make it random.** Every `/api/*` and `/ws/events` request needs it. An empty token blocks all sensitive commands and leaves ordinary ones completely unprotected.
- **Do not bind `0.0.0.0` casually.** The default `127.0.0.1` is local-only. Before changing it, know which devices share that network — and configure TLS, or the service will not start at all.
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
