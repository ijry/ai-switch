export type AccountDisplayPreferences = {
  showAccountType: boolean;
  showModelList: boolean;
  showRequestStats: boolean;
};

export const ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY = "ai-switch.account-display-preferences";

export const DEFAULT_ACCOUNT_DISPLAY_PREFERENCES: AccountDisplayPreferences = {
  showAccountType: false,
  showModelList: true,
  showRequestStats: true,
};

function isPreferences(value: unknown): value is AccountDisplayPreferences {
  if (!value || typeof value !== "object") {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    typeof record.showAccountType === "boolean" &&
    typeof record.showModelList === "boolean" &&
    typeof record.showRequestStats === "boolean"
  );
}

export function loadAccountDisplayPreferences(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): AccountDisplayPreferences {
  try {
    const raw = storage.getItem(ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
    }
    const parsed: unknown = JSON.parse(raw);
    return isPreferences(parsed) ? parsed : DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
  } catch {
    return DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
  }
}

export function saveAccountDisplayPreferences(
  preferences: AccountDisplayPreferences,
  storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
  try {
    storage.setItem(ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences));
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}
