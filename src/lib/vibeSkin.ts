import JSZip from "jszip";
import type { CSSProperties } from "react";

export const VIBE_SKIN_STORAGE_KEY = "ai-switch.vibe.custom-skin";

const MAX_IMPORT_SIZE_BYTES = 8 * 1024 * 1024;
const MAX_STORED_SKIN_BYTES = 4_500_000;
const ZIP_MANIFEST_NAMES = ["skin.json", "vibe-skin.json"];

export type VibeTerminalThemeKey =
  | "background"
  | "foreground"
  | "black"
  | "red"
  | "green"
  | "yellow"
  | "blue"
  | "magenta"
  | "cyan"
  | "white"
  | "brightBlack"
  | "brightRed"
  | "brightGreen"
  | "brightYellow"
  | "brightBlue"
  | "brightMagenta"
  | "brightCyan"
  | "brightWhite";

export type VibeTerminalTheme = Partial<Record<VibeTerminalThemeKey, string>>;

export type VibeSkinUi = {
  accent: string;
  accentText: string;
  background: string;
  backgroundImage?: string;
  backgroundOverlay: string;
  panel: string;
  panelStrong: string;
  panelSubtle: string;
  border: string;
  text: string;
  mutedText: string;
  button: string;
  buttonText: string;
  buttonHover: string;
  dangerBackground: string;
  dangerText: string;
  tabBar: string;
  tabActive: string;
  tabInactive: string;
  tabHover: string;
  focus: string;
};

export const VIBE_SKIN_REGION_KEYS = [
  "app",
  "body",
  "titlebar",
  "toolbar",
  "sidebar",
  "sidebarHeader",
  "controlPanel",
  "sessionList",
  "listTrigger",
  "sessionRow",
  "groupPanel",
  "workspace",
  "tabBar",
  "tab",
  "tabActive",
  "tabClose",
  "terminalShell",
  "emptyState",
  "modal",
  "rightRail",
  "rightCard",
  "statusBar",
  "button",
  "buttonHover",
  "ghostButton",
  "field",
  "select",
  "danger",
  "showcaseOrb",
] as const;

export type VibeSkinRegionKey = (typeof VIBE_SKIN_REGION_KEYS)[number];

const VIBE_SKIN_REGION_STYLE_KEYS = [
  "background",
  "backgroundImage",
  "backgroundOverlay",
  "backgroundSize",
  "backgroundPosition",
  "backgroundRepeat",
  "border",
  "color",
  "shadow",
  "backdropFilter",
  "borderRadius",
  "padding",
  "fontSize",
  "lineHeight",
  "letterSpacing",
  "textTransform",
] as const;

export type VibeSkinRegionStyle = Partial<
  Record<(typeof VIBE_SKIN_REGION_STYLE_KEYS)[number], string>
>;

export type VibeSkinShowcase = {
  enabled?: boolean;
  title?: string;
  subtitle?: string;
  body?: string;
  badge?: string;
  image?: string;
  footer?: string;
};

export type VibeSkinDefinition = {
  id: string;
  name: string;
  author?: string;
  version?: string;
  ui: VibeSkinUi;
  terminal?: VibeTerminalTheme;
  regions?: Partial<Record<VibeSkinRegionKey, VibeSkinRegionStyle>>;
  showcase?: VibeSkinShowcase;
};

