---
title: Platform Support Matrix
description: The complete matrix of 7 platforms against 10 capabilities in AI Switch, explaining native support versus generic API routing, what partial support actually means, and how safe direct writes protect native config files.
---

# Platform Support Matrix

AI Switch knows about 7 target platforms, and each platform's support for 10 capabilities is **declared explicitly in code** rather than guessed at runtime. This page lists the full matrix and explains what every capability and every state means in practice.

## Two support levels

The 7 platforms split into two groups.

**Native support** (`supported`) — Codex, Claude Code, Gemini CLI, Grok

AI Switch understands these tools' config file formats and official sign-in formats. On top of generic API routing, it can write native config, import official accounts, route through official accounts, and handle deeplink imports.

**Generic API routing** (`partial`) — OpenCode, OpenClaw, Hermes

> OpenCode, OpenClaw, and Hermes remain visible for generic API routing, terminal launch, and session workflows, but AI Switch does not claim native configuration, official-account import, or quota support for them.

You can still give them API accounts and route through the local proxy, and you can still launch terminals and manage sessions from AI Switch. But you supply the configuration yourself; AI Switch will not touch their config files.

## The full matrix

Three states:

- **✅ Supported** — fully available
- **◐ Partial** — available, with extra preconditions
- **✕ Unavailable** — the call is rejected

| Capability | Codex | Claude Code | Gemini CLI | Grok | OpenCode | OpenClaw | Hermes |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| `route_credentials` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `generic_api_routing` | ✅ | ✅ | ✅ | ✅ | ◐ | ◐ | ◐ |
| `config_write` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_import` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_account_routing` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `deeplink_import` | ✅ | ✅ | ✅ | ✅ | ✕ | ✕ | ✕ |
| `official_quota` | ✅ | ✅ | **✕** | ✅ | ✕ | ✕ | ✕ |
| `model_test` | ✅ | ✅ | ✅ | ✅ | ◐ | ◐ | ◐ |
| `terminal_launch` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `session_resume` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

::: warning One exception: Gemini CLI quota
The one cell that breaks the "native support means all green" pattern is **`official_quota` for Gemini CLI**.

Gemini CLI is a natively supported platform — config writing, official import, and official account routing all work — but **official quota lookup is unavailable** (`capability.quota_unavailable`). Gemini accounts therefore show no official quota information and cannot have their quota refreshed.

The other three native platforms (Codex, Claude Code, Grok) support all 10 capabilities.
:::

## What each capability means

### `route_credentials`

Manage route accounts for the platform: create, edit, delete, and move in and out of the pool.

**Supported on all 7 platforms.** This is the baseline — any platform can have route accounts.

### `generic_api_routing`

Forward the platform's API requests through the local routing proxy.

Fully supported on the four native platforms. For OpenCode, OpenClaw, and Hermes it's **partial**, with reason code `capability.api_credentials_only`:

> Only API accounts with a configured base URL and interface format are supported.

Three concrete constraints:

1. Only `api`-kind accounts are accepted; accounts imported from official sign-in state do not serve traffic
2. A base URL **must** be provided explicitly
3. An interface format (upstream dialect) **must** be provided explicitly

The four native platforms have default dialects — `openai` for Codex and Grok, `anthropic` for Claude, `gemini` for Gemini — while these three have **no default**, so leaving the field empty means the account can't be used.

Note that partial does not mean unusable. Routing works fine; you just don't get those two fields filled in for you.

### `config_write`

Point the CLI's native config file at the local routing proxy.

Supported on the four native platforms, with these targets:

| Platform | Target file | Format |
| --- | --- | --- |
| Codex | `~/.codex/config.toml` (plus `~/.codex/ai-switch-model-catalog.json`) | TOML |
| Claude Code | `~/.claude/settings.json` | JSON |
| Gemini CLI | `~/.gemini/settings.json` | JSON |
| Grok | `~/.grok/settings.json` | JSON |

Unavailable on the other three, with reason code `capability.native_config_unavailable`:

> Native configuration writing is not implemented for this platform.

Configure those by hand: click the 🔌 button in the toolbar and copy the Base URL and API Key from the 「在以上客户端之外使用」 section at the bottom of the dialog. AI Switch neither parses nor modifies their config files.

### `official_import`

Import a platform's official sign-in state or account credentials. AI Switch accepts several input shapes: OAuth CPA, API Key CPA, session JSON, `auth.json`, Sub2API JSON, accessToken, and refresh_token.

Supported on the four native platforms. Unavailable on the other three, with reason code `capability.official_account_unavailable`:

> This platform does not support official account import or official account routing.

### `official_account_routing`

Route requests through an imported official account rather than an API key account.

