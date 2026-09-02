---
title: FAQ
description: Answers to common AI Switch questions — how it differs from hand-editing CLI configs, platform coverage, protocol bridging, the difference between ports 19527 and 3090, key storage, quota failover, phone access, backups, and licensing.
---

# FAQ

## How is this different from just editing my CLI config files?

Hand-editing a config file switches an account once. AI Switch is about everything that happens *after* the switch.

Concretely:

- **One change, several CLIs.** Codex, Claude Code, Gemini CLI, and Grok all use different config formats. AI Switch writes each one's native format from a single interface.
- **Writes are safe.** Before every config write it takes a snapshot and records a hash, writes atomically, detects concurrent modification, and supports guarded rollback. Hand-editing has none of that.
- **Switching can be automatic.** When an account hits a cooldown or exhausts its quota, requests fall through to the next account in the pool. You do not have to notice an error and go edit a file.
- **You can see what happened.** Usage, token counts, pricing, failure reasons, and the raw upstream error body are all recorded.

If you have exactly one account and never switch, hand-editing really is enough. The difference shows up once you have several accounts, or once you want failover to happen without you. See [quick start](/en/guide/quick-start) and [accounts and the pool](/en/guide/accounts).

## Which platforms are supported, and why do OpenCode, OpenClaw, and Hermes only get generic API routing?

Seven platforms. The first four are natively supported; the last three get generic API routing.

| Platform | Support |
| --- | --- |
| Codex | Native: API routing, config writing, official account import and quota |
| Claude Code | Native: the same, with quota where the upstream account flow allows it |
| Gemini CLI | Native: API routing, config writing, import; official quota is not claimed |
| Grok | Native: the same as Claude Code |
| OpenCode | Generic API routing |
| OpenClaw | Generic API routing |
| Hermes | Generic API routing |

Those three are limited because AI Switch does not claim it can reliably parse and rewrite their native configuration, import official accounts, or read official quota. They remain fully usable as API routing accounts, can be launched from the terminal, and participate in session workflows — but creating a credential for them **requires an explicit base URL and API dialect**, because AI Switch will not guess a default on their behalf.

The full capability matrix (ten platform capabilities across seven platforms) is in the [platform support matrix](/en/guide/platform-support).

## What is protocol bridging, and when do I need it?

Different CLIs speak different dialects, and so do different upstream services. Protocol bridging is the translation in between.

AI Switch supports **four upstream protocols**: `openai`, `openai-responses`, `anthropic`, and `gemini`. The local entry protocol is fixed — the Codex entry point speaks OpenAI Responses, the Claude entry point speaks Anthropic Messages. When the entry protocol and your chosen account's upstream protocol disagree, a bridge kicks in; there are **seven bridge paths** in total.

**When you need it:** you have an Anthropic-protocol account but want to use it from Codex CLI, or you have an OpenAI-compatible third-party endpoint and want Claude Code to reach it. In both cases you change nothing in the CLI — you just pick the account's upstream protocol in AI Switch.

**When you can ignore it:** the account protocol already matches the CLI (Claude Code with an Anthropic account, for instance). No bridge engages and the request is forwarded as-is.

How each path behaves is covered in [protocol routing and bridging](/en/guide/protocol-routing).

## What is the difference between port 19527 and port 3090?

These are two completely different things, and mixing them up is the most common source of confusion:

| | **Local route proxy · 19527** | **Web service · 3090** |
| --- | --- | --- |
| Default address | `127.0.0.1:19527` | `127.0.0.1:3090` |
| Who connects | AI CLIs on your machine | Your browser or phone |
| What flows | Model inference requests, rewritten and forwarded upstream | The AI Switch UI's own API calls and event stream |
| Auth | Route proxy key (AI Switch writes it into each CLI config) | Web access token (HTTP bearer) |
| If it is off | CLIs cannot route through AI Switch, but the UI works fine | You can only use the desktop app; browsers cannot connect |

One-line version: **19527 is for the AI, 3090 is for you.**

They are independent. Managing accounts on the desktop works with 3090 off entirely; checking usage stats from your phone works with 19527 off. Route proxy setup is in [accounts and the pool](/en/guide/accounts); web service setup is in [web service mode](/en/deploy/web-service).

## Where are my API keys stored, and is that safe?