export const BUILT_IN_VIBE_SKINS: VibeSkinDefinition[] = [
  {
    id: "codex-2007-blue",
    name: "Codex 2007 Blue",
    author: "AI Switch",
    version: "2.0.0",
    ui: {
      accent: "#1678d8",
      accentText: "#ffffff",
      background:
        "linear-gradient(180deg, #63b9fb 0%, #2b89df 42%, #0e62b8 100%)",
      backgroundOverlay:
        "radial-gradient(circle at 18% 6%, rgba(255,255,255,0.72), transparent 17rem), radial-gradient(circle at 82% 12%, rgba(163,222,255,0.56), transparent 16rem), linear-gradient(180deg, rgba(255,255,255,0.24), rgba(8,63,126,0.24))",
      panel: "rgba(226, 245, 255, 0.88)",
      panelStrong: "rgba(255, 255, 255, 0.96)",
      panelSubtle: "rgba(188, 226, 250, 0.8)",
      border: "rgba(14, 99, 181, 0.42)",
      text: "#0d315d",
      mutedText: "#3d6d9f",
      button: "linear-gradient(180deg, #63c7ff 0%, #1678d8 48%, #0c5cab 100%)",
      buttonText: "#ffffff",
      buttonHover: "linear-gradient(180deg, #7bd3ff 0%, #2088e5 48%, #0b539e 100%)",
      dangerBackground: "linear-gradient(180deg, #ff7e87, #b72434)",
      dangerText: "#ffffff",
      tabBar: "linear-gradient(180deg, rgba(239,250,255,0.94), rgba(183,226,252,0.9))",
      tabActive: "linear-gradient(180deg, #ffffff 0%, #f5fcff 48%, #d7f0ff 100%)",
      tabInactive: "linear-gradient(180deg, rgba(151,210,247,0.54), rgba(103,176,229,0.38))",
      tabHover: "rgba(255, 255, 255, 0.72)",
      focus: "#44a7ff",
    },
    terminal: {
      background: "#f7fbff",
      black: "#1f4b75",
      blue: "#0d6ec9",
      brightBlack: "#6088ad",
      brightBlue: "#268fe8",
      brightCyan: "#20a6c9",
      brightGreen: "#3d9b5c",
      brightMagenta: "#9b63c6",
      brightRed: "#d94b5a",
      brightWhite: "#ffffff",
      brightYellow: "#d4931f",
      cyan: "#1685a8",
      foreground: "#12375f",
      green: "#2f854d",
      magenta: "#7c55ab",
      red: "#b73546",
      white: "#d9edf8",
      yellow: "#b37613",
    },
    regions: {
      app: {
        background:
          "linear-gradient(180deg, #6fc5ff 0%, #2f91e5 36%, #126bc4 100%)",
        backgroundOverlay:
          "radial-gradient(circle at 12% 2%, rgba(255,255,255,0.94), transparent 12rem), radial-gradient(circle at 92% 0%, rgba(195,240,255,0.68), transparent 15rem), linear-gradient(90deg, rgba(255,255,255,0.18), transparent 28%, rgba(0,61,129,0.16))",
        border: "rgba(7, 86, 160, 0.72)",
        color: "#0d315d",
      },
      titlebar: {
        background:
          "linear-gradient(180deg, #e7fbff 0%, #8bd7ff 18%, #34a0ef 45%, #0f6bc4 100%)",
        backgroundOverlay:
          "linear-gradient(90deg, rgba(255,255,255,0.48), transparent 32%, rgba(255,255,255,0.2) 64%, transparent)",
        border: "rgba(5, 82, 150, 0.65)",
        color: "#ffffff",
        shadow: "inset 0 1px 0 rgba(255,255,255,0.86), 0 1px 0 rgba(7,74,139,0.35)",
      },
      sidebar: {
        background:
          "linear-gradient(180deg, rgba(231,249,255,0.96) 0%, rgba(179,225,252,0.92) 42%, rgba(96,169,226,0.86) 100%)",
        backgroundOverlay:
          "radial-gradient(circle at 18% 8%, rgba(255,255,255,0.9), transparent 9rem), linear-gradient(90deg, rgba(255,255,255,0.32), transparent 72%)",
        border: "rgba(13, 104, 190, 0.52)",
        shadow: "inset -1px 0 0 rgba(255,255,255,0.42)",
      },
      sidebarHeader: {
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.98) 0%, rgba(221,246,255,0.96) 48%, rgba(159,218,252,0.94) 100%)",
        border: "rgba(31, 132, 211, 0.42)",
        shadow: "0 12px 22px rgba(15,99,184,0.16), inset 0 1px 0 rgba(255,255,255,0.86)",
        backdropFilter: "blur(14px)",
      },
      controlPanel: {
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.88), rgba(207,237,255,0.82))",
        border: "rgba(31, 132, 211, 0.38)",
        shadow: "0 10px 20px rgba(15,99,184,0.12)",
        backdropFilter: "blur(14px)",
      },
      groupPanel: {
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.96), rgba(225,246,255,0.94) 48%, rgba(193,229,251,0.9))",
        border: "rgba(38, 143, 222, 0.34)",
        shadow: "0 10px 18px rgba(13,104,190,0.1), inset 0 1px 0 rgba(255,255,255,0.9)",
      },
      workspace: {
        background:
          "linear-gradient(180deg, rgba(231,248,255,0.94), rgba(198,233,252,0.92) 46%, rgba(154,209,244,0.88))",
        backgroundOverlay:
          "linear-gradient(90deg, rgba(255,255,255,0.4), transparent 34%), radial-gradient(circle at 86% 12%, rgba(255,255,255,0.64), transparent 12rem)",
        border: "rgba(13, 104, 190, 0.5)",
        shadow: "inset 1px 0 0 rgba(255,255,255,0.42), 0 18px 44px rgba(4,54,112,0.16)",
      },
      tabBar: {
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.96), rgba(214,241,255,0.94) 54%, rgba(159,217,251,0.92))",
        border: "rgba(31, 132, 211, 0.38)",
        shadow: "inset 0 -1px 0 rgba(28,121,203,0.2)",
      },
      terminalShell: {
        background: "#f7fbff",
        border: "rgba(20, 105, 184, 0.36)",
        shadow: "inset 0 1px 0 rgba(255,255,255,0.95), 0 18px 34px rgba(18,91,166,0.14)",
        borderRadius: "16px",
      },
      emptyState: {
        background:
          "radial-gradient(circle at 50% 38%, rgba(255,255,255,0.98), rgba(233,248,255,0.82) 44%, rgba(186,226,251,0.42) 72%, transparent)",
        color: "#0d315d",
      },
      rightRail: {
        background:
          "linear-gradient(180deg, rgba(216,243,255,0.92), rgba(151,211,247,0.86) 56%, rgba(74,157,222,0.8))",
        backgroundOverlay:
          "radial-gradient(circle at 68% 8%, rgba(255,255,255,0.94), transparent 8rem), linear-gradient(90deg, rgba(255,255,255,0.24), transparent)",
        border: "rgba(13, 104, 190, 0.46)",
        shadow: "inset 1px 0 0 rgba(255,255,255,0.48)",
      },
      rightCard: {
        background:
          "linear-gradient(180deg, rgba(255,255,255,0.96), rgba(226,247,255,0.94) 50%, rgba(172,224,252,0.92))",
        border: "rgba(31, 132, 211, 0.38)",
        shadow: "0 16px 28px rgba(10,82,154,0.16), inset 0 1px 0 rgba(255,255,255,0.92)",
      },
      statusBar: {
        background:
          "linear-gradient(180deg, rgba(211,239,255,0.94), rgba(139,204,244,0.92))",
        border: "rgba(13, 104, 190, 0.48)",
        color: "#174a7c",
        shadow: "inset 0 1px 0 rgba(255,255,255,0.7)",
      },
      field: {
        background: "linear-gradient(180deg, #ffffff, #e7f7ff)",
        border: "rgba(31, 132, 211, 0.42)",
      },
    },
    showcase: {
      enabled: true,
      title: "Codex 2007",
      subtitle: "Blue chrome skin",
      body: "Region-based skinning keeps terminal output clear while the shell, panels, and showcase stay expressive.",
      badge: "Vibe Skin",
      footer: "QQ2007-inspired layout",
    },
  },
];

