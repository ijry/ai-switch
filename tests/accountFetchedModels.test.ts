import { describe, expect, it } from "vitest";
import {
  normalizeFetchedModels,
  parseFetchedModelsFromConfig,
  writeFetchedModelsToConfig,
} from "../src/lib/accountFetchedModels";

describe("accountFetchedModels", () => {
  it("parses valid models and ignores invalid cached entries", () => {
    const models = parseFetchedModelsFromConfig(
      JSON.stringify({
        fetched_models: [
          { id: " gpt-5 ", owned_by: " openai ", supports_1m: true },
          { id: "" },
          null,
        ],
      }),
    );

    expect(models).toEqual([
      { id: "gpt-5", owned_by: "openai", supports_1m: true },
    ]);
  });

  it("returns an empty list for missing or malformed cache data", () => {
    expect(parseFetchedModelsFromConfig("not-json")).toEqual([]);
    expect(parseFetchedModelsFromConfig(JSON.stringify({ fetched_models: {} }))).toEqual([]);
    expect(normalizeFetchedModels(undefined)).toEqual([]);
  });

  it("replaces fetched models while preserving unrelated config fields", () => {
    expect(
      writeFetchedModelsToConfig(
        { base_url: "https://api.example.com/v1", model_mappings: [] },
        [{ id: " gpt-5 ", owned_by: " openai " }],
      ),
    ).toEqual({
      base_url: "https://api.example.com/v1",
      model_mappings: [],
      fetched_models: [{ id: "gpt-5", owned_by: "openai" }],
    });
  });
});
