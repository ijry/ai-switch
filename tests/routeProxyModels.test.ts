import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchRouteProxyModels, routeProxyModelsUrl } from "../src/lib/routeProxyModels";

describe("route proxy model list", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("adds the v1 models path without duplicating an existing v1 prefix", () => {
    expect(routeProxyModelsUrl("http://127.0.0.1:43111")).toBe(
      "http://127.0.0.1:43111/v1/models",
    );
    expect(routeProxyModelsUrl("https://127.0.0.1:43111/v1/")).toBe(
      "https://127.0.0.1:43111/v1/models",
    );
  });

  it("fetches and normalizes the OpenAI-compatible model list", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          object: "list",
          data: [
            { id: "gpt-5.6-sol", owned_by: "ai-switch" },
            { id: "gpt-5.6-terra" },
            { id: "" },
          ],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      fetchRouteProxyModels("http://127.0.0.1:43111", "sk-route", "codex"),
    ).resolves.toEqual([
      { id: "gpt-5.6-sol", owned_by: "ai-switch" },
      { id: "gpt-5.6-terra", owned_by: null },
    ]);
    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:43111/v1/models",
      {
        headers: {
          Authorization: "Bearer sk-route",
          "x-ai-switch-platform": "codex",
        },
      },
    );
  });

  it("fetches Codex models with reasoning levels from the models catalog shape", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          models: [
            {
              slug: "gpt-5.6-sol",
              owned_by: "ai-switch",
              supported_reasoning_levels: [
                { effort: "low", description: "Fast" },
                { effort: "high", description: "Deep" },
              ],
              default_reasoning_level: "low",
            },
          ],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      fetchRouteProxyModels("http://127.0.0.1:43111", "sk-route", "codex"),
    ).resolves.toEqual([
      {
        id: "gpt-5.6-sol",
        owned_by: "ai-switch",
        supported_reasoning_levels: [
          { effort: "low", description: "Fast" },
          { effort: "high", description: "Deep" },
        ],
        default_reasoning_level: "low",
      },
    ]);
  });

  it("reads a declared context window and drops an unusable one", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          data: [
            { id: "declared", context_window: 400000 },
            // A relay that sends 0 means "unknown", not a zero-token window.
            { id: "zero", context_window: 0 },
            { id: "absent" },
          ],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      fetchRouteProxyModels("http://127.0.0.1:43111", "sk-route", "codex"),
    ).resolves.toEqual([
      { id: "declared", owned_by: null, context_window: 400000 },
      { id: "zero", owned_by: null },
      { id: "absent", owned_by: null },
    ]);
  });
});
