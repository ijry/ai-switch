import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

function readSource(path: string) {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

describe("Tauri desktop configuration", () => {
  it("allows frontend HTML5 drag and drop on Windows", () => {
    const configPath = resolve(process.cwd(), "src-tauri/tauri.conf.json");
    const config = JSON.parse(readFileSync(configPath, "utf8")) as {
      app?: { windows?: Array<{ dragDropEnabled?: boolean }> };
    };

    expect(config.app?.windows?.[0]?.dragDropEnabled).toBe(false);
  });

  it("declares the autostart packages and capability", () => {
    const packageJson = JSON.parse(
      readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
    ) as { dependencies?: Record<string, string> };
    const cargo = readSource("src-tauri/Cargo.toml");
    const capability = JSON.parse(
      readFileSync(resolve(process.cwd(), "src-tauri/capabilities/default.json"), "utf8"),
    ) as { permissions?: string[] };

    expect(packageJson.dependencies?.["@tauri-apps/plugin-autostart"]).toBeDefined();
    expect(cargo).toMatch(/^tauri-plugin-autostart\s*=\s*"2/m);
    expect(capability.permissions).toContain("autostart:default");
  });

  it("registers the autostart plugin and hidden-launch argument", () => {
    const source = readSource("src-tauri/src/lib.rs");

    expect(source).toContain("tauri_plugin_autostart::Builder::new()");
    expect(source).toContain(".args([AUTOSTART_ARG])");
    expect(source).toContain("is_autostart_launch(std::env::args().skip(1))");
    expect(source).toContain("window.hide()");
  });
});
