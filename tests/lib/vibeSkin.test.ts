import JSZip from "jszip";
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearStoredVibeSkin,
  importVibeSkinPackage,
  readStoredVibeSkin,
  skinToCssVariables,
  VIBE_SKIN_STORAGE_KEY,
  writeStoredVibeSkin,
} from "../../src/lib/vibeSkin";

describe("vibeSkin", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("imports zip skin packages with relative background assets", async () => {
    const zip = new JSZip();
    zip.file(
      "skin.json",
      JSON.stringify({
        id: "retro-blue",
        name: "Retro Blue",
        ui: {
          accent: "#1278d8",
          backgroundImage: "assets/background.png",
        },
        terminal: {
          background: "#f4fbff",
          foreground: "#12375f",
        },
        regions: {
          terminalShell: {
            backgroundImage: "assets/shell.png",
            backgroundPosition: "center top",
            borderRadius: "18px",
          },
        },
        showcase: {
          enabled: true,
          image: "assets/showcase.png",
        },
      }),
    );
    zip.file("assets/background.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/shell.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/showcase.png", new Uint8Array([137, 80, 78, 71]));

    const blob = await zip.generateAsync({ type: "blob" });
    const skin = await importVibeSkinPackage(
      new File([blob], "retro-blue.zip", { type: "application/zip" }),
    );

    expect(skin.id).toBe("retro-blue");
    expect(skin.name).toBe("Retro Blue");
    expect(skin.ui.accent).toBe("#1278d8");
    expect(skin.ui.backgroundImage).toMatch(/^data:image\/png;base64,/);
    expect(skin.terminal?.background).toBe("#f4fbff");
    expect(skin.regions?.terminalShell?.backgroundImage).toMatch(/^data:image\/png;base64,/);
    expect(skin.regions?.terminalShell?.backgroundPosition).toBe("center top");
    expect(skin.regions?.terminalShell?.borderRadius).toBe("18px");
    expect(skin.showcase?.image).toMatch(/^data:image\/png;base64,/);
  });

  it("imports plain JSON aiskin manifests", async () => {
    const skin = await importVibeSkinPackage(
      new File(
        [
          JSON.stringify({
            id: "plain-json",
            name: "Plain JSON",
            ui: {
              accent: "#00ffee",
              background: "linear-gradient(135deg, #001018, #06405a)",
            },
          }),
        ],
        "plain-json.aiskin",
        { type: "application/json" },
      ),
    );

    expect(skin.id).toBe("plain-json");
    expect(skin.name).toBe("Plain JSON");
    expect(skin.ui.accent).toBe("#00ffee");
  });

  it("persists custom skins and exposes CSS variables", () => {
    writeStoredVibeSkin({
      id: "stored-skin",
      name: "Stored Skin",
      ui: {
        accent: "#00ffee",
        accentText: "#001018",
        background: "linear-gradient(#001018, #063b5a)",
        backgroundOverlay: "linear-gradient(rgba(255,255,255,0.1), transparent)",
        panel: "rgba(2, 28, 40, 0.78)",
        panelStrong: "rgba(4, 42, 58, 0.92)",
        panelSubtle: "rgba(0, 255, 238, 0.12)",
        border: "rgba(0, 255, 238, 0.35)",
        text: "#f4fbff",
        mutedText: "#9be7ff",
        button: "linear-gradient(180deg, #00ffee, #0088ff)",
        buttonText: "#001018",
        buttonHover: "linear-gradient(180deg, #54fff5, #2da4ff)",
        dangerBackground: "#b91c1c",
        dangerText: "#ffffff",
        tabBar: "rgba(2, 28, 40, 0.72)",
        tabActive: "rgba(0, 255, 238, 0.22)",
        tabInactive: "rgba(2, 28, 40, 0.42)",
        tabHover: "rgba(0, 255, 238, 0.18)",
        focus: "#00ffee",
      },
      regions: {
        controlPanel: {
          background: "linear-gradient(#102030, #405060)",
          shadow: "0 0 20px rgba(0,255,238,0.4)",
        },
        terminalShell: {
          background: "#010203",
          borderRadius: "20px",
        },
      },
      showcase: {
        enabled: true,
        title: "Stored Showcase",
      },
    });

    const skin = readStoredVibeSkin();
    const variables = skin ? (skinToCssVariables(skin) as Record<string, unknown>) : {};

    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toContain("Stored Skin");
    expect(skin?.name).toBe("Stored Skin");
    expect(variables["--vibe-accent"]).toBe("#00ffee");
    expect(variables["--vibe-control-panel-background-layer"]).toBe(
      "linear-gradient(#102030, #405060)",
    );
    expect(variables["--vibe-control-panel-shadow"]).toBe("0 0 20px rgba(0,255,238,0.4)");
    expect(variables["--vibe-terminal-shell-border-radius"]).toBe("20px");
    expect(skin?.showcase?.title).toBe("Stored Showcase");

    clearStoredVibeSkin();

    expect(readStoredVibeSkin()).toBeNull();
  });
});
