import { describe, expect, it } from "vitest";
import { routeProxyEndpointForPlatform } from "../src/lib/routeProxyEndpoint";

describe("routeProxyEndpointForPlatform", () => {
  it("appends /v1 for the platforms whose clients append a bare endpoint", () => {
    // Codex calls {base}/responses; the other three reach the pool through
    // {base}/chat/completions. Either way the /v1 has to be in the base URL.
    for (const platform of ["codex", "opencode", "openclaw", "hermes"]) {
      expect(routeProxyEndpointForPlatform("http://127.0.0.1:19527", platform)).toBe(
        "http://127.0.0.1:19527/v1",
      );
    }
    expect(routeProxyEndpointForPlatform("https://127.0.0.1:19528", "codex")).toBe(
      "https://127.0.0.1:19528/v1",
    );
  });

  it("does not double the suffix", () => {
    expect(routeProxyEndpointForPlatform("http://127.0.0.1:19527/v1/", "codex")).toBe(
      "http://127.0.0.1:19527/v1",
    );
    expect(routeProxyEndpointForPlatform("http://127.0.0.1:19527/v1", "hermes")).toBe(
      "http://127.0.0.1:19527/v1",
    );
  });

  it("leaves the other platforms bare", () => {
    // Claude / Gemini / Grok clients append their own versioned paths.
    for (const platform of ["claude", "gemini", "grok"]) {
      expect(routeProxyEndpointForPlatform("http://127.0.0.1:19527/", platform)).toBe(
        "http://127.0.0.1:19527",
      );
    }
  });

  it("returns an empty string for a missing address", () => {
    expect(routeProxyEndpointForPlatform("   ", "codex")).toBe("");
  });
});
