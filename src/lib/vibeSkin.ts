import JSZip from "jszip";
import type { CSSProperties } from "react";

import codex2007BlueSkinManifest from "../skins/codex-2007-blue/skin.json";
import rescuePupsAdventureBaySkinManifest from "../skins/rescue-pups-adventure-bay/skin.json";

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
  "titlebarControls",
  "windowButton",
  "windowButtonMinimize",
  "windowButtonMaximize",
  "windowButtonClose",
  "toolbar",
  "sidebar",
  "sidebarHeader",
  "sidebarProfile",
  "avatar",
  "onlineBadge",
  "profileBadge",
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
  "showcaseStage",
  "showcaseFigure",
  "showcaseFooter",
  "statusBar",
  "button",
  "buttonHover",
  "ghostButton",
  "field",
  "select",
  "danger",
  "showcaseOrb",
  "taskbar",
  "taskbarStartButton",
  "taskbarStartMenu",
  "taskbarMenuItem",
  "taskbarItem",
  "taskbarItemActive",
  "taskbarTray",
  "taskbarClock",
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

export type VibeSkinTitlebarBlock = {
  title?: string;
  subtitle?: string;
  badge?: string;
};

export type VibeSkinProfileBlock = {
  name?: string;
  status?: string;
  signature?: string;
  badge?: string;
  avatar?: string;
};

export type VibeSkinShowcaseBlock = {
  enabled?: boolean;
  title?: string;
  subtitle?: string;
  body?: string;
  badge?: string;
  figure?: string;
  footer?: string;
};

export type VibeSkinStatusbarBlock = {
  left?: string;
  right?: string;
};

export type VibeSkinTaskbarAction = "openAppearance" | "setTheme" | "importSkin" | "clearSkin";

export type VibeSkinTaskbarMenuItem =
  | {
      type: "separator";
    }
  | {
      label: string;
      action?: VibeSkinTaskbarAction;
      theme?: "dark" | "light" | "skin";
      disabled?: boolean;
    };

export type VibeSkinTaskbarButton = {
  label?: string;
  icon?: string;
};

export type VibeSkinTaskbarItem = {
  label?: string;
  icon?: string;
  active?: boolean;
};

export type VibeSkinTaskbarBlock = {
  enabled?: boolean;
  startButton?: VibeSkinTaskbarButton;
  startMenu?: {
    items?: VibeSkinTaskbarMenuItem[];
  };
  items?: VibeSkinTaskbarItem[];
  tray?: string[];
  clockFormat?: "HH:mm";
};

export const VIBE_SKIN_DECORATION_VARIANTS = ["codex-2007", "rescue-pups"] as const;

export type VibeSkinDecorationVariant = (typeof VIBE_SKIN_DECORATION_VARIANTS)[number];

export const VIBE_SKIN_DECORATION_TEMPLATES = [
  "qq-mascot",
  "qq-person",
  "rescue-rider",
  "rescue-hq",
  "rescue-dog-team",
  "rescue-civic",
  "rescue-mayor",
  "rescue-chicken",
] as const;

export type VibeSkinDecorationTemplate = (typeof VIBE_SKIN_DECORATION_TEMPLATES)[number];

export const VIBE_SKIN_DECORATION_TONES = [
  "red",
  "blue",
  "yellow",
  "green",
  "pink",
  "orange",
  "neutral",
] as const;

export type VibeSkinDecorationTone = (typeof VIBE_SKIN_DECORATION_TONES)[number];

export type VibeSkinDecorationItem = {
  label: string;
  badge?: string;
  template?: VibeSkinDecorationTemplate;
  tone?: VibeSkinDecorationTone;
  image?: string;
};

export type VibeSkinDecorationCard = {
  title?: string;
  subtitle?: string;
  badge?: string;
  footer?: string;
  template?: VibeSkinDecorationTemplate;
  figure?: string;
  items?: VibeSkinDecorationItem[];
};

export type VibeSkinDecorations = {
  variant?: VibeSkinDecorationVariant;
  titlebarMark?: string;
  avatarTemplate?: VibeSkinDecorationTemplate;
  showcaseTemplate?: VibeSkinDecorationTemplate;
  rightCards?: VibeSkinDecorationCard[];
};

