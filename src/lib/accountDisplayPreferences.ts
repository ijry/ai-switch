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

/**
 * Reads each toggle on its own, falling back per field.
 *
 * The whole-object check this replaced meant a stored record missing any one key
 * reset all three. That is not a hypothetical: the next toggle added here ships
 * with every existing user's saved record lacking it, so an all-or-nothing check
 * silently wipes choices on upgrade. Unknown keys are dropped rather than
 * preserved, so a stale key from a removed toggle cannot come back.
 */
function readPreferences(value: unknown): AccountDisplayPreferences {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
  }
  const record = value as Record<string, unknown>;
  const read = (key: keyof AccountDisplayPreferences) =>
    typeof record[key] === "boolean"
      ? (record[key] as boolean)
      : DEFAULT_ACCOUNT_DISPLAY_PREFERENCES[key];
  return {
    showAccountType: read("showAccountType"),
    showModelList: read("showModelList"),
    showRequestStats: read("showRequestStats"),
  };
}

/**
 * Resolves the store lazily so a context without `localStorage` — a sandboxed
 * iframe, or a browser with site data blocked — cannot throw. A default
 * parameter would be evaluated before the `try` below and escape it, and this
 * runs inside a `useState` initializer during render, where an exception is a
 * blank screen rather than a caught error.
 */
function resolveStorage<T extends keyof Storage>(
  storage: Pick<Storage, T> | undefined,
): Pick<Storage, T> | null {
  if (storage) {
    return storage;
  }
  try {
    return typeof window === "undefined" ? null : (window.localStorage as Pick<Storage, T>);
  } catch {
    return null;
  }
}

export function loadAccountDisplayPreferences(
  storage?: Pick<Storage, "getItem">,
): AccountDisplayPreferences {
  try {
    const store = resolveStorage(storage);
    const raw = store?.getItem(ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
    }
    return readPreferences(JSON.parse(raw) as unknown);
  } catch {
    return DEFAULT_ACCOUNT_DISPLAY_PREFERENCES;
  }
}

export function saveAccountDisplayPreferences(
  preferences: AccountDisplayPreferences,
  storage?: Pick<Storage, "setItem">,
): void {
  try {
    resolveStorage(storage)?.setItem(
      ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY,
      JSON.stringify(preferences),
    );
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}
