# Vibe Starship Cockpit Skin Design

## Goal

Add a built-in Vibe skin that makes the app feel like a cinematic starship cockpit: deep-space fast travel, cockpit glass HUD, radar scanning, a slowly rotating spacecraft model, and typewriter-style telemetry output.

The skin must remain a standard editable skin package so users can copy and customize it later.

## Scope

- Add a built-in standard skin package at `src/skins/starship-cockpit/skin.json`.
- Extend the safe decoration template whitelist with space cockpit templates.
- Keep uploaded skin packages data-driven. They can select approved templates and style regions, but cannot inject HTML, JavaScript, CSS files, or native commands.
- Style the shell through the existing skin region variables wherever possible.
- Add only targeted shared rendering/CSS needed for reusable templates and animation.

## Visual Direction

- Overall view: a starship moving quickly through deep space, using layered gradients, star streaks, cockpit glass arcs, and holographic cyan/amber HUD lighting.
- Top bar: a cockpit canopy status strip with Chinese labels, simulated window controls, mission/status text, and technical spacing.
- Left sidebar: a ship task/channel panel. The profile area becomes an AI core/captain status block, while session groups look like mission folders or deck channels.
- Workspace: a transparent command glass panel. The terminal background stays transparent so the cockpit backdrop remains visible while terminal text remains readable.
- Tabs: thin HUD-style segmented tabs, with close icons inheriting the tab background and only changing color/glow on hover.
- Right rail: a display bay with a radar scan, rotating spacecraft, and telemetry output.
- Bottom taskbar/status: ship console footer with system indicators, signal labels, and mission clock.

## New Safe Templates

- `space-ai-core`: avatar/profile mark for the left sidebar.
- `space-ship`: CSS-built spacecraft display with slow 3D-like rotation.
- `space-radar`: circular radar scope with sweep animation and target blips.
- `space-telemetry`: typewriter-like diagnostic output using static lines from the skin manifest.
- `space-starmap`: compact orbital map or route display for secondary right cards.

Templates are rendered by React components selected from the manifest. They do not execute user-provided code.

## Data Model

Extend the existing decoration unions:

- `VIBE_SKIN_DECORATION_VARIANTS` gains `starship-cockpit`.
- `VIBE_SKIN_DECORATION_TEMPLATES` gains the space templates listed above.

The existing `decorations.rightCards[].items` model remains enough for telemetry and status chips. If a template needs repeated labels, it reads from `items[].label` and `items[].badge`.

## Animation And Accessibility

- Radar sweep, starfield movement, ship rotation, and telemetry cursor use CSS animations.
- Motion is decorative only and must not affect layout.
- `prefers-reduced-motion: reduce` disables or greatly slows starfield, radar sweep, ship rotation, and typing animations.
- Contrast must stay readable on dark HUD panels.

## Testing

- Unit tests verify the new built-in skin is listed and importable as a standard skin manifest.
- Vibe screen tests verify the skin renders the cockpit templates, including radar, ship, telemetry, and AI core.
- Existing terminal transparency tests remain valid.

## Out Of Scope

- No Three.js, WebGL, Canvas, external images, or external font loading.
- No arbitrary custom HTML/CSS/JS in imported skin files.
- No changes to non-Vibe screens.
