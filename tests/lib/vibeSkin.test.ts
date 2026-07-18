import JSZip from "jszip";
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearStoredVibeSkin,
  getVibeSkinBlocks,
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
          sidebarProfile: {
            background: "linear-gradient(#ffffff, #bce7ff)",
            border: "rgba(21, 104, 184, 0.42)",
          },
          showcaseFigure: {
            shadow: "0 18px 30px rgba(10,82,154,0.24)",
          },
          windowButtonClose: {
            background: "linear-gradient(#ff9aa2, #b51f2e)",
          },
        },
        showcase: {
          enabled: true,
          image: "assets/showcase.png",
        },
        blocks: {
          titlebar: {
            title: "自定义终端",
            subtitle: "复古蓝色皮肤",
            badge: "正在运行",
          },
          profile: {
            name: "测试用户",
            status: "在线",
            signature: "正在测试皮肤包",
            badge: "蓝钻",
            avatar: "assets/avatar.png",
          },
          showcase: {
            title: "QQ秀展示",
            subtitle: "Retro Blue",
            body: "右侧展示区来自 blocks.showcase。",
            figure: "assets/figure.png",
            footer: "自定义展示区",
          },
          statusbar: {
            left: "已连接",
            right: "皮肤区域已启用",
          },
        },
      }),
    );
    zip.file("assets/background.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/shell.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/showcase.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/avatar.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/figure.png", new Uint8Array([137, 80, 78, 71]));

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
    expect(skin.blocks?.titlebar?.title).toBe("自定义终端");
    expect(skin.blocks?.profile?.name).toBe("测试用户");
    expect(skin.blocks?.profile?.avatar).toMatch(/^data:image\/png;base64,/);
    expect(skin.blocks?.showcase?.figure).toMatch(/^data:image\/png;base64,/);
    expect(skin.blocks?.statusbar?.left).toBe("已连接");

    const variables = skinToCssVariables(skin) as Record<string, unknown>;
    expect(variables["--vibe-sidebar-profile-background-layer"]).toBe(
      "linear-gradient(#ffffff, #bce7ff)",
    );
    expect(variables["--vibe-showcase-figure-shadow"]).toBe(
      "0 18px 30px rgba(10,82,154,0.24)",
    );
    expect(variables["--vibe-window-button-close-background-layer"]).toBe(
      "linear-gradient(#ff9aa2, #b51f2e)",
    );
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

  it("normalizes stored blocks and maps legacy showcase when blocks showcase is absent", () => {
    writeStoredVibeSkin({
      id: "stored-block-skin",
      name: "Stored Block Skin",
      ui: {
        accent: "#1678d8",
        accentText: "#ffffff",
        background: "#0f6bc4",
        backgroundOverlay: "linear-gradient(rgba(255,255,255,0.3), transparent)",
        panel: "rgba(226,245,255,0.88)",
        panelStrong: "rgba(255,255,255,0.96)",
        panelSubtle: "rgba(188,226,250,0.8)",
        border: "rgba(14,99,181,0.42)",
        text: "#0d315d",
        mutedText: "#3d6d9f",
        button: "#1678d8",
        buttonText: "#ffffff",
        buttonHover: "#2088e5",
        dangerBackground: "#b72434",
        dangerText: "#ffffff",
        tabBar: "rgba(239,250,255,0.94)",
        tabActive: "#ffffff",
        tabInactive: "rgba(151,210,247,0.54)",
        tabHover: "rgba(255,255,255,0.72)",
        focus: "#44a7ff",
      },
      blocks: {
        titlebar: {
          title: "存储终端",
        },
        profile: {
          name: "存储用户",
          status: "在线",
        },
        statusbar: {
          right: "右侧状态",
        },
      },
      showcase: {
        enabled: true,
        title: "旧展示",
        image: "data:image/png;base64,AAAA",
        footer: "旧页脚",
      },
    });

    const skin = readStoredVibeSkin();
    const blocks = skin ? getVibeSkinBlocks(skin) : null;

    expect(skin?.blocks?.titlebar?.title).toBe("存储终端");
    expect(blocks?.titlebar.title).toBe("存储终端");
    expect(blocks?.titlebar.subtitle).toBe("QQ2007 蓝色经典");
    expect(blocks?.profile.name).toBe("存储用户");
    expect(blocks?.showcase.title).toBe("旧展示");
    expect(blocks?.showcase.figure).toBe("data:image/png;base64,AAAA");
    expect(blocks?.showcase.footer).toBe("旧页脚");
    expect(blocks?.statusbar.right).toBe("右侧状态");
  });
});
