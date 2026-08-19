import { beforeEach, describe, expect, it } from "vitest";
import {
  loadModelTestModels,
  MODEL_TEST_MODELS_STORAGE_KEY,
  poolModelTestKey,
  pruneModelTestModelMap,
  pruneModelTestModels,
  saveModelTestModel,
} from "../../src/lib/modelTestModels";

function seed(value: unknown) {
  window.localStorage.setItem(
    MODEL_TEST_MODELS_STORAGE_KEY,
    typeof value === "string" ? value : JSON.stringify(value),
  );
}

function stored() {
  return JSON.parse(
    window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY) ?? "null",
  );
}

describe("modelTestModels", () => {
  beforeEach(() => window.localStorage.clear());

  it("builds the pool key and defaults to an empty map", () => {
    expect(poolModelTestKey("codex")).toBe("pool:codex");
    expect(poolModelTestKey("claude")).toBe("pool:claude");
    expect(loadModelTestModels()).toEqual({});
  });

  it("reads account and pool entries verbatim", () => {
    seed({
      "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    expect(loadModelTestModels()).toEqual({
      "cred-api-1": { model: "gpt-5.6-sol", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("falls back to an empty map for malformed top-level data", () => {
    seed("not-json");
    expect(loadModelTestModels()).toEqual({});

    seed(42);
    expect(loadModelTestModels()).toEqual({});

    seed(null);
    expect(loadModelTestModels()).toEqual({});

    seed([{ model: "gpt-5", platform: "codex" }]);
    expect(loadModelTestModels()).toEqual({});
  });

  it("skips malformed entries but keeps the valid ones", () => {
    seed({
      "bad-not-object": "gpt-5",
      "bad-no-model": { platform: "codex" },
      "bad-model-type": { model: 7, platform: "codex" },
      "bad-no-platform": { model: "gpt-5" },
      "bad-null": null,
      "good-1": { model: "gpt-5.6-sol", platform: "codex" },
    });

    expect(loadModelTestModels()).toEqual({
      "good-1": { model: "gpt-5.6-sol", platform: "codex" },
    });
  });

  it("writes one entry without disturbing the others", () => {
    seed({
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    saveModelTestModel("cred-a", "gpt-5.6-sol", "codex");

    expect(stored()).toEqual({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("deletes the key when the model name is blank", () => {
    seed({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
    });

    saveModelTestModel("cred-a", "   ", "codex");

    expect(stored()).toEqual({
      "cred-b": { model: "claude-opus-4-8", platform: "claude" },
    });
  });

  it("trims the model name before storing it", () => {
    saveModelTestModel("cred-a", "  gpt-5.6-sol  ", "codex");

    expect(stored()).toEqual({
      "cred-a": { model: "gpt-5.6-sol", platform: "codex" },
    });
  });

  it("prunes orphaned account keys of the given platform only", () => {
    const map = {
      "cred-live": { model: "gpt-5", platform: "codex" as const },
      "cred-gone": { model: "gpt-4o", platform: "codex" as const },
      "cred-other-platform": { model: "claude-opus-4-8", platform: "claude" as const },
      "pool:codex": { model: "gpt-5", platform: "codex" as const },
      "pool:claude": { model: "claude-sonnet-4-5", platform: "claude" as const },
    };

    // cred-gone dropped; other-platform key, both pool keys and the live key stay.
    expect(pruneModelTestModelMap(map, ["cred-live"], "codex")).toEqual({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "cred-other-platform": { model: "claude-opus-4-8", platform: "claude" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
      "pool:claude": { model: "claude-sonnet-4-5", platform: "claude" },
    });
  });

  it("returns the very same map object when nothing is orphaned", () => {
    const map = {
      "cred-live": { model: "gpt-5", platform: "codex" as const },
      "pool:codex": { model: "gpt-5", platform: "codex" as const },
    };

    // Reference equality matters: the screen feeds this straight into setState,
    // and an unchanged reference is what stops a needless re-render.
    expect(pruneModelTestModelMap(map, new Set(["cred-live", "cred-extra"]), "codex")).toBe(map);
  });

  it("writes the pruned map back to storage", () => {
    seed({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "cred-gone": { model: "gpt-4o", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });

    pruneModelTestModels(["cred-live"], "codex");

    expect(stored()).toEqual({
      "cred-live": { model: "gpt-5", platform: "codex" },
      "pool:codex": { model: "gpt-5", platform: "codex" },
    });
  });

  it("never writes anything when there is nothing to prune", () => {
    // No stored data at all: the key must not be created.
    pruneModelTestModels(["cred-live"], "codex");
    expect(window.localStorage.getItem(MODEL_TEST_MODELS_STORAGE_KEY)).toBeNull();

    seed({ "cred-live": { model: "gpt-5", platform: "codex" } });
    pruneModelTestModels(["cred-live"], "codex");
    expect(stored()).toEqual({ "cred-live": { model: "gpt-5", platform: "codex" } });
  });

  it("never throws when storage is unavailable", () => {
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };

    expect(loadModelTestModels(blocked)).toEqual({});
    expect(() => saveModelTestModel("cred-a", "gpt-5", "codex", blocked)).not.toThrow();
    expect(() => pruneModelTestModels(["cred-a"], "codex", blocked)).not.toThrow();
  });
});