Supported on the four native platforms, unavailable on the other three with the same `capability.official_account_unavailable`. This is also why their `generic_api_routing` is restricted to `api`-kind accounts.

### `deeplink_import`

Import accounts via deeplink. The desktop app registers the `aiswitch` URL scheme.

Supported on the four native platforms. Unavailable on the other three, with reason code `capability.deeplink_unavailable`:

> This platform does not support deeplink import.

### `official_quota`

Query and refresh quota information for official accounts.

**Supported on Codex, Claude Code, and Grok. Unavailable on Gemini CLI**, as well as on OpenCode, OpenClaw, and Hermes. The reason code in every unavailable case is `capability.quota_unavailable`:

> This platform does not support official account quota refresh.

Pool routing does consider remaining quota when filtering — an account with zero remaining is skipped — but that information depends on the quota capability. Since Gemini accounts can't report official quota, unavailability there is discovered only through request failures.

### `model_test`

Run a real generation test against an account. This is not a reachability probe: AI Switch genuinely has the upstream generate content, then shows the model output and the full request chain.

Fully supported on the four native platforms. **Partial** on the other three, again `capability.api_credentials_only` — only `api` accounts with a base URL and interface format can be tested.

See [Model Connectivity Tests](/en/guide/model-test).

### `terminal_launch`

Launch a system terminal from AI Switch running the platform's CLI.

**Supported on all 7 platforms.**

### `session_resume`

Resume a previous session for the platform, either in a system terminal or by copying the resume command to run yourself.

**Supported on all 7 platforms.**

See [Session Management](/en/features/sessions).

## How partial differs from unavailable in behavior

The distinction matters because it determines whether an operation is refused.

**Partial (`partial`) operations are callable.** AI Switch attaches extra constraints — must be an `api` account, must have a base URL, must have an interface format — and executes normally once those hold. The UI surfaces the explanatory text tied to the reason code so you know why the constraint exists.

**Unavailable (`unavailable`) operations are refused.** The call returns a `capability.unavailable` validation error, with a message shaped like `Hermes does not support config_write` and the specific reason code attached. The corresponding UI controls are disabled, with the reason on hover.

This check is enforced server-side, not merely as greyed-out UI — invoking the command directly hits the same rule.

## Safety of native config writing

Config writing on the four native platforms is not a plain file overwrite:

> Native configuration writes use safe direct writes: a snapshot is prepared before mutation, the write is atomic, concurrent modifications are detected, and guarded rollback is supported.

Four guarantees.

**A snapshot before mutation.** Each write first copies the original file into `~/.ai-switch/backups/config-snapshots/` (mode `0700` on Unix). The write-result panel shows the snapshot id.

**Atomic writes.** A half-written config file cannot happen.

**Concurrent-modification detection.** If the file changed after AI Switch read it and before it wrote — you edited it, or another tool did — that's detected rather than silently clobbered. The result includes before and after hashes.

**Guarded rollback.** You can roll back to a snapshot, and the rollback itself is guarded so it won't blindly overwrite current state.

Writes are also **incremental**: AI Switch only adds or updates the fields it manages, leaving your other settings in place. Existing `env` entries and other settings in Claude Code's `settings.json`, for example, are not cleared.

## Platform ids and aliases

The platform ids used by commands and the API are `codex`, `claude`, `gemini`, `grok`, `opencode`, `openclaw`, and `hermes`.

Parsing accepts some aliases:

| Platform id | Accepted aliases |
| --- | --- |
| `codex` | `openai`, `chatgpt` |
| `claude` | `anthropic`, `claude_code`, `claude_desktop`, `claude-code` |
| `gemini` | `google`, `gemini_cli`, `gemini-cli` |
| `grok` | `xai`, `x_ai`, `x.ai` |
| `opencode` | `open_code`, `open-code` |
| `openclaw` | `open_claw`, `open-claw` |
| `hermes` | — |

Parsing is case-insensitive, and spaces and hyphens normalize to underscores. But **only explicit aliases are accepted** — a string like `my-claude-wrapper` is rejected with `platform.unknown` rather than fuzzy-matched to Claude.

## Related pages

- [Accounts and the Pool](/en/guide/accounts) — account kinds, priority, concurrency, and pool scheduling
- [Protocol Routing and Bridging](/en/guide/protocol-routing) — the four upstream dialects and seven bridges
- [Model Connectivity Tests](/en/guide/model-test) — details of real generation testing
- [Usage and Request Stats](/en/guide/usage-stats) — per-account usage and billing
- [Reliability and Auto Recovery](/en/guide/reliability) — failure handling and recovery
- [Session Management](/en/features/sessions) — terminal launch and session resume
