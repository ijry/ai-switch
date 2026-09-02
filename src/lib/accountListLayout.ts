export type AccountListLayout = "list" | "card";

export const ACCOUNT_LIST_LAYOUT_STORAGE_KEY = "ai-switch.account-list-layout";

const DEFAULT_ACCOUNT_LIST_LAYOUT: AccountListLayout = "list";

function isAccountListLayout(value: string | null): value is AccountListLayout {
  return value === "list" || value === "card";
}

export function loadAccountListLayout(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): AccountListLayout {
  try {
    const stored = storage.getItem(ACCOUNT_LIST_LAYOUT_STORAGE_KEY);
    return isAccountListLayout(stored) ? stored : DEFAULT_ACCOUNT_LIST_LAYOUT;
  } catch {
    return DEFAULT_ACCOUNT_LIST_LAYOUT;
  }
}

export function saveAccountListLayout(
  layout: AccountListLayout,
  storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
  try {
    storage.setItem(ACCOUNT_LIST_LAYOUT_STORAGE_KEY, layout);
  } catch {
    // Storage can be unavailable in restricted browser contexts.
  }
}
