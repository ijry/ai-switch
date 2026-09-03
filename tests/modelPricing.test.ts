import { describe, expect, it } from "vitest";
import { mergeModelPriceRows, parsePriceValue, type ModelPriceConfig } from "../src/lib/modelPricing";

describe("model pricing helpers", () => {
  it("merges proxy models with saved prices and deduplicates ids", () => {
    const rows = mergeModelPriceRows(
      [
        { id: "anthropic/claude-sonnet-4", owned_by: "a" },
        { id: "claude-sonnet-4", owned_by: "b" },
        { id: "gpt-5", owned_by: "openai" },
      ],
      {
        "claude-sonnet-4": {
          display_name: "Sonnet 4",
          input_per_mtok: 3,
          output_per_mtok: 15,
          cache_read_per_mtok: 0.3,
          cache_write_per_mtok: 3.75,
        },
      },
    );

    expect(rows).toEqual([
      {
        model: "claude-sonnet-4",
        display_name: "Sonnet 4",
        input_per_mtok: 3,
        output_per_mtok: 15,
        cache_read_per_mtok: 0.3,
        cache_write_per_mtok: 3.75,
      },
      {
        model: "gpt-5",
        display_name: "",
        input_per_mtok: null,
        output_per_mtok: null,
        cache_read_per_mtok: null,
        cache_write_per_mtok: null,
      },
    ]);
  });

  it("accepts non-negative decimal prices and rejects invalid values", () => {
    expect(parsePriceValue("0.125")).toBe(0.125);
    expect(parsePriceValue("  ")).toBeNull();
    expect(parsePriceValue("-1")).toBeNull();
    expect(parsePriceValue("abc")).toBeNull();
  });

  it("uses saved prices when the saved key includes a vendor prefix", () => {
    const rows = mergeModelPriceRows(
      [{ id: "claude-sonnet-4" }],
      {
        "anthropic/claude-sonnet-4": {
          display_name: "Anthropic Sonnet",
          input_per_mtok: 3,
          output_per_mtok: 15,
        },
      },
    );

    expect(rows[0]).toMatchObject({
      model: "claude-sonnet-4",
      display_name: "Anthropic Sonnet",
      input_per_mtok: 3,
      output_per_mtok: 15,
    });
  });
});
