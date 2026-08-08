import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("Tauri desktop configuration", () => {
  it("allows frontend HTML5 drag and drop on Windows", () => {
    const configPath = resolve(process.cwd(), "src-tauri/tauri.conf.json");
    const config = JSON.parse(readFileSync(configPath, "utf8")) as {
      app?: { windows?: Array<{ dragDropEnabled?: boolean }> };
    };

    expect(config.app?.windows?.[0]?.dragDropEnabled).toBe(false);
  });
});
