# Vibe Skin Packages

Vibe mode accepts three skin file types:

- `.json`: a plain JSON skin manifest.
- `.aiskin`: a JSON manifest, or a zip package using the `.aiskin` extension.
- `.zip`: a package containing `skin.json` or `vibe-skin.json`.

Zip packages can include image assets. When `ui.backgroundImage`, `regions.*.backgroundImage`, `showcase.image`, `blocks.profile.avatar`, or `blocks.showcase.figure` is a relative path, Vibe resolves it from inside the zip package and stores it as a data URL in local storage.

The built-in `Codex 2007 Blue` skin is QQ2007-inspired chrome: glossy title bar, left rail, tab strip, terminal shell, right display rail, and status bar. It does not directly use the reference image as a full-screen background.

## Minimal Manifest

```json
{
  "id": "my-blue-skin",
  "name": "My Blue Skin",
  "author": "Your Name",
  "version": "1.0.0",
  "ui": {
    "accent": "#1678d8",
    "background": "linear-gradient(135deg, #dff5ff, #1157a4)",
    "backgroundImage": "assets/background.png",
    "panel": "rgba(232, 247, 255, 0.78)",
    "panelStrong": "rgba(255, 255, 255, 0.92)",
    "panelSubtle": "rgba(216, 239, 255, 0.68)",
    "border": "rgba(15, 99, 184, 0.34)",
    "text": "#0d315d",
    "mutedText": "#386b9e",
    "button": "linear-gradient(180deg, #49a7ff, #126fc5)",
    "buttonHover": "linear-gradient(180deg, #5eb4ff, #0f61ae)",
    "focus": "#44a7ff"
  },
  "terminal": {
    "background": "transparent",
    "foreground": "#12375f",
    "blue": "#0d6ec9",
    "green": "#2f854d",
    "red": "#b73546",
    "yellow": "#b37613"
  },
  "regions": {
    "titlebar": {
      "background": "linear-gradient(180deg, #e7fbff, #0f6bc4)",
      "border": "rgba(5, 82, 150, 0.65)",
      "color": "#ffffff"
    },
    "windowButtonClose": {
      "background": "linear-gradient(180deg, #ffb0b7, #b8202f)"
    },
    "sidebarProfile": {
      "background": "linear-gradient(180deg, rgba(255,255,255,0.75), rgba(186,231,255,0.5))"
    },
    "avatar": {
      "backgroundImage": "assets/avatar-frame.png",
      "backgroundSize": "cover"
    },
    "terminalShell": {
      "background": "rgba(247,251,255,0.42)",
      "backgroundImage": "assets/terminal-shell.png",
      "borderRadius": "16px",
      "shadow": "0 18px 34px rgba(18,91,166,0.14)"
    },
    "showcaseStage": {
      "backgroundImage": "assets/showcase-stage.png",
      "backgroundSize": "cover"
    },
    "showcaseFigure": {
      "shadow": "0 18px 30px rgba(10,82,154,0.2)"
    }
  },
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
      "badge": "我的QQ秀",
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

All `ui` fields are optional. Missing values fall back to the built-in Codex 2007 Blue skin. The `terminal` object is also optional and may override any xterm color key supported by the app.

## Content Blocks

`blocks` customizes safe app-rendered skin content. Values are strings and image references only; the app never executes user HTML, CSS files, or scripts from a skin package.

Supported blocks:

- `blocks.titlebar.title`: main Chinese-style title text in the skin titlebar.
- `blocks.titlebar.subtitle`: smaller titlebar subtitle.
- `blocks.titlebar.badge`: small badge before the visual window controls.
- `blocks.profile.name`: nickname in the left profile card.
- `blocks.profile.status`: presence text, for example `在线`.
- `blocks.profile.signature`: short status signature under the nickname.
- `blocks.profile.badge`: small profile badge.
- `blocks.profile.avatar`: avatar image path or data URL.
- `blocks.showcase.title`: right QQ秀-style display title.
- `blocks.showcase.subtitle`: right display subtitle.
- `blocks.showcase.body`: descriptive text inside the showcase stage.
- `blocks.showcase.badge`: small right rail label.
- `blocks.showcase.figure`: figure image path or data URL for the QQ秀-style stage.
- `blocks.showcase.footer`: footer tag under the showcase stage.
- `blocks.statusbar.left`: left status bar text.
- `blocks.statusbar.right`: right status bar text.

If `blocks.showcase` is omitted, the older `showcase` object still renders in the right rail. If `blocks.showcase` exists, it takes precedence over `showcase`.

The minimize, maximize, and close controls in the skin titlebar are decorative. They are intentionally inert and do not call native window APIs.

## Region Keys

Skins can define styles for these regions:

- `app`
- `body`
- `titlebar`
- `titlebarControls`
- `windowButton`
- `windowButtonMinimize`
- `windowButtonMaximize`
- `windowButtonClose`
- `toolbar`
- `sidebar`
- `sidebarHeader`
- `sidebarProfile`
- `avatar`
- `onlineBadge`
- `profileBadge`
- `controlPanel`
- `sessionList`
- `listTrigger`
- `sessionRow`
- `groupPanel`
- `workspace`
- `tabBar`
- `tab`
- `tabActive`
- `tabClose`
- `terminalShell`
- `emptyState`
- `modal`
- `rightRail`
- `rightCard`
- `showcaseStage`
- `showcaseFigure`
- `showcaseFooter`
- `statusBar`
- `button`
- `buttonHover`
- `ghostButton`
- `field`
- `select`
- `danger`
- `showcaseOrb`

Each region can use these style fields:

- `background`
- `backgroundImage`
- `backgroundOverlay`
- `backgroundSize`
- `backgroundPosition`
- `backgroundRepeat`
- `border`
- `color`
- `shadow`
- `backdropFilter`
- `borderRadius`
- `padding`
- `fontSize`
- `lineHeight`
- `letterSpacing`
- `textTransform`

`terminalShell` styles the decorative shell around the terminal. Use `terminal.background` and `terminal.foreground` for terminal text colors; do not rely on decorative backgrounds to carry terminal readability.

In skin mode, Vibe keeps the xterm wrapper, viewport, screen, rows, and canvas layers transparent so the `terminalShell` region can remain visible after creating new terminals. Use `terminal.foreground` and the other terminal color keys for text readability, and keep decorative terminal backgrounds on `regions.terminalShell`.

## Zip Layout

```text
my-skin.zip
  skin.json
  assets/
    background.png
    sidebar.png
    terminal-shell.png
    avatar-frame.png
    avatar.png
    showcase-stage.png
    qqshow.png
```

The app accepts PNG, JPG, WEBP, GIF, and SVG asset paths. Imported skins are limited to 8 MB before extraction and must fit in browser local storage after assets are embedded.