const FALLBACK_UI = BUILT_IN_VIBE_SKINS[0].ui;
const TERMINAL_THEME_KEYS: VibeTerminalThemeKey[] = [
  "background",
  "foreground",
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
];

type AssetResolver = (path: string) => Promise<string>;

function asRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Skin manifest must be a JSON object.");
  }
  return value as Record<string, unknown>;
}

function optionalString(value: unknown) {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function sanitizeId(value: unknown, fallbackName: string) {
  const source = optionalString(value) ?? fallbackName;
  const normalized = source
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return normalized || "custom-vibe-skin";
}

function isResolvedImageReference(value: string) {
  return /^(data:|blob:|https?:\/\/|\/)/i.test(value);
}

function normalizeTerminalTheme(value: unknown): VibeTerminalTheme | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const theme: VibeTerminalTheme = {};
  for (const key of TERMINAL_THEME_KEYS) {
    const color = optionalString(source[key]);
    if (color) {
      theme[key] = color;
    }
  }
  return Object.keys(theme).length > 0 ? theme : undefined;
}

function normalizeRegionStyle(value: unknown): VibeSkinRegionStyle | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const style: VibeSkinRegionStyle = {};
  for (const key of VIBE_SKIN_REGION_STYLE_KEYS) {
    const item = optionalString(source[key]);
    if (item) {
      style[key] = item;
    }
  }
  return Object.keys(style).length > 0 ? style : undefined;
}