export type ResolvedVibeSkinTaskbarBlock = {
  enabled: boolean;
  startButton: Required<Pick<VibeSkinTaskbarButton, "label">> &
    Pick<VibeSkinTaskbarButton, "icon">;
  startMenu: {
    items: VibeSkinTaskbarMenuItem[];
  };
  items: Array<
    Required<Pick<VibeSkinTaskbarItem, "label" | "active">> &
      Pick<VibeSkinTaskbarItem, "icon">
  >;
  tray: string[];
  clockFormat: "HH:mm";
};

export type VibeSkinBlocks = {
  titlebar?: VibeSkinTitlebarBlock;
  profile?: VibeSkinProfileBlock;
  showcase?: VibeSkinShowcaseBlock;
  statusbar?: VibeSkinStatusbarBlock;
  taskbar?: VibeSkinTaskbarBlock;
};

export type ResolvedVibeSkinBlocks = {
  titlebar: Required<VibeSkinTitlebarBlock>;
  profile: Omit<Required<VibeSkinProfileBlock>, "avatar"> & Pick<VibeSkinProfileBlock, "avatar">;
  showcase: Omit<Required<VibeSkinShowcaseBlock>, "figure"> &
    Pick<VibeSkinShowcaseBlock, "figure">;
  statusbar: Required<VibeSkinStatusbarBlock>;
  taskbar: ResolvedVibeSkinTaskbarBlock;
};

export const DEFAULT_VIBE_SKIN_BLOCKS: ResolvedVibeSkinBlocks = {
  titlebar: {
    title: "AI Switch 终端",
    subtitle: "QQ2007 蓝色经典",
    badge: "皮肤模式",
  },
  profile: {
    name: "AI Switch",
    status: "在线",
    signature: "正在使用 Vibe 终端",
    badge: "经典蓝钻",
  },
  showcase: {
    enabled: true,
    title: "QQ秀展示",
    subtitle: "Codex 2007 Blue",
    body: "右侧展示区可由皮肤定义图片、舞台和说明。",
    badge: "我的QQ秀",
    footer: "自定义展示区",
  },
  statusbar: {
    left: "AI Switch 已连接",
    right: "皮肤区域已启用",
  },
  taskbar: {
    enabled: true,
    startButton: {
      label: "开始",
    },
    startMenu: {
      items: [
        { label: "外观设置", action: "openAppearance" },
        { label: "切换到皮肤模式", action: "setTheme", theme: "skin" },
        { label: "切换亮色主题", action: "setTheme", theme: "light" },
        { label: "切换暗色主题", action: "setTheme", theme: "dark" },
        { label: "导入皮肤...", action: "importSkin" },
        { type: "separator" },
        { label: "AI Switch 终端", disabled: true },
      ],
    },
    items: [{ label: "AI Switch 终端", active: true }],
    tray: ["Vibe", "在线"],
    clockFormat: "HH:mm",
  },
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
  blocks?: VibeSkinBlocks;
  decorations?: VibeSkinDecorations;
};

function asBuiltInVibeSkin(skin: unknown): VibeSkinDefinition {
  return skin as VibeSkinDefinition;
}

