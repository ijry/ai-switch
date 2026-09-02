import { beforeEach, describe, expect, it } from "vitest";
import {
  ACCOUNT_LIST_LAYOUT_STORAGE_KEY,
  loadAccountListLayout,
  saveAccountListLayout,
} from "../../src/lib/accountListLayout";

describe("accountListLayout", () => {
  beforeEach(() => window.localStorage.clear());

  it("defaults to the list layout", () => {
    expect(loadAccountListLayout()).toBe("list");
  });

  it("loads a stored layout and falls back for anything else", () => {
    window.localStorage.setItem(ACCOUNT_LIST_LAYOUT_STORAGE_KEY, "card");
    expect(loadAccountListLayout()).toBe("card");

    window.localStorage.setItem(ACCOUNT_LIST_LAYOUT_STORAGE_KEY, "grid");
    expect(loadAccountListLayout()).toBe("list");
  });

  it("persists selections and tolerates unavailable storage", () => {
    saveAccountListLayout("card");
    expect(window.localStorage.getItem(ACCOUNT_LIST_LAYOUT_STORAGE_KEY)).toBe("card");

    expect(() =>
      loadAccountListLayout({
        getItem: () => {
          throw new Error("blocked");
        },
      }),
    ).not.toThrow();
    expect(() =>
      saveAccountListLayout("list", {
        setItem: () => {
          throw new Error("blocked");
        },
      }),
    ).not.toThrow();
  });
});
