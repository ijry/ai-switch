---
title: Skills Management
description: Browse, edit and install Agent Skills across 11 AI CLIs from one screen, with global and project scope, skill-directory and single-file layouts, and two bundled Skill packs totalling 27 Skills.
---

# Skills Management

Agent Skills are a way to hand a reusable workflow to an AI agent: a Skill is a Markdown document whose YAML frontmatter describes when it should be used. The agent reads that description and knows which procedure to follow when it hits a matching task.

As with [MCP servers](/en/features/mcp), the pain point is that **every CLI has its own Skill directories**. AI Switch's Skills management unifies them: one screen to browse every client's Skills, edit their content directly, and install the bundled Skill packs into a target CLI.

Reach it from "Skills" under the "System" group in the main sidebar.

## What a Skill looks like

The common form is a **Skill directory** containing at least a `SKILL.md`:

```text
~/.codex/skills/
└── systematic-debugging/
    ├── SKILL.md
    └── (optional helper files, scripts, templates)
```

`SKILL.md` opens with YAML frontmatter:

```text
---
name: systematic-debugging
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

Body: the actual procedure...
```

`description` is the line that matters most — it is how the agent decides whether this Skill applies to the task at hand. So it should describe *when to use this*, not *what this is*.

The other form is a **single Markdown file** (`markdown_file` layout): an `xxx.md` dropped straight into the Skills directory. This layout is **supported only by Codex CLI**; every other client recognises Skill directories only.