export const BUILT_IN_VIBE_SKINS: VibeSkinDefinition[] = [
  asBuiltInVibeSkin(codex2007BlueSkinManifest),
  asBuiltInVibeSkin(rescuePupsAdventureBaySkinManifest),
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

function normalizeTitlebarBlock(value: unknown): VibeSkinTitlebarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinTitlebarBlock = {};
  for (const key of ["title", "subtitle", "badge"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeProfileBlock(value: unknown): VibeSkinProfileBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinProfileBlock = {};
  for (const key of ["name", "status", "signature", "badge", "avatar"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeShowcaseBlock(value: unknown): VibeSkinShowcaseBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinShowcaseBlock = {};
  if (typeof source.enabled === "boolean") {
    block.enabled = source.enabled;
  }
  for (const key of ["title", "subtitle", "body", "badge", "figure", "footer"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeStatusbarBlock(value: unknown): VibeSkinStatusbarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinStatusbarBlock = {};
  for (const key of ["left", "right"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      block[key] = item;
    }
  }
  return Object.keys(block).length > 0 ? block : undefined;
}

const SAFE_TASKBAR_ACTIONS = new Set<VibeSkinTaskbarAction>([
  "openAppearance",
  "setTheme",
  "importSkin",
  "clearSkin",
]);

function normalizeTaskbarMenuItem(value: unknown): VibeSkinTaskbarMenuItem | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  if (source.type === "separator") {
    return { type: "separator" };
  }

  const label = optionalString(source.label);
  if (!label) {
    return undefined;
  }

  if (source.disabled === true) {
    return { label, disabled: true };
  }

  const action = optionalString(source.action);
  if (!action || !SAFE_TASKBAR_ACTIONS.has(action as VibeSkinTaskbarAction)) {
    return undefined;
  }

  const item: VibeSkinTaskbarMenuItem = {
    label,
    action: action as VibeSkinTaskbarAction,
  };
  if (item.action === "setTheme") {
    const theme = optionalString(source.theme);
    if (theme !== "dark" && theme !== "light" && theme !== "skin") {
      return undefined;
    }
    item.theme = theme;
  }
  return item;
}

function normalizeTaskbarButton(value: unknown): VibeSkinTaskbarButton | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const button: VibeSkinTaskbarButton = {};
  const label = optionalString(source.label);
  const icon = optionalString(source.icon);
  if (label) {
    button.label = label;
  }
  if (icon) {
    button.icon = icon;
  }
  return Object.keys(button).length > 0 ? button : undefined;
}

function normalizeTaskbarItems(value: unknown): VibeSkinTaskbarItem[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  const items = value
    .map((item): VibeSkinTaskbarItem | undefined => {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        return undefined;
      }
      const source = item as Record<string, unknown>;
      const label = optionalString(source.label);
      if (!label) {
        return undefined;
      }

      const normalized: VibeSkinTaskbarItem = { label };
      const icon = optionalString(source.icon);
      if (icon) {
        normalized.icon = icon;
      }
      if (typeof source.active === "boolean") {
        normalized.active = source.active;
      }
      return normalized;
    })
    .filter((item): item is VibeSkinTaskbarItem => Boolean(item));
  return items.length > 0 ? items : undefined;
}

function normalizeTaskbarBlock(value: unknown): VibeSkinTaskbarBlock | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const block: VibeSkinTaskbarBlock = {};
  if (typeof source.enabled === "boolean") {
    block.enabled = source.enabled;
  }

  const startButton = normalizeTaskbarButton(source.startButton);
  if (startButton) {
    block.startButton = startButton;
  }

  const startMenuSource =
    source.startMenu && typeof source.startMenu === "object" && !Array.isArray(source.startMenu)
      ? (source.startMenu as Record<string, unknown>)
      : undefined;
  const menuItems = Array.isArray(startMenuSource?.items)
    ? startMenuSource.items
        .map(normalizeTaskbarMenuItem)
        .filter((item): item is VibeSkinTaskbarMenuItem => Boolean(item))
    : undefined;
  if (menuItems && menuItems.length > 0) {
    block.startMenu = { items: menuItems };
  }

  const items = normalizeTaskbarItems(source.items);
  if (items) {
    block.items = items;
  }

  if (Array.isArray(source.tray)) {
    const tray = source.tray
      .map((item) => optionalString(item))
      .filter((item): item is string => Boolean(item));
    if (tray.length > 0) {
      block.tray = tray.slice(0, 6);
    }
  }

  if (source.clockFormat === "HH:mm") {
    block.clockFormat = "HH:mm";
  }

  return Object.keys(block).length > 0 ? block : undefined;
}

function normalizeBlocks(value: unknown): VibeSkinBlocks | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const blocks: VibeSkinBlocks = {
    titlebar: normalizeTitlebarBlock(source.titlebar),
    profile: normalizeProfileBlock(source.profile),
    showcase: normalizeShowcaseBlock(source.showcase),
    statusbar: normalizeStatusbarBlock(source.statusbar),
    taskbar: normalizeTaskbarBlock(source.taskbar),
  };
  return Object.values(blocks).some(Boolean) ? blocks : undefined;
}

const SAFE_DECORATION_VARIANTS = new Set<VibeSkinDecorationVariant>(VIBE_SKIN_DECORATION_VARIANTS);
const SAFE_DECORATION_TEMPLATES = new Set<VibeSkinDecorationTemplate>(
  VIBE_SKIN_DECORATION_TEMPLATES,
);
const SAFE_DECORATION_TONES = new Set<VibeSkinDecorationTone>(VIBE_SKIN_DECORATION_TONES);

function normalizeDecorationVariant(value: unknown): VibeSkinDecorationVariant | undefined {
  const variant = optionalString(value);
  return variant && SAFE_DECORATION_VARIANTS.has(variant as VibeSkinDecorationVariant)
    ? (variant as VibeSkinDecorationVariant)
    : undefined;
}

function normalizeDecorationTemplate(value: unknown): VibeSkinDecorationTemplate | undefined {
  const template = optionalString(value);
  return template && SAFE_DECORATION_TEMPLATES.has(template as VibeSkinDecorationTemplate)
    ? (template as VibeSkinDecorationTemplate)
    : undefined;
}

function normalizeDecorationTone(value: unknown): VibeSkinDecorationTone | undefined {
  const tone = optionalString(value);
  return tone && SAFE_DECORATION_TONES.has(tone as VibeSkinDecorationTone)
    ? (tone as VibeSkinDecorationTone)
    : undefined;
}

function normalizeDecorationItem(value: unknown): VibeSkinDecorationItem | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const label = optionalString(source.label);
  if (!label) {
    return undefined;
  }

  const item: VibeSkinDecorationItem = { label };
  const badge = optionalString(source.badge);
  const image = optionalString(source.image);
  const template = normalizeDecorationTemplate(source.template);
  const tone = normalizeDecorationTone(source.tone);

  if (badge) {
    item.badge = badge;
  }
  if (image) {
    item.image = image;
  }
  if (template) {
    item.template = template;
  }
  if (tone) {
    item.tone = tone;
  }

  return item;
}

