---
title: MCP Servers
description: Write one normalised MCP server definition and AI Switch writes it into the native config file of 11 clients, absorbing the differences in transport support, key names, and file formats — with the official registry and Smithery built in.
---

# MCP Servers

MCP (Model Context Protocol) servers give AI CLIs extra capabilities: reading files, querying databases, calling APIs. The problem is that **one MCP server has to be configured eleven times, differently each time.** The config files live in different places, come in JSON, TOML, and YAML, name the server map differently, and express transport types differently.

AI Switch's MCP management exists to solve exactly that. You write one **canonical definition**, pick which clients should have it, and it translates that into the shape each client understands and writes it into the right file.

## The 11 supported clients

The paths below are each client's **publicly documented config location** — AI Switch simply reads and writes those public file formats.

| Client | Config file | Format | Server map key |
| --- | --- | --- | --- |
| Codex CLI | `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`) | TOML | `mcp_servers` |
| Claude Code | `~/.claude.json` | JSON | `mcpServers` |
| Gemini CLI | `~/.gemini/settings.json` | JSON | `mcpServers` |
| Grok | `$GROK_HOME/config.toml` (default `~/.grok/config.toml`) | TOML | `mcp_servers` |
| OpenCode | `~/.config/opencode/opencode.json` | JSON | `mcpServers` (legacy `mcp` also read) |
| OpenClaw | `~/.openclaw/openclaw.json` | JSON | `mcp.servers` (nested) |
| Hermes Agent | `$HERMES_HOME/config.yaml` (default `~/.hermes/config.yaml`) | YAML | `mcp_servers` |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json` | JSON | `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | JSON | `mcpServers` |
| Kimi Code | `$KIMI_CODE_HOME/mcp.json` (default `~/.kimi-code/mcp.json`) | JSON | `mcpServers` |
| CodeBuddy | `~/.codebuddy.json` | JSON | `mcpServers` |

Four clients let an environment variable override their home directory (`CODEX_HOME`, `GROK_HOME`, `HERMES_HOME`, `KIMI_CODE_HOME`); AI Switch checks the variable before falling back to the default path.

Claude Code and CodeBuddy each have a secondary file (`~/.claude/settings.json`, `~/.codebuddy/settings.json`) holding plugin-enablement state, which AI Switch maintains alongside the main file when needed.

## One canonical definition

What you type in the UI is a JSON object in AI Switch's own **canonical shape** — not any single client's native format:

```json
{
  "type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
}
```

`type` has three values:

| `type` | Required fields | Meaning |
| --- | --- | --- |
| `stdio` | `command`; optionally `args`, `env`, `cwd` | A local process communicating over stdin/stdout |
| `sse` | `url`; optionally `headers` | A remote Server-Sent Events server |
| `http` | `url`; optionally `headers` | A remote streamable-HTTP server |

The full field set:

| Field | Transports | Meaning |
| --- | --- | --- |
| `command` | stdio | The executable |
| `args` | stdio | Argument array |
| `env` | stdio | Environment variable object |
| `cwd` | stdio | Working directory |
| `url` | sse / http | Server address |
| `headers` | sse / http | Request header object, commonly used for tokens |

Malformed JSON is caught before saving (`mcp.invalidJson`); invalid fields come back from the backend as `mcp.invalid_spec`.

Server ids are validated too: they cannot be empty and cannot contain path separators or other characters that would break a config file's structure, otherwise you get `mcp.invalid_server_id`.

## What normalisation actually buys you

This is where the feature earns its keep. Writing the same definition into different clients involves these adjustments, all following each client's **public config format**:

| Difference | How it is handled |
| --- | --- |
| **File format** | Three serialisers (JSON / TOML / YAML), each preserving unrelated sections of the existing file |
| **Server map key** | `mcpServers`, `mcp_servers`, or the nested `mcp.servers` |
| **Transport field name** | Some use `type`, some use `transport`, some write no transport field at all |
| **Naming of HTTP** | Some clients spell streamable HTTP as `streamableHttp` rather than `http` |
| **Header key** | Codex's TOML calls it `http_headers`, mapped both ways to canonical `headers` |
| **Command shape** | OpenCode's legacy `mcp` section writes the command as an array `[cmd, ...args]` and names the env map `environment` |
| **Extra keys** | Cursor keeps only the keys it recognises; extra keys on Grok entries such as `enabled` and `required` are **preserved verbatim** when rewriting |