Skill ids are validated: not empty, not `.` or `..`, not starting with `.`, and free of `/`, `\`, `:`, whitespace, and control characters. Violations return `skills.invalid_id`.

## The 11 supported clients

Skill directories per client — global paths relative to your home directory, project paths relative to the workspace root:

| Client | Global Skill directories | Project Skill directories |
| --- | --- | --- |
| Codex CLI | `$CODEX_HOME/skills`, `$CODEX_HOME/skills/.system` (read-only), `~/.agents/skills` | `.codex/skills`, `.agents/skills` |
| Claude Code | `~/.claude/skills` | `.claude/skills` |
| Gemini CLI | `~/.gemini/skills`, `~/.agents/skills` | `.gemini/skills`, `.agents/skills` |
| Grok | `$GROK_HOME/skills` | `.grok/skills` |
| OpenCode | `~/.config/opencode/skills`, `~/.agents/skills` | `.agents/skills`, `.opencode/skills` |
| OpenClaw | `~/.openclaw/skills` | `skills` |
| Hermes Agent | `$HERMES_HOME/skills` | none |
| Cline | `~/.agents/skills`, `~/.cline/skills` | `.agents/skills`, `.cline/skills`, `.clinerules/skills`, `.claude/skills` |
| Cursor | `~/.cursor/skills`, `~/.agents/skills`, `~/.cursor/skills-cursor` (read-only) | `.cursor/skills`, `.agents/skills` |
| Kimi Code | `$KIMI_CODE_HOME/skills` | `.kimi-code/skills` |
| CodeBuddy | `~/.codebuddy/skills` | `.codebuddy/skills` |

Four clients honour a home-directory override: `CODEX_HOME`, `GROK_HOME`, `HERMES_HOME`, `KIMI_CODE_HOME`. Those variables also accept `~` and `~/something`.

Worth noticing:

- **`~/.agents/skills` is a shared, cross-tool location.** Five clients scan it — Codex, Gemini CLI, OpenCode, Cline, and Cursor — so a Skill placed there is available to all of them without keeping five copies.
- **Codex's `$CODEX_HOME/skills/.system` is marked read-only** (those are Codex's own system Skills), and so is **Cursor's `~/.cursor/skills-cursor`**.
- **Hermes Agent has no project-scope directory.** Switching to project scope leaves its Skill list empty.
- **OpenClaw's project directory is plain `skills`** at the workspace root, with no dot prefix.
- **Cline's project scope also reads `.claude/skills`**, so project Skills you commit for Claude Code work in Cline as well.
- **Only Codex supports the single-Markdown-file layout.**

Path resolution includes an escape guard: a Skill path must still resolve inside its own storage directory, or you get `skills.path_invalid`. A non-existent workspace directory in project scope returns `skills.directory_missing`.

## Read-only Skills

Skills discovered under a read-only root carry a "Read only" badge: the edit button is disabled (with a "this Skill comes from a read-only directory" tooltip) and delete is hidden entirely.

The reason is that those directories belong to the client itself — edits get clobbered on upgrade and may break the client's own integrity checks. To change a read-only Skill, copy it into a writable directory and edit the copy.

## Global versus project scope

Skills have two scopes:

| Scope | Location | Use for |
| --- | --- | --- |
| Global (`global`) | Under your home directory | General workflows: debugging methodology, code review procedure |
| Project (`project`) | Under the workspace directory | Project-specific conventions: this repo's release process, its testing rules |

Global is the default. Switching to project scope **requires a workspace path first** — with the path empty no query runs at all, because "project scope, unknown project" is meaningless.

Project Skills can be committed to the repository, so everyone who clones it gets the same set.

## The screen

Three selectors sit at the top: **client** (Codex CLI by default), **scope** (global by default), and, in project scope, the **workspace path**. Below that are two tabs:

### Skills

Lists every Skill found for the current client and scope, showing id, source, layout, and description. You can create, edit, and delete (read-only Skills excepted).

The editor has three inputs:

1. **Skill id** — the directory or file name.
2. **Layout** — Skill directory, or Markdown file (the latter only for Codex).
3. **Content** — the full body of `SKILL.md` (or the `.md` file), frontmatter included.

Each Skill's source is labelled builtin, Codex, Agents, project, or unknown, reflecting which root it was discovered under.

### Skill packages

Lists the bundled Skill packs and their installation state for the current client, with an "Install missing Skills" button in the pack detail.

## The two bundled Skill packs

AI Switch ships two Skill packs, 27 Skills in total. Both are marked builtin and read-only, and both can currently **only be installed for Codex CLI**. For any other client the packages tab is empty, and attempting an install returns "AI Switch Skill packages can currently be installed for Codex CLI only".

### AI Switch Core Skill Pack (`ai-switch.core`, 14 Skills)

Pack description: Core agent workflow Skills bundled by AI Switch.

| Skill | When to use it |
| --- | --- |
| `brainstorming` | Before any creative work — new features, components, behaviour changes: explore intent, requirements, and design first |
| `dispatching-parallel-agents` | Facing 2+ independent tasks with no shared state or sequential dependency |
| `executing-plans` | You have a written implementation plan to execute in a separate session with review checkpoints |
| `finishing-a-development-branch` | Implementation is complete and tests pass; you need to decide how to integrate the work |
| `receiving-code-review` | On receiving review feedback, before implementing it — especially when it seems unclear or technically questionable; demands rigour and verification, not performative agreement or blind compliance |
| `requesting-code-review` | On completing tasks, shipping major features, or before merging, to verify the work meets requirements |
| `subagent-driven-development` | Executing a plan with independent tasks in the current session |
| `systematic-debugging` | On any bug, test failure, or unexpected behaviour, before proposing fixes |
| `test-driven-development` | Implementing any feature or bugfix, before writing implementation code |
| `using-git-worktrees` | Starting feature work that needs isolation from the current workspace, or before executing a plan |
| `using-superpowers` | At the start of any conversation — establishes how to find and use Skills, requiring Skill invocation before *any* response, including clarifying questions |
| `verification-before-completion` | About to claim work is complete, fixed, or passing, and before committing or opening PRs — run the verification commands and confirm the output first; evidence before assertions |
| `writing-plans` | You have a spec or requirements for a multi-step task, before touching code |
| `writing-skills` | Creating, editing, or verifying Skills before deployment |

This pack is essentially **engineering discipline**: think before you build, test-drive, verify before claiming done, review with rigour.

### AI Switch Science Skill Pack (`ai-switch.science`, 13 Skills)

Pack description: Scientific research and analysis Skills bundled by AI Switch. All contributed by K-Dense Inc. under the MIT licence.

| Skill | What it does |
| --- | --- |
| `citation-management` | Searches Google Scholar and PubMed, extracts accurate metadata, validates citations, generates correct BibTeX |
| `experimental-design` | Designs studies **before** data collection: choosing a design, randomising, blocking, laying out treatment combinations |
| `exploratory-data-analysis` | EDA across 200+ scientific file formats, auto-detecting type and producing a quality report |
| `hypothesis-generation` | Turns observations or data into testable hypotheses with predictions, proposed mechanisms, and validating experiments |
| `paper-lookup` | Searches 10 academic APIs (PubMed, PMC, bioRxiv, medRxiv, arXiv, OpenAlex, Crossref, Semantic Scholar, CORE, Unpaywall) with reproducible provenance |
| `peer-review` | Checklist-driven formal peer review: methodology, statistical validity, reporting-standard compliance (CONSORT/STROBE) |
| `scholar-evaluation` | Scores scholarly work with the ScholarEval framework across problem formulation, methodology, analysis, and writing |
| `scientific-brainstorming` | Open-ended research ideation, interdisciplinary connections, challenging assumptions, finding gaps — for the stage before you have specific observations |
| `scientific-critical-thinking` | Evaluates claims and evidence quality, identifies bias and confounders, applies GRADE and Cochrane risk-of-bias frameworks |
| `scientific-schematics` | Publication-quality diagrams: neural network architectures, system diagrams, flowcharts, biological pathways |
| `scientific-visualization` | Journal-submission figures: multi-panel layouts, significance annotations, error bars, colourblind-safe palettes, per-journal formatting |
| `statistical-analysis` | End-to-end guided analysis: test selection, assumption checking, effect sizes, power, Bayesian alternatives, APA-formatted reporting |
| `statistical-power` | Sample size and power: a priori analysis, minimum detectable effect, power curves, plus Monte Carlo simulation for designs with no closed form |

These Skills cross-reference each other explicitly: `experimental-design` covers the design, `statistical-power` the sample size, and `statistical-analysis` data already collected — each description says which of the others to hand off to.

## Install behaviour

When you click "Install missing Skills":

1. The set of Skill ids already present for the target client is computed.
2. Only Skills **not in that set** are copied.
3. Copying **writes only files that do not exist and never overwrites**.
4. Ids already present are recorded in a "skipped" list and reported in the result.

::: tip Same id means skip, regardless of origin
"Already installed" is determined purely by id match, not by whether the Skill came from this pack. So if you wrote your own Skill called `writing-plans`, installing the Core pack skips it and leaves yours intact. To take the pack's version instead, delete yours first, then install.
:::

The install target is the **first writable (non-read-only) root** for the current scope. For Codex in global scope that means `$CODEX_HOME/skills`.

## Where the pack files are found

Skill pack files are distributed with the app and located at runtime in this order:

1. The `AI_SWITCH_SKILL_PACKAGES_DIR` environment variable, for development and unusual deployments.
2. Candidates next to the executable: `skill-packages`, `resources/skill-packages`, `_up_/skill-packages`, `../skill-packages`, `../resources/skill-packages`.
3. Candidates relative to the working directory: `src-tauri/resources/skill-packages`, `resources/skill-packages`, `../src-tauri/resources/skill-packages`.

Group 2 covers the three platforms' differing bundle layouts; group 3 exists for development runs such as `pnpm tauri:dev` — see [Local Setup](/en/dev/local-setup).

## When changes take effect

As with MCP, Skills are loaded **at client startup**. After installing or editing, an already-running CLI process will not notice — restart it, or open a fresh tab in the [Vibe terminal](/en/features/vibe).

## Writing your own Skill

1. Pick the client and scope on screen, then create.
2. Choose a descriptive, hyphenated id (`deploy-to-staging`, not `deploy`).
3. **Make `description` a trigger condition.** Look at how the bundled Skills word it — they all start with "Use when …" and describe the situation, not the artefact. That one line decides whether the agent ever thinks of your Skill.
4. Write the procedure in the body. Numbered steps and explicit decision points beat long prose.
5. Verify in a fresh CLI session: throw it a task that should match, and watch whether the agent actually follows the procedure.

To share Skills with a team, use **project scope** and commit them. To share across tools, Codex users can drop them in `~/.agents/skills`.

## Next steps

- [MCP Servers](/en/features/mcp) — configuring tool servers across clients
- [Vibe Terminal and Skins](/en/features/vibe) — open a fresh terminal to check a Skill loads
- [Session Management](/en/features/sessions) — review whether the agent actually followed the Skill
- [Platform Support Matrix](/en/guide/platform-support) — capabilities per platform