function normalizeDecorationCard(value: unknown): VibeSkinDecorationCard | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const card: VibeSkinDecorationCard = {};
  for (const key of ["title", "subtitle", "badge", "footer", "figure"] as const) {
    const item = optionalString(source[key]);
    if (item) {
      card[key] = item;
    }
  }

  const template = normalizeDecorationTemplate(source.template);
  if (template) {
    card.template = template;
  }

  if (Array.isArray(source.items)) {
    const items = source.items
      .map(normalizeDecorationItem)
      .filter((item): item is VibeSkinDecorationItem => Boolean(item))
      .slice(0, 12);
    if (items.length > 0) {
      card.items = items;
    }
  }

  return Object.keys(card).length > 0 ? card : undefined;
}

function normalizeDecorations(value: unknown): VibeSkinDecorations | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }

  const source = value as Record<string, unknown>;
  const decorations: VibeSkinDecorations = {};
  const variant = normalizeDecorationVariant(source.variant);
  const titlebarMark = optionalString(source.titlebarMark);
  const avatarTemplate = normalizeDecorationTemplate(source.avatarTemplate);
  const showcaseTemplate = normalizeDecorationTemplate(source.showcaseTemplate);

  if (variant) {
    decorations.variant = variant;
  }
  if (titlebarMark) {
    decorations.titlebarMark = titlebarMark.slice(0, 4);
  }
  if (avatarTemplate) {
    decorations.avatarTemplate = avatarTemplate;
  }
  if (showcaseTemplate) {
    decorations.showcaseTemplate = showcaseTemplate;
  }

  if (Array.isArray(source.rightCards)) {
    const rightCards = source.rightCards
      .map(normalizeDecorationCard)
      .filter((card): card is VibeSkinDecorationCard => Boolean(card))
      .slice(0, 6);
    if (rightCards.length > 0) {
      decorations.rightCards = rightCards;
    }
  }

  return Object.keys(decorations).length > 0 ? decorations : undefined;
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

