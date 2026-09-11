import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { getSettings } from "../../lib/api/client";
import {
  applyThemePreference,
  normalizeThemePreference,
  readStoredThemePreference,
  storeThemePreference,
  subscribeSystemTheme,
  type ThemePreference,
} from "../../lib/theme";

/**
 * Keeps the document theme in sync with the persisted settings. Renders null;
 * it exists so App can react to the shared ["settings"] cache — the moment
 * SettingsScreen saves a new theme, this re-applies it without any extra
 * wiring. Before the first fetch resolves, the last stored preference (cached
 * in localStorage by main.tsx) is kept so startup doesn't flash the wrong look.
 */
export function ThemeSync() {
  const preferenceRef = useRef<ThemePreference>(readStoredThemePreference() ?? "system");
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: getSettings });

  useEffect(() => {
    const settings = settingsQuery.data;
    if (!settings) return;
    const preference = normalizeThemePreference(settings.theme);
    preferenceRef.current = preference;
    storeThemePreference(preference);
    applyThemePreference(preference);
  }, [settingsQuery.data]);

  useEffect(
    () =>
      subscribeSystemTheme(() => {
        applyThemePreference(preferenceRef.current);
      }),
    [],
  );

  return null;
}
