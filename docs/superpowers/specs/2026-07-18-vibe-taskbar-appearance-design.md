# Vibe Taskbar And Appearance Popup Design

## Goal

Add a QQ2007/Windows-XP-inspired bottom taskbar to Vibe skin mode, with controlled simple interactions, and move Vibe theme and skin management into a unified appearance popup. Also compact long left-rail directory labels to the last two path segments.

## Scope

- Skin mode renders a bottom decorative taskbar when enabled by the active skin.
- Taskbar interactions are limited to a small hardcoded action allowlist.
- The existing theme cycle button becomes an appearance entry point instead of cycling immediately.
- The appearance popup contains dark, light, and skin theme choices plus skin selection, import, and clear controls.
- Long left directory names display only the last two path segments, for example `D:/Repos/xyito/open/ai-switch` displays as `open/ai-switch`.
- Dark and light non-skin layouts keep their current structure except for the appearance popup replacing direct theme cycling.

## Skin Manifest Model

Extend the existing safe `blocks` model with an optional `blocks.taskbar` object:

```json
{
  "blocks": {
    "taskbar": {
      "enabled": true,
      "startButton": {
        "label": "开始",
        "icon": "assets/start.png"
      },
      "startMenu": {
        "items": [
          { "label": "外观设置", "action": "openAppearance" },
          { "label": "切换到皮肤模式", "action": "setTheme", "theme": "skin" },
          { "label": "切换暗色主题", "action": "setTheme", "theme": "dark" },
          { "label": "导入皮肤...", "action": "importSkin" },
          { "type": "separator" },
          { "label": "AI Switch 终端", "disabled": true }
        ]
      },
      "items": [
        { "label": "AI Switch 终端", "icon": "assets/app.png", "active": true }
      ],
      "tray": ["Vibe", "在线"],
      "clockFormat": "HH:mm"
    }
  }
}
```

Missing values fall back to built-in QQ2007-style defaults. Relative image paths in `blocks.taskbar.startButton.icon` and `blocks.taskbar.items[].icon` resolve from zip packages using the same asset resolver as avatar and showcase images.

## Safe Taskbar Actions

Taskbar menu items may use only these actions:

- `openAppearance`: opens the appearance popup.
- `setTheme`: switches to `dark`, `light`, or `skin` based on a normalized `theme` value.
- `importSkin`: opens the existing skin file picker.
- `clearSkin`: clears the custom skin only when one exists.

Unknown actions, malformed actions, disabled items, and separator items are ignored. Skin packages cannot define JavaScript, arbitrary URLs, native window calls, or custom callbacks.

## UI Behavior

In skin mode, `VibeScreen` renders the taskbar as the single bottom chrome row when `blocks.taskbar.enabled` is true. In that case it replaces the current skin status bar and can include `blocks.statusbar.left` as a small status label plus `blocks.statusbar.right` near the tray. If taskbar is disabled, the existing skin status bar continues to render unchanged. The main body remains flex-sized so the taskbar does not cover terminal content. The built-in taskbar includes a glossy blue-green start button, one active application item, a tray cluster, and a live clock.

Clicking the start button toggles a popup start menu anchored above the button. Clicking outside the menu or selecting a valid action closes the menu. The window titlebar controls remain decorative only; the taskbar is the only new skin element with limited interactions.

The appearance popup is shared by the normal left toolbar and taskbar menu. It contains:

- Segmented theme choices: dark, light, skin.
- Skin select dropdown shown when skin mode is active or selected.
- Import skin button using the existing hidden file input.
- Clear custom skin button when a custom skin exists.
- Short help text that skin files support safe blocks and region styles only.

The toolbar no longer permanently shows the skin dropdown and import button. The existing "Switch Vibe theme" button becomes "Appearance" behavior while preserving accessible naming for tests and keyboard users.

## Directory Label Compaction

Add a display helper for left-rail directory labels. It splits Windows and POSIX separators, removes empty segments, and returns the final two segments joined with `/`. If there are fewer than two segments, it returns the original visible label.

The full directory path remains available in `title`, `aria-label`, resume logic, group keys, and command inputs. Only rendered visual text is compacted.

## Styling Regions

Add taskbar region keys to `VIBE_SKIN_REGION_KEYS`:

- `taskbar`
- `taskbarStartButton`
- `taskbarStartMenu`
- `taskbarMenuItem`
- `taskbarItem`
- `taskbarItemActive`
- `taskbarTray`
- `taskbarClock`

Each uses the existing region style fields and CSS variable generation. The built-in skin should provide a glossy QQ2007-like blue taskbar without needing bitmap backgrounds, while custom skins can provide background images and icons.

## Components And Data Flow

- `src/lib/vibeSkin.ts` normalizes taskbar blocks, resolves taskbar icon assets, provides defaults, and exposes taskbar data through `getVibeSkinBlocks()`.
- `src/screens/VibeScreen.tsx` owns appearance popup state, start menu state, safe taskbar action dispatch, and compact directory display.
- `src/styles.css` consumes the new taskbar region variables and defines the built-in glossy taskbar look.
- Existing import and storage limits remain unchanged.

## Testing

Add focused tests for:

- Normalizing taskbar defaults, taskbar menu items, tray values, icon assets, and new region CSS variables.
- Rendering the built-in taskbar only in skin mode.
- Opening and closing the taskbar start menu.
- Executing safe actions from the start menu, especially `openAppearance`, `setTheme`, and `importSkin`.
- Opening the appearance popup from the toolbar and switching dark, light, and skin themes inside it.
- Moving skin select/import/clear controls into the appearance popup.
- Compacting left directory labels while preserving full path in labels or titles.
- Confirming unknown taskbar actions do not throw and do not execute arbitrary behavior.

## Compatibility

Existing skins without `blocks.taskbar` continue to work. The built-in skin enables the taskbar by default. Custom skins can disable it with `blocks.taskbar.enabled: false`. Existing `blocks.statusbar` remains valid; the taskbar is an additional richer bottom UI layer for skin mode.
