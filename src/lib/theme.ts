export type ThemePreference = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "ai-switch.theme";

/**
 * Settings persist `theme` as a free string; anything unexpected falls back to
 * "system" so a corrupted value can never pin the app to the wrong look.
 */
export function normalizeThemePreference(value: string | null | undefined): ThemePreference {
  return value === "dark" || value === "light" ? value : "system";
}

export function resolveSystemPrefersDark(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

/**
 * Applies the preference to the document root by toggling the `dark` class
 * (UnoCSS's class-based dark variant and the global overrides in styles.css
 * both key off it) and mirroring the result into `color-scheme` so native
 * controls, form widgets, and scrollbars follow along.
 */
export function applyThemePreference(preference: ThemePreference): void {
  if (typeof document === "undefined") return;
  const dark = preference === "dark" || (preference === "system" && resolveSystemPrefersDark());
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

export function readStoredThemePreference(): ThemePreference | null {
  if (typeof window === "undefined") return null;
  try {
    return normalizeThemePreference(window.localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return null;
  }
}

export function storeThemePreference(preference: ThemePreference): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // Storage may be unavailable in restricted webviews; the backend setting
    // still wins on the next settings fetch.
  }
}

/**
 * Fires whenever the OS switches between light and dark, so a "system"
 * preference re-resolves without a reload. Returns an unsubscribe function.
 */
export function subscribeSystemTheme(onChange: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => onChange();
  media.addEventListener("change", handler);
  return () => media.removeEventListener("change", handler);
}
