---
title: Session Management
description: AI Switch discovers your CLI transcripts by scanning the files each tool writes to disk, unifies them into one searchable list across 7 platforms, and resumes any of them with that platform's own resume command.
---

# Session Management

Every AI CLI writes its conversation history to disk — but each one uses a different directory, a different file naming scheme, and a different JSON shape. AI Switch's session management does one straightforward thing: **find those scattered transcript files, present them as one searchable list, and hand any of them back to the CLI that wrote it so you can keep going.**

One premise up front: **AI Switch does not create these transcripts and does not own them.** It only reads them. Uninstalling AI Switch leaves your history untouched; conversely, AI Switch cannot recover a transcript the CLI itself has pruned.

## Where sessions come from

The backend defines a scan root set and file extensions for each of the seven platforms, all relative to your home directory:

| Platform | Scan roots | Extensions |
| --- | --- | --- |
| Codex | `.codex/sessions`, `.codex` | `.jsonl` |
| Claude Code | `.claude/projects`, `.cache/claude/projects` | `.jsonl` |
| Grok | `.grok/sessions`, `.xai/sessions`, `.cache/grok/sessions` | `.json`, `.jsonl` |
| Gemini CLI | `.gemini/tmp`, `.cache/gemini/tmp` | `.json`, `.jsonl` |
| OpenCode | `.local/share/opencode`, `AppData/Local/opencode` | `.json`, `.jsonl` |
| OpenClaw | `.openclaw/agents` | `.jsonl` |
| Hermes Agent | `.hermes/sessions` | `.json`, `.jsonl` |

Scanning rules:

