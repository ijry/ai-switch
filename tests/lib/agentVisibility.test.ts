import { describe, expect, it } from "vitest";
import {
  AGENT_VISIBILITY_STORAGE_KEY,
  createDefaultAgentVisibility,
  readAgentVisibility,
  resolveVisibleAgentScreen,
  writeAgentVisibility,
} from "../../src/lib/agentVisibility";

describe("agent visibility preferences", () => {
  it("defaults every supported agent to visible and merges stored choices", () => {
    const storage = new Map<string, string>();
    storage.set(AGENT_VISIBILITY_STORAGE_KEY, JSON.stringify({ claude: false, unknown: false }));

    const visibility = readAgentVisibility({
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, value),
    });

    expect(visibility).toEqual({
      codex: true,
      claude: false,
      grok: true,
      gemini: true,
      opencode: true,
      openclaw: true,
      hermes: true,
    });
  });

  it("persists visibility and falls back when the active agent is hidden", () => {
    const storage = new Map<string, string>();
    const visibility = {
      ...createDefaultAgentVisibility(),
      codex: false,
      claude: false,
    };

    writeAgentVisibility(visibility, {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, value),
    });

    expect(JSON.parse(storage.get(AGENT_VISIBILITY_STORAGE_KEY) ?? "{}")).toEqual(visibility);
    expect(resolveVisibleAgentScreen("Claude", visibility)).toBe("Grok");
    expect(resolveVisibleAgentScreen("Settings", visibility)).toBe("Settings");
  });

  it("uses Settings when every agent is hidden", () => {
    const visibility = Object.fromEntries(
      Object.keys(createDefaultAgentVisibility()).map((platform) => [platform, false]),
    ) as ReturnType<typeof createDefaultAgentVisibility>;

    expect(resolveVisibleAgentScreen("Codex", visibility)).toBe("Settings");
  });
});
