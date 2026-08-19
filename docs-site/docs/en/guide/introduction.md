---
title: What is AI Switch
description: AI Switch is an open-source tool for switching AI providers and accounts. One local proxy manages accounts across Codex, Claude Code, Gemini CLI, Grok and three more platforms, with pool scheduling and four upstream protocol bridges.
---

# What is AI Switch

AI Switch is an open-source tool for switching AI providers and accounts. It ships as a desktop app and as a self-hosted web service. It runs a local routing proxy on your machine, accepts requests from your AI CLIs, and forwards each one to an upstream account according to the priority and concurrency rules you set.

The practical upshot: **your CLIs point at one fixed local address, and every difference between accounts, providers, and protocols gets resolved inside AI Switch.**

## The problem it solves

If you use more than one AI CLI, some of this will look familiar.

**Every CLI manages its own accounts.** Codex reads `~/.codex/config.toml`, Claude Code reads `~/.claude/settings.json`, Gemini CLI reads `~/.gemini/settings.json`, Grok reads `~/.grok/settings.json`. One relay key has to be pasted into several places, in several different formats, and changing it means editing several files.

**Switching providers means editing config.** Moving from one relay to another means finding the right file, changing the base URL, changing the key, and hoping you didn't break the syntax. Trying a second provider just to compare is expensive enough that you never bother.

**Exhausted quota means switching by hand.** When an account gets rate-limited or runs out of credit, all your CLI tells you is a 429 or a 5xx. You have to work out what happened, go back to the config file, swap in a different account, and restart the CLI.

**Protocol mismatches waste accounts.** You have a relay account that only speaks OpenAI Chat Completions, but you want to use Codex. Codex speaks Responses. The two don't line up, so the account sits idle.

AI Switch collapses all of this into one place: accounts live in one inventory, switching happens automatically inside the proxy, and protocol mismatches get translated in flight. You write the CLI config once and then leave it alone.

## Core concepts

Five concepts cover essentially all of how AI Switch works.

### Route accounts

A **route account** is one set of upstream credentials you can send requests with. The common case is an API account: base URL, API key, and upstream protocol (the interface format).

Each account carries its own routing parameters:

- **Route priority**, 1-5, default 3 — lower numbers win
- **Max concurrency**, default 1, minimum 1
- **Failure policy**: extra retry count, retry interval, error threshold
- **Model mappings**: map the model name the client asks for onto a model the upstream actually serves
- **Auto recovery**: off, daily schedule, or health-check probing

Besides API accounts there are accounts imported from a platform's official sign-in state. Both kinds can join the pool and serve traffic.

See [Accounts and the Pool](/en/guide/accounts).

### The pool

The **pool** is the set of accounts serving traffic for the current platform. When a request arrives, the proxy picks an account from the pool.

Selection is **strict priority tiers with rotation inside each tier**:

1. Accounts are grouped by `route_priority` ascending — every priority-1 account is tried before any priority-2 account
2. Within a tier, a cursor rotates round-robin. The cursor advances by one after each completed request, so load spreads evenly across the tier
3. Every attempt must first acquire a concurrency lease. An account already at its `max_concurrency` is skipped and the next one is tried
4. Accounts that aren't healthy, are archived, have exhausted quota, or are cooling down get filtered out
5. If no account in the pool serves the requested model, the proxy returns `route_pool.model_unmatched` rather than guessing

Failures don't need you. Retryable errors — connection failures, timeouts, 408, 429, 5xx — are retried according to the account's own policy, and once the retries are used up the proxy moves to the next account. 401 and 403 are never retried on the same account; they go straight to switching.

That's what makes "switching by hand when quota runs out" go away. You just put the backup accounts in the pool and set their priorities.

### The local proxy

The **local routing proxy** is what your CLIs actually connect to. It binds to `127.0.0.1` on default port **19527**.

Its local entry protocol depends on the platform:

- **Codex** uses `/responses` (OpenAI Responses)
- **Claude** uses `/v1/messages` (Anthropic Messages)
- **Gemini CLI** stays on Gemini native

The proxy issues one local key per platform, shaped like `sk-ai-switch-<uuid>`. Your CLI authenticates with it (via `Authorization: Bearer`, `x-api-key`, or `x-goog-api-key`), and the proxy uses it to tell which platform the request belongs to. This key is for local authentication only and is **never forwarded upstream**.

The proxy can also serve HTTPS locally, configured in settings, which generates and imports a root certificate.