function normalizeRegions(
  value: unknown,
): Partial<Record<VibeSkinRegionKey, VibeSkinRegionStyle>> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const regions: Partial<Record<VibeSkinRegionKey, VibeSkinRegionStyle>> = {};
  for (const key of VIBE_SKIN_REGION_KEYS) {
    const style = normalizeRegionStyle(source[key]);
    if (style) {
      regions[key] = style;
    }
  }
  return Object.keys(regions).length > 0 ? regions : undefined;
}

function normalizeShowcase(value: unknown): VibeSkinShowcase | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const showcase: VibeSkinShowcase = {};
  if (typeof source.enabled === "boolean") {
    showcase.enabled = source.enabled;
  }
  for (const key of ["title", "subtitle", "body", "badge", "image", "footer"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      showcase[key] = item;
    }
  }
  return Object.keys(showcase).length > 0 ? showcase : undefined;
}

async function resolveImageReference(
  value: string | undefined,
  fieldName: string,
  resolveAsset?: AssetResolver,
) {
  if (!value || isResolvedImageReference(value)) {
    return value;
  }
  if (!resolveAsset) {
    throw new Error(`Relative ${fieldName} requires a zip skin package.`);
  }
  return resolveAsset(value);
}

async function resolveRegionAssets(
  regions: Partial<Record<VibeSkinRegionKey, VibeSkinRegionStyle>> | undefined,
  resolveAsset?: AssetResolver,
) {
  if (!regions) {
    return;
  }

  for (const [key, style] of Object.entries(regions) as [
    VibeSkinRegionKey,
    VibeSkinRegionStyle,
  ][]) {
    if (style.backgroundImage) {
      style.backgroundImage = await resolveImageReference(
        style.backgroundImage,
        `regions.${key}.backgroundImage`,
        resolveAsset,
      );
    }
  }
}

async function normalizeSkinManifest(
  manifest: unknown,
  resolveAsset?: AssetResolver,
): Promise<VibeSkinDefinition> {
  const raw = asRecord(manifest);
  const name = optionalString(raw.name) ?? "Custom Vibe Skin";
  const rawUi = raw.ui ? asRecord(raw.ui) : {};
  const ui: VibeSkinUi = {
    ...FALLBACK_UI,
    ...Object.fromEntries(
      Object.entries(rawUi).filter(([, value]) => typeof value === "string" && value.trim()),
    ),
  };
  const regions = normalizeRegions(raw.regions);
  const showcase = normalizeShowcase(raw.showcase);

  ui.backgroundImage = await resolveImageReference(
    ui.backgroundImage,
    "ui.backgroundImage",
    resolveAsset,
  );
  await resolveRegionAssets(regions, resolveAsset);
  if (showcase?.image) {
    showcase.image = await resolveImageReference(showcase.image, "showcase.image", resolveAsset);
  }

  return {
    id: sanitizeId(raw.id, name),
    name,
    author: optionalString(raw.author),
    version: optionalString(raw.version),
    ui,
    terminal: normalizeTerminalTheme(raw.terminal),
    regions,
    showcase,
  };
}

function mimeTypeForPath(path: string) {
  const lower = path.toLowerCase();
  if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) {
    return "image/jpeg";
  }
  if (lower.endsWith(".webp")) {
    return "image/webp";
  }
  if (lower.endsWith(".gif")) {
    return "image/gif";
  }
  if (lower.endsWith(".svg")) {
    return "image/svg+xml";
  }
  return "image/png";
}

