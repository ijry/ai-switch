import { describe, expect, it } from "vitest";
import {
  ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY,
  DEFAULT_ACCOUNT_DISPLAY_PREFERENCES,
  loadAccountDisplayPreferences,
  saveAccountDisplayPreferences,
} from "../../src/lib/accountDisplayPreferences";

describe("account display preferences", () => {
  it("uses the requested defaults when no preference has been saved", () => {
    const storage = { getItem: () => null };
    expect(loadAccountDisplayPreferences(storage)).toEqual({
      showAccountType: false,
      showModelList: true,
      showRequestStats: true,
    });
    expect(DEFAULT_ACCOUNT_DISPLAY_PREFERENCES).toEqual({
      showAccountType: false,
      showModelList: true,
      showRequestStats: true,
    });
  });

  it("loads valid saved preferences and ignores malformed values", () => {
    const storage = {
      getItem: (key: string) => key === ACCOUNT_DISPLAY_PREFERENCES_STORAGE_KEY
        ? '{"showAccountType":true,"showModelList":false,"showRequestStats":true}'
        : null,
    };
    expect(loadAccountDisplayPreferences(storage)).toEqual({
      showAccountType: true,
      showModelList: false,
      showRequestStats: true,
    });
    expect(loadAccountDisplayPreferences({ getItem: () => "not-json" })).toEqual(
      DEFAULT_ACCOUNT_DISPLAY_PREFERENCES,
    );
  });

  it("persists the selected display preferences", () => {
    let saved = "";
    saveAccountDisplayPreferences(
      { showAccountType: true, showModelList: false, showRequestStats: false },
      { setItem: (_key, value) => { saved = value; } },
    );
    expect(JSON.parse(saved)).toEqual({
      showAccountType: true,
      showModelList: false,
      showRequestStats: false,
    });
  });
});