### Protocol bridging

**Protocol bridging** is how AI Switch makes a "wrong protocol" account usable anyway.

There are four upstream protocols (dialects):

| Dialect | Upstream API |
| --- | --- |
| `openai` | Chat Completions |
| `openai-responses` | Responses API |
| `anthropic` | Messages API |
| `gemini` | generateContent |

When the local entry protocol and the upstream account's protocol don't match, the proxy translates both the request and the response. There are seven bridges:

`ResponsesToChat`, `ResponsesToResponses`, `ResponsesToAnthropic`, `ResponsesToGemini`, `ClaudeToChat`, `ClaudeToResponses`, `ClaudeToGemini`.

A concrete example: all you have is a Chat Completions relay account, but you want to use Codex. Codex sends `/responses`, the proxy takes the `ResponsesToChat` bridge, rewrites the request as Chat Completions, sends it upstream, and converts the response back into Responses format. Codex never knows anything happened.

Streaming responses are translated too. See [Protocol Routing and Bridging](/en/guide/protocol-routing).

### Platforms

A **platform** is a target CLI that AI Switch knows about. There are seven:

- **Native support**: Codex, Claude Code, Gemini CLI, Grok
- **Generic API routing**: OpenCode, OpenClaw, Hermes

The difference is scope. AI Switch can write native config files and import official accounts for the first four. The last three get generic API routing only, and you have to supply the base URL and interface format yourself.

The full 7-platform by 10-capability table is in [Platform Support Matrix](/en/guide/platform-support).

## Who it's for

**People running several AI CLIs.** Accounts live in one inventory, AI Switch writes the config for four of the CLIs, and you never have to remember which file uses which format.

**People holding several relay accounts.** Put them all in the pool, set the priorities, and let the proxy handle rate limits and exhausted credit. Main account at priority 1, backup at 3 — when the main one dies, traffic drops down on its own.

**People whose account protocol doesn't match the CLI they want.** Bridging lets a Chat Completions account feed Codex, and a Gemini account feed Claude Code.

**People who want to know what they're spending.** Every request records input, output, and cache tokens plus price, aggregated per account and per time window. See [Usage and Request Stats](/en/guide/usage-stats).

**People who need browser or phone access.** Web service mode runs the same UI in a browser.

**People who work in the terminal.** There's a built-in terminal workspace, session management, MCP server management, and skills management. See [Vibe Terminal and Skins](/en/features/vibe) and [Session Management](/en/features/sessions).

## How desktop and web service relate

These are two ways to run the same program, **not two products**.

Desktop and browser share one React UI and one Rust core. Only the transport differs:

- **Desktop** calls the core over Tauri IPC
- **Browser mode** calls it over HTTP: `POST /api/:command` and `GET /ws/events`, both requiring an access token

The web service binds to `127.0.0.1:3090` by default. One hard safety rule applies here: **listening on a non-loopback address without TLS enabled will refuse to start**, failing with `web.sensitive_transport_requires_tls`. To reach it from your LAN or remotely, either configure TLS or use a private network such as Tailscale.

There are two ways to run it:

1. **Embedded in the desktop app** — turn on Web Service in settings. Good for "desktop is always running, I occasionally check from my phone."
2. **Standalone server** — the separate `ai-switch-server` binary, no desktop environment required. Good for a NAS or a server.

Both use the same data directory layout. See [Web Service Mode](/en/deploy/web-service) and [Standalone Server](/en/deploy/standalone-server).

## What it's built on

- **Frontend**: Tauri 2 + React 18 + TypeScript
- **Core**: the Rust crate `ai_switch_lib`, using axum, sqlx, rustls, rcgen, reqwest, portable-pty
- **Data**: SQLite, 23 migrations, in `src-tauri/migrations`
- **Sidecar**: `ai-switch-tsnet`, written in Go on Tailscale tsnet + Funnel, for private remote access
- **Standalone binary**: `ai-switch-server`, for browser and mobile access

Architecture details are in [Architecture](/en/dev/architecture).

## One note

AI Switch can import account formats from other tools (a compatible import protocol), but this is built by studying **public behavior, public documentation, and public file formats**. The project does not reuse third-party code.

## Next steps

- [Installation](/en/guide/installation) — download a build and learn where your data lives
- [Quick Start](/en/guide/quick-start) — from first account to first working request
- [Platform Support Matrix](/en/guide/platform-support) — check how far support goes for your CLI