- Each root is walked **at most 6 levels deep**, and each platform contributes **at most 1000 files**, so an enormous history directory cannot stall the list.
- Paths reached through more than one root are deduplicated (Codex's two roots nest inside each other, for instance).
- A missing directory is skipped silently, not treated as an error.

Because discovery is directory-driven, the session list naturally reflects which CLIs have actually been used on this machine. A CLI you never installed simply never appears.

## How each field is inferred

Transcript formats differ, so parsing uses a set of fallbacks. Every field tries several common key names:

| Field | Source | Fallback |
| --- | --- | --- |
| Session id | `session_id` / `sessionId` / `id` / `payload.id` within the first 20 lines | The file name without its extension |
| Project directory | `cwd` / `project_dir` / `projectDir` / `payload.cwd` / `payload.project_dir` | The containing directory's name |
| Timestamps | `timestamp` / `created_at` / `createdAt` / `ts`, accepting integer epochs and RFC 3339 | File modification time |
| Title | The first thing a human actually said | The directory name |

**Title extraction is the fiddly part.** It skips assistant, developer, system, and tool roles, and it also skips content that looks like injected context rather than a user's prompt — blocks starting with things like `<permissions instructions>`, `<skills_instructions>`, `<environment_context>`, `<instructions>`, or `# agents.md instructions`. The first genuine user message wins and is truncated to 72 characters with an ellipsis.

To avoid pulling large files into memory, the list phase reads only the **first 80 lines** of each file to infer title and timestamps. Opening a session to read its messages reads further, capped at **2000 lines**.

### Sub-agent transcripts are filtered out

Parallel sub-agents write transcripts of their own. Those are noise in a list of your conversations, so they are detected and dropped:

| Platform | Detection |
| --- | --- |
| Codex | `payload.thread_source == "subagent"`, or a `payload.source.subagent.thread_spawn` object |
| Claude Code | `isSidechain == true` |
| The other five | No filtering — their formats carry no equivalent marker |

### Ordering and the cap

Results from all platforms are merged, sorted by **last active time** descending (falling back to creation time), then **truncated to 500 entries**. So the list is always "the 500 most recent", not your complete history.

Roles are normalised for consistent display: `human` and `user_message` become `user`; `assistant_message` and `ai` become `assistant`; `tool`, `tool_result`, and `function_call` become `tool`.

## Two entry points

### The session manager screen

Open it from the main UI via **Settings → Feature entries → Sessions** — it is not a top-level nav item. The screen gives you:

- **Search** across title, project directory, and platform.
- **Grouped / Flat** layouts: grouped clusters by project directory, flat is one chronological list.
- **A message pane** showing the parsed messages for the selected session, with role labels (user / assistant / system / tool / developer) and quick navigation.
- **Copy buttons**: copy directory, copy source file, copy resume command.
- **Open in system terminal**, which hands off to your OS terminal app.

### The Vibe sidebar

The left rail of the [Vibe terminal](/en/features/vibe) is the same list. The difference is that resuming there opens **a terminal tab inside Vibe** without leaving the screen.

## Resuming a session

Each session gets a synthesised resume command, matching each platform's own CLI convention:

| Platform | Resume command |
| --- | --- |
| Codex | `codex resume <session_id>` |
| Claude Code | `claude --resume <session_id>` |
| Grok | `grok resume <session_id>` |
| Gemini CLI | `gemini --resume <session_id>` |
| OpenCode | `opencode session <session_id>` |
| OpenClaw | `openclaw resume <session_id>` |
| Hermes Agent | `hermes resume <session_id>` |

The command runs in the session's **project directory** — not wherever you started AI Switch. All seven platforms are marked as supporting `session_resume` in the capability matrix; see [Platform Support Matrix](/en/guide/platform-support).

Two hard prerequisites: both the **project directory** and the **resume command** must be present. If either is missing, the button reports an error rather than launching a terminal that is guaranteed to fail.

### How the two resume paths differ

| | Vibe terminal tab | System terminal |
| --- | --- | --- |
| Where | Vibe's left rail | "Open in system terminal" on the session manager screen |
| Mechanism | A PTY that AI Switch owns | Hands off to your OS terminal application |
| Web Service mode | **Available** | **Not available** — the command is not exposed over HTTP |
| Good for | Keeping several tasks in one window | Using your own terminal setup (colours, keybindings, tmux) |

The system terminal handoff is per-OS:

| OS | Behaviour |
| --- | --- |
| Windows | `cmd.exe /D /K <command>` |
| macOS | `osascript` tells Terminal.app to run `cd -- '<dir>' && <command>` |
| Linux | Tries `x-terminal-emulator`, `gnome-terminal`, `konsole`, `xfce4-terminal` in order |

When none of the four is found on Linux you get "No supported terminal emulator was found" — use a Vibe terminal tab instead, or copy the resume command out and run it yourself.

::: tip If resume fails, suspect the CLI first
The resume command is assembled and handed to a shell; AI Switch does not interpret its semantics. If the terminal opens but the CLI reports "session not found", usually the CLI has pruned that transcript itself, or its resume subcommand changed in a newer version. Running the same command by hand in the same directory confirms which it is.
:::

## Sessions versus terminals

These two are easy to conflate, so to be explicit:

- **A session** is a transcript file on disk, written by a CLI and only read by AI Switch. Quit AI Switch and it is still there.
- **A terminal** is a PTY process AI Switch owns, with a lifetime tied to the app. Close the tab and the process is killed.

One session can be resumed any number of times, producing a new terminal each time. And a terminal need not correspond to any session at all — a plain shell tab doesn't.

## Privacy

Transcripts routinely contain code, file paths, and sometimes secrets. The behaviour here is:

- **Read-only, never uploaded.** All parsing happens locally — in the desktop process, or on whichever machine runs the web service.
- **Nothing is copied or cached.** Session content is never written into AI Switch's database; every list and view re-reads the files.
- **Web Service mode deserves care.** Exposing the web service means exposing every CLI transcript on that machine to anyone who can get past authentication. Configure your access token properly — see [Web Service Mode](/en/deploy/web-service) and [Remote Access and HTTPS](/en/deploy/remote-access).

## Next steps

- [Vibe Terminal and Skins](/en/features/vibe) — resume sessions in an in-app terminal tab
- [Platform Support Matrix](/en/guide/platform-support) — `session_resume` and terminal launch per platform
- [MCP Servers](/en/features/mcp) — configuring tool servers across these same CLIs
- [Skills Management](/en/features/skills) — managing Skills across these same CLIs
