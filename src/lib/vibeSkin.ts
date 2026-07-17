import JSZip from "jszip";
import type { CSSProperties } from "react";
import codex2007SkinImage from "../assets/vibe/codex-2007.png";

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

export type VibeSkinDefinition = {
  id: string;
  name: string;
  author?: string;
  version?: string;
  ui: VibeSkinUi;
  terminal?: VibeTerminalTheme;
};

export const BUILT_IN_VIBE_SKINS: VibeSkinDefinition[] = [
  {
    id: "codex-2007-blue",
    name: "Codex 2007 Blue",
    author: "AI Switch",
    version: "1.0.0",
    ui: {
      accent: "#1678d8",
      accentText: "#ffffff",
      background:
        "linear-gradient(135deg, #dff5ff 0%, #74b6ee 38%, #2777cc 72%, #1157a4 100%)",
      backgroundImage: codex2007SkinImage,
      backgroundOverlay:
        "radial-gradient(circle at 12% 10%, rgba(255,255,255,0.98), transparent 18rem), radial-gradient(circle at 86% 12%, rgba(143,214,255,0.78), transparent 16rem), linear-gradient(180deg, rgba(255,255,255,0.28), rgba(10,77,146,0.18))",
      panel: "rgba(232, 247, 255, 0.78)",
      panelStrong: "rgba(255, 255, 255, 0.92)",
      panelSubtle: "rgba(216, 239, 255, 0.68)",
      border: "rgba(15, 99, 184, 0.34)",
      text: "#0d315d",
      mutedText: "#386b9e",
      button: "linear-gradient(180deg, #49a7ff 0%, #126fc5 100%)",
      buttonText: "#ffffff",
      buttonHover: "linear-gradient(180deg, #5eb4ff 0%, #0f61ae 100%)",
      dangerBackground: "rgba(184, 39, 54, 0.92)",
      dangerText: "#ffffff",
      tabBar: "rgba(220, 242, 255, 0.82)",
      tabActive: "rgba(255, 255, 255, 0.96)",
      tabInactive: "rgba(126, 194, 239, 0.32)",
      tabHover: "rgba(255, 255, 255, 0.68)",
      focus: "#44a7ff",
    },
    terminal: {
      background: "#f4fbff",
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

function isExternalImageReference(value: string) {
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

  if (ui.backgroundImage && !isExternalImageReference(ui.backgroundImage)) {
    if (!resolveAsset) {
      throw new Error("Relative backgroundImage requires a zip skin package.");
    }
    ui.backgroundImage = await resolveAsset(ui.backgroundImage);
  }

  return {
    id: sanitizeId(raw.id, name),
    name,
    author: optionalString(raw.author),
    version: optionalString(raw.version),
    ui,
    terminal: normalizeTerminalTheme(raw.terminal),
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

export function skinToCssVariables(skin: VibeSkinDefinition): CSSProperties {
  const imageLayer = skin.ui.backgroundImage ? cssUrl(skin.ui.backgroundImage) : "";
  const backgroundLayer = [skin.ui.backgroundOverlay, imageLayer, skin.ui.background]
    .filter(Boolean)
    .join(", ");

  return {
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
  } as CSSProperties;
}
