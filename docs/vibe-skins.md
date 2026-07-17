# Vibe Skin Packages

Vibe mode accepts three skin file types:

- `.json`: a plain JSON skin manifest.
- `.aiskin`: a JSON manifest, or a zip package using the `.aiskin` extension.
- `.zip`: a package containing `skin.json` or `vibe-skin.json`.

Zip packages can include image assets. When `ui.backgroundImage`, `regions.*.backgroundImage`, or `showcase.image` is a relative path, Vibe resolves it from inside the zip package and stores it as a data URL in local storage.

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
    "background": "#f4fbff",
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
    "sidebar": {
      "backgroundImage": "assets/sidebar.png",
      "backgroundSize": "cover",
      "backgroundPosition": "center"
    },
    "terminalShell": {
      "background": "#f7fbff",
      "backgroundImage": "assets/terminal-shell.png",
      "borderRadius": "16px",
      "shadow": "0 18px 34px rgba(18,91,166,0.14)"
    },
    "rightRail": {
      "background": "linear-gradient(180deg, #d8f3ff, #4a9dde)"
    }
  },
  "showcase": {
    "enabled": true,
    "title": "My Display Area",
    "badge": "Custom Skin",
    "image": "assets/avatar.png",
    "footer": "made for Vibe"
  }
}
```

All `ui` fields are optional. Missing values fall back to the built-in Codex 2007 Blue skin. The `terminal` object is also optional and may override any xterm color key supported by the app.

## Region Keys

Skins can define styles for these regions:

- `app`
- `body`
- `titlebar`
- `toolbar`
- `sidebar`
- `sidebarHeader`
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

## Zip Layout

```text
my-skin.zip
  skin.json
  assets/
    background.png
    sidebar.png
    terminal-shell.png
    avatar.png
```

The app accepts PNG, JPG, WEBP, GIF, and SVG asset paths. Imported skins are limited to 8 MB before extraction and must fit in browser local storage after assets are embedded.
