# Vibe Region Skin Design

## Scope

Vibe skin mode should support highly customized UI skins without changing the existing dark and light themes. A skin can style the app background, title bar, left session/control rail, terminal workspace shell, tab bar, modal, right showcase rail, status bar, and reusable controls. The built-in default should interpret the QQ2007-style reference as composable chrome, panels, gradients, and glossy regions rather than using the reference image as a full-screen background.

## Architecture

Skin files remain JSON or zip packages. The manifest keeps the existing `ui` fallback colors and adds a `regions` map keyed by named UI regions. Each region accepts presentation fields such as `background`, `backgroundImage`, `backgroundOverlay`, `backgroundSize`, `backgroundPosition`, `backgroundRepeat`, `border`, `color`, `shadow`, `backdropFilter`, `borderRadius`, `padding`, and typography values. Imported zip assets are resolved to data URLs for any region `backgroundImage` and optional showcase image.

The Vibe screen renders skin mode with explicit layout regions: title bar, sidebar, control panel, session list, workspace, tab bar, terminal shell, optional right showcase rail, and status bar. Each region maps to CSS variables generated from the active skin. The terminal output itself stays isolated in `XtermPane`; skins can style the outer terminal shell and xterm color theme, but decorative skin backgrounds must not obscure terminal text.

## Data Flow

`importVibeSkinPackage` normalizes manifests, resolves zip assets, and stores the skin in local storage. `skinToCssVariables` converts `ui` and `regions` values into CSS custom properties. `VibeScreen` applies those variables only when `themeMode === "skin"` and uses the active skin's optional `showcase` metadata to decide whether to render the right display rail.

## Error Handling

Invalid manifests continue to fail with actionable import errors. Relative image references are only allowed for zip/aiskin packages that contain the referenced asset. Oversized imports and local-storage overflow keep the existing size guard behavior.

## Testing

Tests should cover region asset import, CSS variable generation for region styles, stored skin normalization, Vibe skin mode rendering, right showcase visibility, and terminal theme override behavior. Typecheck and the relevant Vitest suites must pass before committing implementation changes.

## Self Review

The design has no placeholders. The scope is limited to Vibe skin mode, avoids dark/light regressions, keeps the terminal readable, and defines explicit behavior for manifest parsing, layout mapping, and tests.
