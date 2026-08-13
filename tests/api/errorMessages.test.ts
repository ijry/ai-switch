import { describe, expect, it } from "vitest";
import { apiErrorMessageKey } from "../../src/lib/api/errorMessages";

describe("apiErrorMessageKey", () => {
  it("maps MCP and Skills codes to stable translation keys", () => {
    expect(apiErrorMessageKey("mcp.config_invalid")).toBe("errors.mcp.configInvalid");
    expect(apiErrorMessageKey("skills.read_only")).toBe("errors.skills.readOnly");
  });

  it("falls back for unknown codes", () => {
    expect(apiErrorMessageKey("future.unknown")).toBe("errors.operationFailed");
  });
});
