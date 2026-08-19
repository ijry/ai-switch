---
title: Remote Access and HTTPS
description: Reach AI Switch over a Tailscale private network or Funnel, and generate and trust a self-signed root certificate for the local pool proxy. Includes real certificate paths, per-platform trust commands, and default states.
---

# Remote Access and HTTPS

This page covers two independent things. Separate them before reading on:

- **Remote access**: letting other devices reach the AI Switch management UI on your machine (the web service, port `3090` by default).
- **Local pool HTTPS**: making the local routing proxy (port `19527` by default) serve `https://` for clients that only accept HTTPS upstream URLs.

Different ports, different certificate stories. Do not conflate them.

## Two routes to remote access

| Route | Visibility | Transport encryption | Best for |
| --- | --- | --- | --- |
| Tailscale private network | Only devices on your own Tailscale account | Provided by Tailscale | Reaching an always-on home machine from a phone or laptop |
| Tailscale Funnel | Anyone on the internet can reach the address | HTTPS provided by Tailscale | When you genuinely need to serve external clients |

Neither route changes AI Switch's own authentication: **the access token is still required**. Tailscale solves reachability and channel encryption; it is not a substitute for application-level authorisation.

## Tailscale private network

AI Switch ships a Go sidecar (`ai-switch-tsnet`, built on Tailscale's `tsnet` library). It joins your tailnet in userspace, outside the app process — no system Tailscale client to install and no administrator privileges required.

### Turning it on

1. In the desktop app, go to **Settings → Web Service** and enable **secure network**.
2. Set **Access mode** to private only.
3. Save and start the service.
4. In the **secure network** area, click **Sign in with OAuth**. A browser opens the Tailscale authorisation page; complete it and return to the app.

After sign-in the UI shows the reachable addresses, in this shape:

```text
http://100.x.y.z:3090
http://ai-switch.<your-tailnet>.ts.net:3090
```

The first is the tailnet IP, the second the MagicDNS name. The sidecar uses the hostname `ai-switch` by default and keeps its state in `~/.ai-switch/tailscale/`.

### The other sign-in path: auth keys

If a browser OAuth flow is impractical — for example when you are SSH'd into a headless machine — paste a Tailscale **auth key** into the same area and click connect with auth key. The key is persisted to `~/.ai-switch/tailscale/auth-key` so the node can reconnect after a restart.

::: warning The auth key is stored in plaintext
`~/.ai-switch/tailscale/auth-key` is a plaintext file. If you do not need automatic reconnection, revoke the key in the Tailscale admin console afterwards, or use OAuth sign-in instead.
:::

### Sign-in never happens on its own

Worth stressing: **the app does not sign in to Tailscale at startup.**

Startup recovery only attempts a reconnect if one of these exists: a saved auth key, or previously persisted tsnet login state in the sidecar directory. With neither, the UI reports that the secure network is waiting for sign-in and waits for you to click.

In other words, installing the app does not join you to any network. You have to perform an explicit authorisation step.

## Tailscale Funnel (public)

Switch **Access mode** to public and the sidecar listens through Tailscale Funnel instead, which turns the service into a publicly reachable HTTPS address:

```text
https://ai-switch.<your-tailnet>.ts.net
```

Funnel uses port 443 by default (443, 8443, and 10000 — the ports Funnel supports — are preserved as configured). Things to know:

- **Your Tailscale account must permit Funnel.** That is a tailnet policy setting, enabled in the Tailscale admin console.
- **HTTPS comes from Tailscale**, which issues and renews the certificate. You do not supply one.
- **The access token still applies.** Anyone can reach the address, but without the token they get nothing. At that point the token is the only door.
- **Sign-in works the same way.** Switching to public mode does not skip Tailscale sign-in.

## Bringing your own certificate for the web service

If you skip Tailscale and bind the web service directly to a non-loopback address, you must supply a TLS certificate yourself. Otherwise the service refuses to start with the error `web.sensitive_transport_requires_tls`.

- Desktop: edit `~/.ai-switch/web-service.json` and set `tlsEnabled`, `tlsCertPath`, and `tlsKeyPath` (there are no UI fields for these three), then restart the web service.
- Standalone server: set the environment variables `AI_SWITCH_TLS_CERT_PATH` and `AI_SWITCH_TLS_KEY_PATH`.

Both paths must be supplied together; only one fails with `web.tls_paths_incomplete`. See [Web Service Mode](/en/deploy/web-service) and [Standalone Server](/en/deploy/standalone-server).

## Local pool HTTPS

This has nothing to do with remote access. It solves a different problem: **some clients only accept `https://` upstream URLs**, while the AI Switch local routing proxy defaults to `http://127.0.0.1:19527`. To bridge that, the app can generate a self-signed certificate chain so the local proxy serves HTTPS.

### It is off by default

The defaults in `route-proxy-https.json` are `enabled: false` and `autoStart: false`. Meaning: **unless you turn it on yourself, local HTTPS stays off and no certificate is generated**. Installing the app writes nothing to your system trust store.

The proxy itself always binds `127.0.0.1`, starting at port `19527` and walking up to the next free port if that one is taken. Enabling HTTPS does not make it listen on an external address.

### Where the certificates live

Once enabled, the certificate material is generated in `~/.ai-switch/certs/route-proxy/`:

| File | Contents |
| --- | --- |
| `root-ca.pem` | Self-signed root certificate — this is the file you import into the trust store |
| `root-ca-key.pem` | Root private key (mode 0600 on Unix) |
| `server-cert.pem` | The server certificate the proxy actually serves |
| `server-key.pem` | Server private key (mode 0600 on Unix) |
| `metadata.json` | Root SHA-256 fingerprint, SHA-1 thumbprint, validity dates |

Certificate parameters: the root CN is `AI Switch Route Proxy Root CA` with 3650 days of validity; the server CN is `AI Switch Route Proxy localhost` with 825 days, and its SANs are exactly `localhost` and `127.0.0.1`. Because there is no external hostname in the SAN list, this material **is only valid for local access** — it cannot be reused for another machine.

### Trusting it

Everything happens in the **local pool HTTPS** panel in the desktop app. The available actions are:

- **Generate and import root certificate**: generate the material and write it to the system trust store in one step.
- **Re-import root certificate**: for when the files are still there but the trust state was lost (for example after the trust store was cleaned).
- **Regenerate certificates**: retire the old material and produce a fresh set. Generation happens in a temporary directory with an atomic swap, rolling back to `.backup` on failure, so a half-written state is never left behind.
- **Uninstall root certificate**: remove it from the system trust store while keeping the files.
- **Delete local certificate material**: delete the files entirely, allowed only once HTTPS is off and the root certificate has been uninstalled.
- **Open certificate directory**: open that directory in your file manager (desktop only).

The panel also shows the root fingerprint, expiry date, certificate directory, and the current trust status (system-trusted, NSS-trusted, partially trusted, untrusted, or unknown).

If automatic import fails — usually from insufficient privileges or a non-standard distribution — the panel prints **manual trust steps**. The commands it uses, per platform:

```powershell
certutil.exe -user -addstore Root "$HOME\.ai-switch\certs\route-proxy\root-ca.pem"
```

That writes to the **current user's** trust store (`-user`), so no administrator privileges are needed.

macOS writes to the login keychain:

```bash
security add-trusted-cert -r trustRoot -k ~/Library/Keychains/login.keychain-db \
  ~/.ai-switch/certs/route-proxy/root-ca.pem
```

Linux varies by distribution — pick the line that matches yours:

```bash
# p11-kit (Arch, Fedora, and others)
trust anchor ~/.ai-switch/certs/route-proxy/root-ca.pem

# Debian / Ubuntu
sudo install -Dm644 ~/.ai-switch/certs/route-proxy/root-ca.pem \
  /usr/local/share/ca-certificates/ai-switch-route-proxy-root-ca.crt
sudo update-ca-certificates

# RHEL / CentOS / Fedora
sudo install -Dm644 ~/.ai-switch/certs/route-proxy/root-ca.pem \
  /etc/pki/ca-trust/source/anchors/ai-switch-route-proxy-root-ca.pem
sudo update-ca-trust extract
```

Firefox and some other NSS-based applications ignore the system trust store and need a separate import:

```bash
certutil -A -d <nss-db-path> -n "AI Switch Route Proxy Root CA" -t C,, \
  -i ~/.ai-switch/certs/route-proxy/root-ca.pem
```

The NSS nickname must match the root CN — `AI Switch Route Proxy Root CA` — because uninstall looks the certificate up by that name.

The manual steps in the panel are generated from the real paths on your machine, so copying the commands from the panel is more reliable than copying them from this page.

### Private keys never reach the UI

The proxy HTTPS status endpoint returns the certificate directory, the root certificate path, trust status, expiry, and the manual steps. It **never** returns private key material.

## Security notes

::: warning Ground rules for remote access
- **Every `/api/*` and `/ws/events` request needs the access token, including over Tailscale.** Tailscale is not an authentication layer; it only provides the channel.
- **Tailscale sign-in is a manual action.** The app never signs in at startup; it only attempts a reconnect when a saved auth key or prior tsnet state exists locally.
- **Think before binding `0.0.0.0`.** Non-loopback listeners require TLS (or startup is refused), and it means every device on that network can touch the port.
- **Think harder before enabling Funnel.** That is a public address, and the token is the only door. Prefer private mode unless you truly need otherwise.
- **The token is equivalent to shell access.** The web API includes terminal session commands, so a leaked token costs more than config disclosure.
- **The self-signed root is for this machine only.** Its SANs are just `localhost` and `127.0.0.1`. Never copy the root private key to another machine, and never use it to issue certificates for remote access.
- **Clean up when you are done.** After disabling local HTTPS, use the panel's uninstall and delete actions to remove the root certificate from the system trust store rather than leaving an unused root sitting there.
:::

## Next steps

- To revisit web service setup and endpoints, see [Web Service Mode](/en/deploy/web-service).
- To deploy a headless instance on a server, see [Standalone Server](/en/deploy/standalone-server).
- To understand how the local proxy routes requests, see [Protocol Routing and Bridging](/en/guide/protocol-routing).
- If something will not connect, start with the [FAQ](/en/faq).
