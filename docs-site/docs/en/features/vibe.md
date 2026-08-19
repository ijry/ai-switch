---
title: Vibe Terminal and Skins
description: Vibe is AI Switch's built-in multi-tab terminal workspace for launching agents and resuming local transcripts, with three bundled skins and a fully customisable skin package format (.json / .aiskin / .zip).
---

# Vibe Terminal and Skins

Vibe is a **full-screen terminal workspace** inside AI Switch. It is not a replacement for your terminal emulator — it collapses three steps into one screen: pick an agent, pick a project directory, open a terminal. Discovered local transcripts sit in the left rail, terminal tabs fill the middle, and the launch bar sits at the bottom. Every tab is backed by a real PTY process rendered with xterm.js.

You get there via "Switch to Vibe mode" at the top of the main sidebar; Vibe has a way back to the main screen too.

## Three launch kinds

When Vibe opens a terminal there are exactly three intents (`TerminalLaunchKind`), and the backend decides what to actually run from that:

| Kind | Triggered by | What runs |
| --- | --- | --- |
| `shell` | New shell tab | Your default system shell |
| `agent` | The launch bar or the "create session" dialog | An executable resolved from the platform name |
| `resume` | The resume button on a transcript in the left rail | The session's resume command, handed to a shell |

The platform-to-executable map for `agent` launches is a fixed set of seven, hard-coded in the backend:

| Platform argument | Executable launched |
| --- | --- |
| `codex` | `codex` |
| `claude` | `claude` |
| `grok` | `grok` |
| `gemini` | `gemini` |
| `opencode` | `opencode` |
| `openclaw` | `openclaw` |
| `hermes` | `hermes` |

Anything else is rejected with `Unsupported terminal platform`. Which also means **Vibe assumes the CLI is already on your PATH** — AI Switch does not install them for you.

Two checks run before launch: the working directory must be non-empty and must actually exist (`Working directory does not exist`), and a `resume` launch must carry a command (`Resume command is required`). Terminals start at 100 columns × 30 rows, then fit to the pane and push the new size down to the PTY.

For where the transcript list comes from and how resume commands are built, see [Session Management](/en/features/sessions).

## Terminal behaviour

- **Output**: the backend streams PTY output as `terminal://output` events, with `terminal://exit` and `terminal://error` for process exit and terminal failures. Exit codes are printed inline (`[process exited: 0]`).
- **Input**: xterm's `onData` is written straight back to the PTY, control characters included.
- **Closing a tab**: if the process is still running, closing the tab kills it first, then removes the tab.
- **Font**: JetBrains Mono, then Cascadia Code, SF Mono, Consolas, at 13px.

Vibe works both on the desktop and in [Web Service mode](/en/deploy/web-service) — creating, writing to, resizing, and killing terminals all have HTTP commands. The one difference is the **directory picker**: the desktop can open a native folder dialog, whereas in a browser you pick from the dropdown of known directories or type a path.

## Three appearance modes

