# Vibe QQ2007 Skin Blocks Design

## Scope

Vibe skin mode should restore more of the QQ2007-style reference through composable UI elements instead of a full-screen background image. The default `Codex 2007 Blue` skin should look more like a Chinese QQ2007 desktop client: Chinese decorative copy, visual-only window buttons, a profile/avatar area with online status, a QQ秀-like right showcase, glossy blue chrome, and auxiliary panels that skin authors can customize.

This work only changes `themeMode === "skin"`. Existing dark and light Vibe modes keep their current layout and behavior.

## User-Facing Requirements

- The built-in QQ2007-inspired skin uses Chinese decorative text by default, including title/profile/showcase/status copy.
- The skin title bar includes visual-only minimize, maximize, and close buttons. These buttons do not call Tauri window APIs and do not close the app.
- The left sidebar includes a profile block with avatar, online indicator, nickname, presence text, signature, and small badge/status details.
- The right rail includes a QQ秀-style display block with a stage, figure/avatar image support, title, subtitle/body copy, and footer tags.
- New terminal sessions must not introduce an opaque skin-mode terminal surface that hides the skin shell background. In skin mode, the xterm viewport/screen/canvas layers should stay transparent enough for the terminal shell skin to remain visible while terminal text remains readable through the configured terminal theme.
- User-created skins can customize these added blocks through manifest content and region styles, without allowing arbitrary HTML or script injection.

## Architecture

The existing `regions` model remains the styling layer. It should be extended with region keys for the new visual parts:

- `titlebarControls`
- `windowButton`
- `windowButtonMinimize`
- `windowButtonMaximize`
- `windowButtonClose`
- `sidebarProfile`
- `avatar`
- `onlineBadge`
- `profileBadge`
- `showcaseStage`
- `showcaseFigure`
- `showcaseFooter`

The manifest gains a structured `blocks` object for safe content customization. `blocks` is optional and normalized like `showcase`; missing values fall back to the built-in QQ2007 defaults. The first version should support:

```json
{
  "blocks": {
    "titlebar": {
      "title": "AI Switch 终端",
      "subtitle": "QQ2007 蓝色经典",
      "badge": "皮肤模式"
    },
    "profile": {
      "name": "AI Switch",
      "status": "在线",
      "signature": "正在使用 Vibe 终端",
      "badge": "经典蓝钻",
      "avatar": "assets/avatar.png"
    },
    "showcase": {
      "title": "QQ秀展示",
      "subtitle": "Codex 2007 Blue",
      "body": "右侧展示区可由皮肤定义图片、舞台和说明。",
      "figure": "assets/qqshow.png",
      "footer": "自定义展示区"
    },
    "statusbar": {
      "left": "AI Switch 已连接",
      "right": "皮肤区域已启用"
    }
  }
}
```

`blocks.profile.avatar` and `blocks.showcase.figure` resolve relative zip asset paths to data URLs using the same asset resolver as `ui.backgroundImage`, `regions.*.backgroundImage`, and `showcase.image`.

## Layout Design

Skin mode uses the current three-column frame but adds semantic sub-blocks.

The title bar uses a QQ2007-like composition: small app orb on the left, Chinese title/subtitle in the center-left, skin badge, then three visual window controls on the right. The controls must be keyboard-hidden or inert (`aria-hidden`) because they are decorative and do not perform actions.

The left sidebar header becomes a profile card. It keeps access to the existing exit/switch button but adds an avatar tile with an online dot, Chinese nickname/status/signature, and a small badge strip. This preserves the existing Vibe navigation actions while making the skin look like the QQ contact-list header.

The right rail becomes a showcase stage. If `blocks.showcase.figure` or legacy `showcase.image` exists, render it as the figure. Otherwise render a CSS-built default QQ秀-style figure/orb so the built-in skin looks complete without shipping a raster asset. The existing region list/debug chips should remain available but become secondary, below the showcase, so the right rail is not visually dominated by implementation metadata.

## Terminal Transparency

Skin backgrounds should apply to `.vibe-skin-terminal-shell`; terminal output readability should be controlled by `activeSkin.terminal`. When a terminal is created, `XtermPane` and xterm internals must not paint an opaque wrapper that covers the shell. CSS should explicitly keep these skin-mode layers transparent:

- `.vibe-skin-terminal-shell .xterm`
- `.vibe-skin-terminal-shell .xterm-screen`
- `.vibe-skin-terminal-shell .xterm-viewport`
- `.vibe-skin-terminal-shell .xterm-rows`
- `.vibe-skin-terminal-shell canvas`
- the local wrapper element used by `XtermPane`, if it has one

The built-in skin's terminal theme should use a transparent or very lightly translucent background only if xterm supports it safely; otherwise the shell remains visible around the terminal and the xterm viewport stays as clear as the library allows.

## Compatibility

Existing skin manifests remain valid. `showcase` remains supported for backward compatibility and maps into right rail display when `blocks.showcase` is absent. Existing `regions` keys continue to work. Dark and light themes do not render the new skin-only profile/showcase/window-control blocks.

Arbitrary HTML, CSS files, and script execution remain unsupported. Skins can provide strings and image references only; layout is controlled by the app.

## Error Handling

Invalid `blocks` entries are ignored field-by-field when they are missing or empty strings. Relative image paths in `blocks.profile.avatar` and `blocks.showcase.figure` require a zip/aiskin package and throw the same actionable missing-asset error style as existing skin image fields. Size limits for imported and stored skins remain unchanged.

## Testing

Tests should cover:

- Normalizing and storing `blocks` content.
- Resolving zip assets for `blocks.profile.avatar` and `blocks.showcase.figure`.
- Generating CSS variables for the new region keys.
- Rendering the built-in skin with Chinese title/profile/showcase/status text.
- Rendering visual-only window controls in skin mode.
- Rendering a custom skin's profile avatar/showcase figure.
- Keeping terminal skin layers transparent after a terminal session is rendered.
- Verifying dark and light modes do not render the skin-only QQ2007 blocks.

## Documentation

`docs/vibe-skins.md` should document `blocks`, the new region keys, the visual-only nature of window controls, and the terminal transparency rule. The example manifest should include Chinese QQ2007-style profile and showcase blocks.

## Self Review

The design has no placeholders. The scope is focused on Vibe skin mode and default QQ2007 fidelity. The architecture keeps arbitrary user HTML out of manifests, keeps existing skins compatible, and explicitly covers the terminal background issue reported after new terminal creation.