One capability gap has to be handled explicitly: **Codex CLI does not support the SSE transport.** Writing a `type: "sse"` server to it is rejected with `mcp.unsupported_transport`. To use the same remote server from Codex, switch to the `http` transport if the server offers it, or front it with a stdio proxy.

If none of the selected clients can accept the current definition, you get `mcp.no_compatible_client`.

## Writes are atomic

Rewriting someone's config file is risky — they may be using that CLI right now, and a half-written file is a broken file. So the write path is:

1. Read the existing file. **A missing file is treated as an empty object**, which is neither an error nor a reason to lose configuration that was never there.
2. Modify only the MCP section of the in-memory structure, leaving every other section untouched.
3. Serialise and write to a temporary file in the same directory (named like `.config.toml.ai-switch-<uuid>`).
4. Rename over the target.

A same-directory rename is atomic on mainstream filesystems, so any reader sees either the complete old file or the complete new one.

A read failure (permissions, disk) returns `mcp.config_io`. A file that exists but will not parse — hand-edited into invalid syntax — returns `mcp.config_invalid`. In that case AI Switch **will not** guess at a repair; it would rather report the error than overwrite content you might still want to salvage.

::: warning Hand-edited config files
If a client's config file has a syntax error (a missing quote in TOML, say), AI Switch cannot write to that client and reports `mcp.config_invalid`. Fix the syntax in an editor, then retry.
:::

## Managing local servers

The MCP screen has two views, Local and Marketplace. Local manages the servers you have configured.

Each server card shows the server id, a transport-type badge, the `command` or `url`, and a row of client chips — **the chips are which clients this server is currently active in**.

Creating or editing takes three inputs:

1. **Server id**, which becomes the key in each client's config file.
2. **The canonical JSON** from the previous section.
3. **Target clients**, at least one. Codex CLI and Claude Code are pre-selected.

Save is disabled with an empty id or no clients selected. Delete asks for confirmation, because it removes the entry from every affected client's config file at once.

## The marketplace

The marketplace view is wired to two sources:

| Source | Description |
| --- | --- |
| Official MCP Registry | The Model Context Protocol project's own server registry |
| Smithery | A third-party MCP server directory |

A search returns up to 30 results. Opening one brings up a detail modal with the description, a homepage link, a target-client selector, and two sections driven by the marketplace metadata:

- **Transport selection**: a server may offer both a stdio and a remote install option; when there is more than one you choose.
- **A parameter form**: every parameter the marketplace declares renders as the matching control — enums become dropdowns, booleans become checkboxes, JSON becomes a textarea, secret types become password fields, numeric types become number inputs. If any **required parameter is empty**, install stays disabled.

Installing simply fills in the canonical definition for you and then goes through exactly the same write path as a hand-created server. Afterwards there is no difference between the two.

Marketplace errors: `mcp.marketplace_network` for network failures, `mcp.marketplace_invalid` for malformed responses, `mcp.marketplace_not_found` when the requested server does not exist.

::: tip Marketplace listings are not vetted by AI Switch
The marketplace just surfaces what the upstream directories publish. Installing an MCP server means letting your AI CLI execute the tools it provides — which is running third-party code. Look at the homepage and source before installing, especially for anything that wants a secret.
:::

## When changes take effect

MCP configuration is read **at client startup**. Once AI Switch has written the file, an already-running CLI process will not notice.

So the right order is: configure in AI Switch first, then start (or restart) the CLI. Opening a fresh tab in the [Vibe terminal](/en/features/vibe) after editing satisfies that naturally.

## Next steps

- [Skills Management](/en/features/skills) — the other cross-client capability layer, with a similar design
- [Vibe Terminal and Skins](/en/features/vibe) — spin up a terminal to verify a change immediately
- [Platform Support Matrix](/en/guide/platform-support) — config-write and other capabilities per platform
- [Architecture](/en/dev/architecture) — where the client adapter layer sits in the code