The appearance panel (reachable from the skin taskbar's appearance entry, or the appearance button in the UI) offers three themes:

| Theme | Meaning |
| --- | --- |
| `light` | Light |
| `dark` | Solarized Dark — the default |
| `skin` | Skin mode: apply the currently selected skin package |

Your choice is persisted in browser local storage under `ai-switch.vibe.appearance` (theme, skin id, sound toggle). An imported custom skin lives separately under `ai-switch.vibe.custom-skin`.

::: tip The terminal is transparent in skin mode
In skin mode Vibe forces the xterm wrapper, viewport, screen, rows, and canvas layers to transparent backgrounds so the skin's `terminalShell` styling (image, gradient, border) shows through. Readability therefore depends on your foreground colours such as `terminal.foreground`, not on `terminal.background`.
:::

## The three bundled skins

Bundled skins and imported skins go through **exactly the same parser** — they are three ordinary manifest files in the repository, with no privileged path:

- `src/skins/codex-2007-blue/skin.json`
- `src/skins/rescue-pups-adventure-bay/skin.json`
- `src/skins/starship-cockpit/skin.json`

How they actually differ:

| | Codex 2007 Blue | Rescue Pups (汪汪队救援主题) | Starship Cockpit (星舰驾驶舱) |
| --- | --- | --- | --- |
| Manifest `id` | `codex-2007-blue` | `rescue-pups-adventure-bay` | `starship-cockpit` |
| `decorations.variant` | `codex-2007` | `rescue-pups` | `starship-cockpit` |
| Regions styled | 31 | 49 | 49 |
| `blocks` | titlebar / profile / showcase / statusbar / taskbar | same | same, plus `launch` |
| Avatar template | none (uses the showcase mascot) | `rescue-rider` | `space-ai-core` |
| Showcase template | `qq-mascot` | `rescue-hq` | `space-ship` |
| Right-rail cards | `qq-person` | `rescue-dog-team`, `rescue-civic` | `space-radar`, `space-ship`, `space-starmap`, `space-telemetry` |
| Taskbar start button | 开始 | 出动 | 舰桥 |
| Audio | none | none | **yes** — 3 event sounds + 1 ambient loop |
| Bundled assets | none | none | three WAVs under `assets/sounds/` |

All three build their look from pure CSS gradients and the app's built-in vector decorations, so **none of them depends on external images**. That makes their manifests directly copyable as templates for your own skin.

Starship Cockpit is the only one with an `assets/` directory, and the only one that defines `audio`.

## Skin package format

Three file types are accepted on import:

| Extension | Contents |
| --- | --- |
| `.json` | A plain JSON manifest, no assets |
| `.aiskin` | Either a JSON manifest or a renamed zip — if JSON parsing fails, it is retried as a zip |
| `.zip` | Must contain `skin.json` or `vibe-skin.json` |

Relative asset paths inside a zip are read at import time and converted to data URLs stored alongside the manifest, so an imported skin no longer depends on the original archive.

Two size limits apply:

- The imported file must not exceed **8 MB** (`8 * 1024 * 1024`).
- The serialised skin, assets inlined, must stay under roughly **4.5 MB**, or local storage rejects it.

Image MIME types are inferred from the extension: `.jpg` / `.jpeg`, `.webp`, `.gif`, `.svg`; anything else is treated as PNG. Audio accepts only `.mp3`, `.ogg`, and `.wav` — references with any other extension are dropped.

Absolute paths, URLs with a scheme, and paths containing `..` are all treated as unsafe references and discarded. Only in-package relative paths and allowlisted data URLs get through.

A typical package looks like this:

```text
my-skin.zip
├── skin.json
└── assets/
    ├── background.png
    ├── terminal-shell.png
    ├── avatar.png
    └── sounds/
        └── ping.wav
```

## The skin.json structure

The manifest is a JSON object with these top-level fields:

```json
{
  "id": "my-skin",
  "name": "My Skin",
  "author": "someone",
  "version": "1.0.0",
  "ui": {},
  "terminal": {},
  "regions": {},
  "blocks": {},
  "decorations": {},
  "audio": {},
  "showcase": {}
}
```

Only `id` and `name` are needed for identity; everything else is optional and any missing value falls back to the built-in Codex 2007 Blue.

### ui: the global palette

`ui` is the skin's palette. Every field takes a raw CSS value, so a solid colour, a gradient, or a `linear-gradient(...)` all work:

```json
{
  "ui": {
    "accent": "#1678d8",
    "accentText": "#ffffff",
    "background": "linear-gradient(180deg, #63b9fb 0%, #0e62b8 100%)",
    "backgroundImage": "assets/background.png",
    "backgroundOverlay": "linear-gradient(180deg, rgba(255,255,255,0.24), rgba(8,63,126,0.24))",
    "panel": "rgba(226, 245, 255, 0.88)",
    "panelStrong": "rgba(255, 255, 255, 0.96)",
    "panelSubtle": "rgba(188, 226, 250, 0.8)",
    "border": "rgba(14, 99, 181, 0.42)",
    "text": "#0d315d",
    "mutedText": "#3d6d9f",
    "button": "linear-gradient(180deg, #63c7ff 0%, #0c5cab 100%)",
    "buttonText": "#ffffff",
    "buttonHover": "linear-gradient(180deg, #7bd3ff 0%, #0b539e 100%)",
    "dangerBackground": "linear-gradient(180deg, #ff7e87, #b72434)",
    "dangerText": "#ffffff",
    "tabBar": "rgba(239,250,255,0.94)",
    "tabActive": "#ffffff",
    "tabInactive": "rgba(151,210,247,0.54)",
    "tabHover": "rgba(255, 255, 255, 0.72)",
    "focus": "#44a7ff"
  }
}
```

### terminal: xterm colours

`terminal` can override any of xterm's 18 colour keys:

```text
background  foreground
black       red         green        yellow
blue        magenta     cyan         white
brightBlack brightRed   brightGreen  brightYellow
brightBlue  brightMagenta brightCyan brightWhite
```

Keys you leave out keep the current light/dark theme's default.

### regions: per-area styling

`regions` is where a skin has the most reach — it overrides styling area by area. There are **59** region keys:

```text
app                body               titlebar           titlebarControls
windowButton       windowButtonMinimize windowButtonMaximize windowButtonClose
toolbar            sidebar            sidebarHeader      sidebarProfile
avatar             onlineBadge        profileBadge       controlPanel
sessionList        listTrigger        sessionRow         groupPanel
workspace          tabBar             tab                tabActive
tabClose           terminalShell      emptyState         modal
rightRail          rightCard          showcaseStage      showcaseFigure
showcaseFooter     statusBar          button             buttonHover
ghostButton        field              select             danger
showcaseOrb        launchPanel        agentStrip         agentOption
agentOptionActive  composer           composerInput      composerMetaBar
composerControl    composerSendButton composerAddon      taskbar
taskbarStartButton taskbarStartMenu   taskbarMenuItem    taskbarItem
taskbarItemActive  taskbarTray        taskbarClock
```

Each region accepts **16** style fields:

```text
background  backgroundImage  backgroundOverlay  backgroundSize
backgroundPosition  backgroundRepeat  border  color
shadow  backdropFilter  borderRadius  padding
fontSize  lineHeight  letterSpacing  textTransform
```

For example, giving the terminal shell a background image:

```json
{
  "regions": {
    "terminalShell": {
      "backgroundImage": "assets/terminal-shell.png",
      "backgroundSize": "cover",
      "backgroundPosition": "center",
      "border": "1px solid rgba(255,255,255,0.24)",
      "borderRadius": "12px",
      "shadow": "0 18px 48px rgba(0,0,0,0.42)"
    }
  }
}
```

### blocks: copy and imagery

`blocks` replaces **text and image references** that the app renders. It cannot inject structure:

| Block | Fields |
| --- | --- |
| `titlebar` | `title`, `subtitle`, `badge` |
| `profile` | `name`, `status`, `signature`, `badge`, `avatar` |
| `showcase` | `enabled`, `title`, `subtitle`, `body`, `badge`, `figure`, `footer` |
| `statusbar` | `left`, `right` |
| `launch` | `title`, `body`, `placeholder`, `sendLabel`, `folderLabel`, `modelLabel`, `reasoningLabel`, `agentStripLabel`, `agentStripPrefix`, `agentStripSuffix`, `extraLabel`, `extraValue` |
| `taskbar` | `enabled`, `startButton`, `startMenu`, `items`, `tray`, `clockFormat` |

`blocks.showcase` takes precedence over the older top-level `showcase` field.

Start-menu entries may only invoke four actions; anything else is ignored:

| Action | Effect |
| --- | --- |
| `openAppearance` | Open the appearance panel |
| `setTheme` | Switch theme; `theme` accepts only `dark`, `light`, `skin` |
| `importSkin` | Open the skin import dialog |
| `clearSkin` | Remove the imported custom skin |

The menu can also carry `{"type": "separator"}` dividers and decorative entries marked `disabled: true`.

### decorations: vector templates

Decorations are not free-form HTML. You pick from an allowlist of vector graphics built into the app:

- `variant`: `codex-2007`, `rescue-pups`, `starship-cockpit`
- `titlebarMark`: the titlebar corner glyph, truncated past 4 characters
- `avatarTemplate` / `showcaseTemplate` / `rightCards[].template` / `rightCards[].items[].template`: one of 13 templates — `qq-mascot`, `qq-person`, `rescue-rider`, `rescue-hq`, `rescue-dog-team`, `rescue-civic`, `rescue-mayor`, `rescue-chicken`, `space-ai-core`, `space-ship`, `space-radar`, `space-telemetry`, `space-starmap`
- `items[].tone`: `red`, `blue`, `yellow`, `green`, `pink`, `orange`, `neutral`

Values outside the allowlist are silently ignored — no error, and nothing renders.

### audio: sound effects

`audio` is optional. The Starship Cockpit skin demonstrates the full shape:

```json
{
  "audio": {
    "enabled": true,
    "volume": 0.48,
    "events": {
      "agentSelect": "assets/sounds/weapon-switch.wav",
      "hologramInteract": "assets/sounds/hologram-tap.wav",
      "radarPulse": "assets/sounds/radar-pulse.wav"
    },
    "ambient": [
      { "id": "radar", "src": "assets/sounds/radar-pulse.wav", "intervalMs": 8500, "volume": 0.18 }
    ]
  }
}
```

- `events` recognises exactly three names: `agentSelect` (switching agent), `hologramInteract` (tapping a hologram decoration), and `radarPulse`.
- `ambient` holds at most 6 entries. `loop: true` plays continuously; `intervalMs` re-triggers on that millisecond interval; with neither, it plays once.
- Volumes are clamped. Event sounds default to 0.5, ambient to 0.35.
- Sound only plays in **skin mode**, with the appearance panel's "Skin sound effects" toggle on, and only if the skin itself has not set `enabled: false`. Ambient loops additionally wait for a first user interaction (browser autoplay policy).

## Security boundary

Skins are files people download and import casually, so the parser is deliberately narrow:

- **No code from a skin is ever executed.** No HTML fragments, no external stylesheets, no scripts — only strings, colour values, and image/audio references.
- **Decorations, taskbar actions, and decoration tones are allowlisted.** Unrecognised values are dropped.
- **Agent icons in the launch bar are built-in vectors** and cannot be overridden by a skin.
- **The minimise/maximise/close buttons in the titlebar are decorative** and intentionally wired to nothing.
- **Relative assets can only point inside the package.** `..`, absolute paths, and external URLs are refused.

In short: the worst a malicious skin can do is make the UI ugly. It gets no execution capability.

## Building your own skin

1. Copy one of the directories under `src/skins/`, or start from `fixtures/vibe-skins/rescue-pups/skin.json`.
2. Change `id` (it must be unique) and `name`.
3. Tune the `ui` palette. This is the highest-leverage step by a wide margin.
4. Only then reach for `regions` for per-area polish, and `blocks` for copy changes.
5. Put any images or sounds under `assets/` and reference them with relative paths.
6. Package it:
   - No assets → just rename `skin.json` to `my-skin.aiskin`.
   - With assets → zip `skin.json` together with `assets/`, keeping `skin.json` at the archive root rather than nested in a folder.
7. Hit "Import skin" in the appearance panel and pick the file. A successful import switches you into skin mode automatically. "Clear custom skin" undoes it.

## Next steps

- [Session Management](/en/features/sessions) — where Vibe's left-rail transcript list comes from
- [Accounts and the Pool](/en/guide/accounts) — routing the CLIs you launch in Vibe through the pool
- [Platform Support Matrix](/en/guide/platform-support) — which platforms support terminal launch and session resume
- [Architecture](/en/dev/architecture) — where terminals and PTYs sit in the overall design