async function resolveBlockAssets(blocks: VibeSkinBlocks | undefined, resolveAsset?: AssetResolver) {
  if (!blocks) {
    return;
  }

  if (blocks.profile?.avatar) {
    blocks.profile.avatar = await resolveImageReference(
      blocks.profile.avatar,
      "blocks.profile.avatar",
      resolveAsset,
    );
  }

  if (blocks.showcase?.figure) {
    blocks.showcase.figure = await resolveImageReference(
      blocks.showcase.figure,
      "blocks.showcase.figure",
      resolveAsset,
    );
  }

  if (blocks.taskbar?.startButton?.icon) {
    blocks.taskbar.startButton.icon = await resolveImageReference(
      blocks.taskbar.startButton.icon,
      "blocks.taskbar.startButton.icon",
      resolveAsset,
    );
  }

  for (const item of blocks.taskbar?.items ?? []) {
    if (item.icon) {
      item.icon = await resolveImageReference(
        item.icon,
        "blocks.taskbar.items.icon",
        resolveAsset,
      );
    }
  }
}

async function resolveDecorationAssets(
  decorations: VibeSkinDecorations | undefined,
  resolveAsset?: AssetResolver,
) {
  if (!decorations) {
    return;
  }

  for (const card of decorations.rightCards ?? []) {
    if (card.figure) {
      card.figure = await resolveImageReference(
        card.figure,
        "decorations.rightCards.figure",
        resolveAsset,
      );
    }

    for (const item of card.items ?? []) {
      if (item.image) {
        item.image = await resolveImageReference(
          item.image,
          "decorations.rightCards.items.image",
          resolveAsset,
        );
      }
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
  const blocks = normalizeBlocks(raw.blocks);
  const decorations = normalizeDecorations(raw.decorations);

  ui.backgroundImage = await resolveImageReference(
    ui.backgroundImage,
    "ui.backgroundImage",
    resolveAsset,
  );
  await resolveRegionAssets(regions, resolveAsset);
  await resolveBlockAssets(blocks, resolveAsset);
  await resolveDecorationAssets(decorations, resolveAsset);
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
    blocks,
    decorations,
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
    blocks: normalizeBlocks(raw.blocks),
    decorations: normalizeDecorations(raw.decorations),
  };
}

function legacyShowcaseToBlock(
  showcase: VibeSkinShowcase | undefined,
): VibeSkinShowcaseBlock | undefined {
  if (!showcase) {
    return undefined;
  }
  return {
    enabled: showcase.enabled,
    title: showcase.title,
    subtitle: showcase.subtitle,
    body: showcase.body,
    badge: showcase.badge,
    figure: showcase.image,
    footer: showcase.footer,
  };
}

export function getVibeSkinBlocks(skin: VibeSkinDefinition): ResolvedVibeSkinBlocks {
  const showcaseSource = skin.blocks?.showcase ?? legacyShowcaseToBlock(skin.showcase);
  const taskbarSource = skin.blocks?.taskbar;
  return {
    titlebar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.titlebar,
      ...skin.blocks?.titlebar,
    },
    profile: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.profile,
      ...skin.blocks?.profile,
    },
    showcase: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.showcase,
      ...showcaseSource,
      enabled: showcaseSource?.enabled ?? DEFAULT_VIBE_SKIN_BLOCKS.showcase.enabled,
    },
    statusbar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.statusbar,
      ...skin.blocks?.statusbar,
    },
    taskbar: {
      ...DEFAULT_VIBE_SKIN_BLOCKS.taskbar,
      ...taskbarSource,
      enabled: taskbarSource?.enabled ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.enabled,
      startButton: {
        ...DEFAULT_VIBE_SKIN_BLOCKS.taskbar.startButton,
        ...taskbarSource?.startButton,
      },
      startMenu: {
        items: taskbarSource?.startMenu?.items ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.startMenu.items,
      },
      items:
        taskbarSource?.items?.map((item) => ({
          active: false,
          ...item,
          label: item.label ?? "AI Switch 终端",
        })) ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.items,
      tray: taskbarSource?.tray ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.tray,
      clockFormat: taskbarSource?.clockFormat ?? DEFAULT_VIBE_SKIN_BLOCKS.taskbar.clockFormat,
    },
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