function readFileAsArrayBuffer(file: File): Promise<ArrayBuffer> {
  const modernFile = file as File & { arrayBuffer?: () => Promise<ArrayBuffer> };
  if (typeof modernFile.arrayBuffer === "function") {
    return modernFile.arrayBuffer();
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Could not read skin file."));
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) {
        resolve(reader.result);
        return;
      }
      reject(new Error("Could not read skin file as binary data."));
    };
    reader.readAsArrayBuffer(file);
  });
}

function readFileAsText(file: File): Promise<string> {
  const modernFile = file as File & { text?: () => Promise<string> };
  if (typeof modernFile.text === "function") {
    return modernFile.text();
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Could not read skin file."));
    reader.onload = () => {
      if (typeof reader.result === "string") {
        resolve(reader.result);
        return;
      }
      reject(new Error("Could not read skin file as text."));
    };
    reader.readAsText(file);
  });
}

function normalizeZipPath(path: string) {
  return path.replace(/\\/g, "/").replace(/^\.?\//, "");
}

async function importZipSkin(file: File): Promise<VibeSkinDefinition> {
  const zip = await JSZip.loadAsync(await readFileAsArrayBuffer(file));
  const manifestEntry =
    ZIP_MANIFEST_NAMES.map((name) => zip.file(name)).find(Boolean) ??
    Object.values(zip.files).find((entry) => !entry.dir && /(^|\/)skin\.json$/i.test(entry.name));

  if (!manifestEntry) {
    throw new Error("Zip skin package must contain skin.json.");
  }

  const manifest = JSON.parse(await manifestEntry.async("string")) as unknown;
  const resolveAsset: AssetResolver = async (assetPath) => {
    const normalized = normalizeZipPath(assetPath);
    const entry = zip.file(normalized);
    if (!entry || entry.dir) {
      throw new Error(`Skin asset not found: ${assetPath}`);
    }
    const base64 = await entry.async("base64");
    return `data:${mimeTypeForPath(normalized)};base64,${base64}`;
  };

  return normalizeSkinManifest(manifest, resolveAsset);
}

export async function importVibeSkinPackage(file: File): Promise<VibeSkinDefinition> {
  if (file.size > MAX_IMPORT_SIZE_BYTES) {
    throw new Error("Skin file is larger than 8 MB.");
  }

  const lowerName = file.name.toLowerCase();
  if (lowerName.endsWith(".zip")) {
    return importZipSkin(file);
  }

  if (!lowerName.endsWith(".json") && !lowerName.endsWith(".aiskin")) {
    throw new Error("Choose a .aiskin, .json, or .zip skin package.");
  }

  const text = await readFileAsText(file);
  let manifest: unknown;
  try {
    manifest = JSON.parse(text) as unknown;
  } catch (error) {
    if (lowerName.endsWith(".aiskin")) {
      return importZipSkin(file);
    }
    throw error;
  }

  return normalizeSkinManifest(manifest);
}

function normalizeStoredSkin(value: unknown): VibeSkinDefinition {
  const raw = asRecord(value);
  const rawUi = raw.ui ? asRecord(raw.ui) : {};
  const name = optionalString(raw.name) ?? "Custom Vibe Skin";
  return {
    id: sanitizeId(raw.id, name),
    name,
    author: optionalString(raw.author),
    version: optionalString(raw.version),
    ui: {
      ...FALLBACK_UI,
      ...Object.fromEntries(
        Object.entries(rawUi).filter(([, item]) => typeof item === "string" && item.trim()),
      ),
    },
    terminal: normalizeTerminalTheme(raw.terminal),
    regions: normalizeRegions(raw.regions),
    showcase: normalizeShowcase(raw.showcase),
  };
}

export function readStoredVibeSkin(): VibeSkinDefinition | null {
  try {
    const stored = window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY);
    return stored ? normalizeStoredSkin(JSON.parse(stored)) : null;
  } catch {
    return null;
  }
}

export function writeStoredVibeSkin(skin: VibeSkinDefinition) {
  const serialized = JSON.stringify(skin);
  if (serialized.length > MAX_STORED_SKIN_BYTES) {
    throw new Error("Skin package is too large to keep in local storage.");
  }
  window.localStorage.setItem(VIBE_SKIN_STORAGE_KEY, serialized);
}

export function clearStoredVibeSkin() {
  window.localStorage.removeItem(VIBE_SKIN_STORAGE_KEY);
}