Keys live in the SQLite database under your local data directory `~/.ai-switch` — specifically in the `secret_payload_json` column of the `route_credentials` table.

To be clear about what that means: **the current version does not encrypt that column at rest**, and it does not use the operating system's keychain. Security is therefore equivalent to the security of your local filesystem. Treat the entire `~/.ai-switch` directory as a credential directory:

- Mind the permissions on the directory and the database file; other users on the same machine should not be able to read them.
- Keep it out of public repositories, unencrypted sync folders, and shared directories.
- Prefer a volume with full-disk encryption.
- When you back the directory up, handle it as secret material (encrypted archive, offline storage).

The web service adds a separate layer: every `/api/*` and `/ws/events` request requires the access token, and eleven sensitive commands (credential export/import, reading the proxy key, MCP and skill write operations) return 404 rather than 403 when the transport does not meet the security bar — the response will not even confirm the command exists. On top of that, binding to a non-loopback address without TLS configured makes the web service refuse to start rather than run in a degraded mode.

The data directory layout is described in [desktop deployment](/en/deploy/desktop).

## What happens when an account runs out of quota? How does failover work?

Accounts are scheduled by a **priority from 1 to 5, defaulting to 3**, with lower numbers used first. Each account also has a **concurrency limit, 5 by default for new accounts** — once an account has that many requests in flight, the next one goes looking for the next account in the pool.

When an account fails or exhausts its quota, AI Switch:

1. Records the failure kind, the failure message, and the raw error body the upstream returned
2. Distinguishes transient failures (counted, with a backoff-scheduled retry) from quota exhaustion (cooled down until the quota window resets)
3. Detects streaks of the same semantic failure, so requests stop being wasted on an account that is genuinely broken
4. Hands the request to the next available account in the pool

Recovery is driven by a background scheduler that can re-enable accounts on a schedule or after a health-check probe. See [reliability and auto recovery](/en/guide/reliability).

::: tip When should I lower the concurrency limit?
Official accounts and some third-party endpoints are sensitive to concurrency: parallel requests on a single account tend to trigger rate limiting, and sometimes get flagged as abnormal usage. For those upstreams, set the account's limit to 1 or 2 so AI Switch **spreads concurrency across accounts** — which is the whole point of a pool — rather than piling it onto one.
:::

## Can I use it from my phone?

Yes. Enable the web service, open it in your phone's browser, and enter the access token. Desktop and browser run the same UI — there is no stripped-down mobile version.

Things to know:

- The default bind is `127.0.0.1:3090`, which only the host machine can reach. A phone requires either changing the bind address or going through Tailscale.
- **Binding to a non-loopback address (such as `0.0.0.0`) requires TLS to be configured at the same time.** Without it the web service refuses to start and reports `web.sensitive_transport_requires_tls`. This is a hard block rather than a warning — plaintext HTTP on a LAN would expose your access token and credentials in the clear.
- The better option is Tailscale: install the client on your phone, join the same tailnet, and you get access without exposing a public port or sourcing certificates yourself.

Setup steps are in [web service mode](/en/deploy/web-service) and [remote access and HTTPS](/en/deploy/remote-access).

## If I go through Tailscale, do I still need a token?

**Yes.** This is deliberate, not an oversight.

Tailscale solves network reachability; it does not replace application-level auth. Whether a request comes from the local machine, from another device inside the tailnet, or from the public internet via Tailscale Funnel, the token check on `/api/*` and `/ws/events` is never skipped.

The reasoning is simple: any device on your tailnet — including one shared with you, or one that has been compromised — can reach your node. Dropping the token would mean exposing your account management interface to the entire tailnet.

Tailscale login is also **manual**; the app never logs in automatically at startup.

## Where is my data, and how do I back it up?

Everything lives under `~/.ai-switch/` in your home directory:

```text
~/.ai-switch/
├── ai-switch.db              # main database (dev builds use ai-switch-dev.db)
├── settings.json             # app settings
├── web-service.json          # web service config
├── route-proxy-https.json    # route proxy HTTPS config
├── backups/
│   └── config-snapshots/     # a snapshot before every CLI config write
├── certs/route-proxy/        # route proxy self-signed certificates
├── imports/
├── logs/
└── tailscale/
```