function cssUrl(value: string) {
  const escaped = value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/[\r\n]/g, "");
  return `url("${escaped}")`;
}

function backgroundLayerFromStyle(style: {
  background?: string;
  backgroundImage?: string;
  backgroundOverlay?: string;
}) {
  const imageLayer = style.backgroundImage ? cssUrl(style.backgroundImage) : "";
  return [style.backgroundOverlay, imageLayer, style.background].filter(Boolean).join(", ");
}

function regionCssName(region: VibeSkinRegionKey) {
  return region.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}

function assignRegionCssVariables(
  variables: Record<string, string>,
  region: VibeSkinRegionKey,
  style: VibeSkinRegionStyle,
) {
  const prefix = `--vibe-${regionCssName(region)}`;
  const backgroundLayer = backgroundLayerFromStyle(style);
  if (backgroundLayer) {
    variables[`${prefix}-background-layer`] = backgroundLayer;
  }
  if (style.background) {
    variables[`${prefix}-background`] = style.background;
  }
  if (style.backgroundImage) {
    variables[`${prefix}-background-image`] = cssUrl(style.backgroundImage);
  }
  if (style.backgroundOverlay) {
    variables[`${prefix}-background-overlay`] = style.backgroundOverlay;
  }
  if (style.backgroundSize) {
    variables[`${prefix}-background-size`] = style.backgroundSize;
  }
  if (style.backgroundPosition) {
    variables[`${prefix}-background-position`] = style.backgroundPosition;
  }
  if (style.backgroundRepeat) {
    variables[`${prefix}-background-repeat`] = style.backgroundRepeat;
  }
  if (style.border) {
    variables[`${prefix}-border`] = style.border;
  }
  if (style.color) {
    variables[`${prefix}-color`] = style.color;
  }
  if (style.shadow) {
    variables[`${prefix}-shadow`] = style.shadow;
  }
  if (style.backdropFilter) {
    variables[`${prefix}-backdrop-filter`] = style.backdropFilter;
  }
  if (style.borderRadius) {
    variables[`${prefix}-border-radius`] = style.borderRadius;
  }
  if (style.padding) {
    variables[`${prefix}-padding`] = style.padding;
  }
  if (style.fontSize) {
    variables[`${prefix}-font-size`] = style.fontSize;
  }
  if (style.lineHeight) {
    variables[`${prefix}-line-height`] = style.lineHeight;
  }
  if (style.letterSpacing) {
    variables[`${prefix}-letter-spacing`] = style.letterSpacing;
  }
  if (style.textTransform) {
    variables[`${prefix}-text-transform`] = style.textTransform;
  }
}

export function skinToCssVariables(skin: VibeSkinDefinition): CSSProperties {
  const backgroundLayer = backgroundLayerFromStyle(skin.ui);
  const variables: Record<string, string> = {
    "--vibe-accent": skin.ui.accent,
    "--vibe-accent-text": skin.ui.accentText,
    "--vibe-background": skin.ui.background,
    "--vibe-background-layer": backgroundLayer,
    "--vibe-panel": skin.ui.panel,
    "--vibe-panel-strong": skin.ui.panelStrong,
    "--vibe-panel-subtle": skin.ui.panelSubtle,
    "--vibe-border": skin.ui.border,
    "--vibe-text": skin.ui.text,
    "--vibe-muted-text": skin.ui.mutedText,
    "--vibe-button": skin.ui.button,
    "--vibe-button-text": skin.ui.buttonText,
    "--vibe-button-hover": skin.ui.buttonHover,
    "--vibe-danger-background": skin.ui.dangerBackground,
    "--vibe-danger-text": skin.ui.dangerText,
    "--vibe-tab-bar": skin.ui.tabBar,
    "--vibe-tab-active": skin.ui.tabActive,
    "--vibe-tab-inactive": skin.ui.tabInactive,
    "--vibe-tab-hover": skin.ui.tabHover,
    "--vibe-focus": skin.ui.focus,
  };

  for (const [region, style] of Object.entries(skin.regions ?? {}) as [
    VibeSkinRegionKey,
    VibeSkinRegionStyle,
  ][]) {
    assignRegionCssVariables(variables, region, style);
  }

  return variables as CSSProperties;
}