**Backing up the whole directory is enough** — accounts, settings, and keys are all in there (the keys inside the database). Which also means the backup is itself a credential: store it encrypted, and keep it out of public repos and unencrypted sync folders.

The location is **not configurable**. `AI_SWITCH_DATA_DIR`, which appears in the README, is not implemented in the current code and setting it has no effect — the app always uses `.ai-switch` under the running user's home directory. To relocate the data, control the service account's home directory or mount a container volume there.

If the actual goal is moving accounts to another device, the built-in **credential export/import** is a better tool than copying directories, since it records provenance information.

## My account list is empty after upgrading to 0.7.3 — is the data gone?

**No.** Nothing was deleted. The database was moved into `~/.ai-switch/backups/` under a name like `ai-switch.db.migration-conflict-<timestamp>`.

The cause: 0.7.3 changed the line endings of two database migration scripts from CRLF to LF. Not a single SQL statement changed, but migration checksums are computed over the file's raw bytes, so every existing install decided at startup that a migration had been modified — which triggered the fallback of the day: move the whole database into `backups/` and create a fresh, empty one.

From 0.7.4 on:

- a checksum mismatch caused only by line endings is **repaired in place**, with no quarantine;
- if the live database is empty and a quarantined one exists in `backups/`, it is **restored automatically** at startup (validated on a copy first, and never over accounts you created after the quarantine);
- when a migration's contents genuinely changed and the database holds data, the app **refuses to start** instead of replacing it.

So opening the app after upgrading to 0.7.4 should bring your accounts back. If it does not, quit the app and copy the newest `migration-conflict` file from `backups/` back to `~/.ai-switch/ai-switch.db`, then start it again.

## Which operating systems are supported?

Windows, macOS, and Linux. Every release is built by CI on all three:

| OS | Installer format |
| --- | --- |
| Windows | NSIS installer (`.exe`) |
| macOS | `.dmg` and `.app` |
| Linux | `.deb` and `.AppImage` |

The desktop app supports auto-updates, with minisign-signed update packages that the client verifies before installing. The standalone server and Tailscale sidecar also ship as per-platform binary archives.

Download and install instructions are in [installation](/en/guide/installation).

## Is AI Switch open source? What is the license?

Yes. It is **MIT** licensed, `Copyright (c) 2026 xyito`.

Repository: <https://github.com/ijry/ai-switch>

MIT means you can use, modify, and redistribute it freely, including commercially, as long as you keep the copyright and license notice. Third-party dependency licenses are listed in the repository's `LICENSES/` directory and `THIRD_PARTY_NOTICES.md`.

## What is the relationship with cc-switch?

**AI Switch is compatible with cc-switch's import protocol**, for exactly one reason: so cc-switch users can migrate their configuration over easily. The add-account dialog's "导入其他客户端" tab also reads config files under `~/.cc-switch` on this machine directly (opened read-only, never modifying their data) and imports the API accounts you tick. The desktop app can additionally enable `ccswitch://` deep-link compatibility (off by default), so import links originally aimed at cc-switch are accepted by AI Switch too.

Beyond that there is no relationship. AI Switch is an independent, from-scratch implementation: **this project only studies public behavior, public documentation, and public file formats; it does not reuse their code.** The "Clean-Room Boundary" section of the repository README states this boundary explicitly.

Put another way: compatibility means "can read the same config and import formats", not "shares an implementation".

## Does the desktop app update itself?

Yes. The updater points at the `latest.json` manifest on the repository's releases, and the client verifies each package's minisign signature against a built-in public key. Verification failure means no install.

The release pipeline includes a dedicated check that the signing key and the configured public key are the same pair, so a key rotation cannot silently break the update path. Details are in [release process](/en/dev/release).

## Which clients do MCP and skills management cover?

MCP management writes to the configuration of **eleven clients**: Claude Code, Codex, Gemini, Grok, OpenCode, OpenClaw, Hermes, Cursor, Cline, CodeBuddy, and Kimi Code. You install an MCP server once and tick which clients should receive it.

For skills, two packages ship built in with **27 skills** total: `ai-switch.core` (14, engineering workflow) and `ai-switch.science` (13, research methodology).

See [MCP servers](/en/features/mcp) and [skills management](/en/features/skills).
