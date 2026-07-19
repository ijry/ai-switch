import JSZip from "jszip";
import { beforeEach, describe, expect, it } from "vitest";
import {
  BUILT_IN_VIBE_SKINS,
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
          launchPanel: {
            background: "linear-gradient(#effbff, #b7e7ff)",
            border: "rgba(21,104,184,0.48)",
          },
          composerInput: {
            background: "#ffffff",
            color: "#12375f",
          },
          composerSendButton: {
            background: "linear-gradient(#45d5ff, #0d73cd)",
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
          launch: {
            title: "自定义启动",
            body: "选择智能体与文件夹。",
            placeholder: "输入启动目标",
            sendLabel: "出发",
            folderLabel: "项目舱",
            modelLabel: "模型舱",
            reasoningLabel: "推理舱",
            agentStripLabel: "智能体编队",
            agentStripPrefix: "武器选项",
            agentStripSuffix: "完全权限",
            extraLabel: "模式",
            extraValue: "探索",
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
    expect(skin.blocks?.launch?.title).toBe("自定义启动");

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
    expect(variables["--vibe-launch-panel-background-layer"]).toBe(
      "linear-gradient(#effbff, #b7e7ff)",
    );
    expect(variables["--vibe-composer-input-color"]).toBe("#12375f");
    expect(variables["--vibe-composer-send-button-background-layer"]).toBe(
      "linear-gradient(#45d5ff, #0d73cd)",
    );

    const blocks = getVibeSkinBlocks(skin);
    expect(blocks.launch).toMatchObject({
      title: "自定义启动",
      placeholder: "输入启动目标",
      sendLabel: "出发",
      agentStripPrefix: "武器选项",
      extraValue: "探索",
    });
  });

  it("imports decoration templates and image assets from zip skin packages", async () => {
    const zip = new JSZip();
    zip.file(
      "skin.json",
      JSON.stringify({
        id: "uploaded-rescue",
        name: "Uploaded Rescue",
        ui: {
          accent: "#0b7fec",
          background: "#78d4ff",
        },
        blocks: {
          titlebar: {
            title: "上传救援主题",
            subtitle: "皮肤包",
            badge: "待命",
          },
          profile: {
            name: "莱德队长",
            status: "总部在线",
            signature: "自定义包",
            badge: "队长",
          },
          showcase: {
            enabled: true,
            title: "汪汪队总部",
            subtitle: "上传包",
            body: "来自皮肤文件包。",
            badge: "救援总部",
            footer: "自定义展示",
          },
        },
        decorations: {
          variant: "rescue-pups",
          titlebarMark: "汪汪队超长",
          avatarTemplate: "rescue-rider",
          showcaseTemplate: "rescue-hq",
          unsafeTemplate: "nativeCloseWindow",
          rightCards: [
            {
              template: "rescue-dog-team",
              title: "上传狗狗队",
              badge: "狗狗们",
              figure: "assets/team.png",
              items: [
                { label: "红色救援狗狗", tone: "red", image: "assets/red-dog.png" },
                { label: "非法模板狗狗", template: "nativeCloseWindow", tone: "purple" },
                { template: "rescue-mayor" },
              ],
            },
            {
              template: "rescue-civic",
              title: "上传市政",
              items: [
                { label: "古微市长", template: "rescue-mayor" },
                { label: "咕咕鸡", template: "rescue-chicken", image: "assets/chicken.png" },
              ],
            },
          ],
        },
      }),
    );
    zip.file("assets/team.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/red-dog.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/chicken.png", new Uint8Array([137, 80, 78, 71]));

    const blob = await zip.generateAsync({ type: "blob" });
    const skin = await importVibeSkinPackage(
      new File([blob], "uploaded-rescue.zip", { type: "application/zip" }),
    );

    expect(skin.id).toBe("uploaded-rescue");
    expect(skin.decorations?.variant).toBe("rescue-pups");
    expect(skin.decorations?.titlebarMark).toBe("汪汪队超");
    expect(skin.decorations?.avatarTemplate).toBe("rescue-rider");
    expect(skin.decorations?.showcaseTemplate).toBe("rescue-hq");
    expect(skin.decorations?.rightCards).toHaveLength(2);
    expect(skin.decorations?.rightCards?.[0]?.figure).toMatch(/^data:image\/png;base64,/);
    expect(skin.decorations?.rightCards?.[0]?.items?.[0]?.image).toMatch(
      /^data:image\/png;base64,/,
    );
    expect(skin.decorations?.rightCards?.[0]?.items?.[1]).toEqual({
      label: "非法模板狗狗",
    });
    expect(skin.decorations?.rightCards?.[0]?.items).toHaveLength(2);
    expect(skin.decorations?.rightCards?.[1]?.items?.[1]?.image).toMatch(
      /^data:image\/png;base64,/,
    );
    expect(JSON.stringify(skin.decorations)).not.toContain("nativeCloseWindow");
  });

  it("keeps whitelisted cockpit decoration templates from uploaded skins", async () => {
    const skin = await importVibeSkinPackage(
      new File(
        [
          JSON.stringify({
            id: "uploaded-starship",
            name: "上传星舰",
            ui: {
              accent: "#2ee8ff",
              accentText: "#001018",
              background: "#020617",
              backgroundOverlay: "transparent",
              panel: "rgba(2, 16, 32, 0.7)",
              panelStrong: "rgba(5, 24, 45, 0.9)",
              panelSubtle: "rgba(17, 44, 70, 0.72)",
              border: "rgba(46, 232, 255, 0.36)",
              text: "#e6fbff",
              mutedText: "#8ac9d8",
              button: "#2ee8ff",
              buttonText: "#001018",
              buttonHover: "#7cf6ff",
              dangerBackground: "#f97373",
              dangerText: "#1b0303",
              tabBar: "rgba(2, 16, 32, 0.72)",
              tabActive: "rgba(46, 232, 255, 0.18)",
              tabInactive: "rgba(7, 24, 46, 0.72)",
              tabHover: "rgba(46, 232, 255, 0.12)",
              focus: "#f8c76a",
            },
            terminal: {
              background: "transparent",
              foreground: "#d8fbff",
            },
            decorations: {
              variant: "starship-cockpit",
              avatarTemplate: "space-ai-core",
              showcaseTemplate: "space-ship",
              rightCards: [
                { title: "雷达阵列", template: "space-radar" },
                { title: "舰体模拟", template: "space-ship" },
                { title: "航线星图", template: "space-starmap" },
                {
                  title: "遥测输出",
                  template: "space-telemetry",
                  items: [{ label: "跃迁核心", badge: "稳定" }],
                },
              ],
            },
          }),
        ],
        "uploaded-starship.aiskin",
        { type: "application/json" },
      ),
    );

    expect(skin.decorations?.variant).toBe("starship-cockpit");
    expect(skin.decorations?.avatarTemplate).toBe("space-ai-core");
    expect(skin.decorations?.showcaseTemplate).toBe("space-ship");
    expect(skin.decorations?.rightCards?.map((card) => card.template)).toEqual([
      "space-radar",
      "space-ship",
      "space-starmap",
      "space-telemetry",
    ]);
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

  it("keeps built-in skins as importable standard package manifests", async () => {
    for (const builtInSkin of BUILT_IN_VIBE_SKINS) {
      const skin = await importVibeSkinPackage(
        new File([JSON.stringify(builtInSkin)], `${builtInSkin.id}.aiskin`, {
          type: "application/json",
        }),
      );

      expect(skin.id).toBe(builtInSkin.id);
      expect(skin.name).toBe(builtInSkin.name);
      expect(skin.ui.accent).toBe(builtInSkin.ui.accent);
      expect(skin.terminal?.background).toBe("transparent");
      expect(skin.blocks?.titlebar?.title).toBe(builtInSkin.blocks?.titlebar?.title);
      expect(skin.decorations).toEqual(builtInSkin.decorations);
    }
  });

  it("includes the starship cockpit skin as a standard importable skin package", async () => {
    const builtInSkin = BUILT_IN_VIBE_SKINS.find((skin) => skin.id === "starship-cockpit");

    expect(builtInSkin).toBeTruthy();
    expect(builtInSkin?.name).toBe("星舰驾驶舱");
    expect(builtInSkin?.terminal?.background).toBe("transparent");
    expect(builtInSkin?.decorations?.variant).toBe("starship-cockpit");
    expect(builtInSkin?.decorations?.avatarTemplate).toBe("space-ai-core");
    expect(builtInSkin?.decorations?.showcaseTemplate).toBe("space-ship");
    expect(builtInSkin?.blocks?.showcase?.enabled).toBe(false);
    expect(builtInSkin?.blocks?.showcase?.title).toBe("");
    expect(builtInSkin?.blocks?.showcase?.body).toBe("");
    expect(builtInSkin?.blocks?.launch?.title).toBe("出发下一个星球");
    expect(builtInSkin?.blocks?.launch?.agentStripPrefix).toBe("武器选项");
    expect(builtInSkin?.regions?.composerAddon?.padding).toBe("0.3rem 0.8rem");
    expect(builtInSkin?.regions?.composerAddon?.lineHeight).toBe("1.4");
    expect(builtInSkin?.decorations?.rightCards?.[0]?.title).toBe("");
    expect(builtInSkin?.decorations?.rightCards?.[0]?.status).toBe("近轨目标追踪");
    expect(builtInSkin?.decorations?.rightCards?.[1]?.title).toBe("");
    expect(builtInSkin?.decorations?.rightCards?.[1]?.subtitle).toBe("");
    expect(builtInSkin?.decorations?.rightCards?.[1]?.status).toBe("");
    expect(builtInSkin?.decorations?.rightCards?.[2]?.title).toBe("");
    expect(builtInSkin?.decorations?.rightCards?.[2]?.status).toBe("航线星图");
    expect(builtInSkin?.decorations?.rightCards?.map((card) => card.template)).toEqual([
      "space-radar",
      "space-ship",
      "space-starmap",
      "space-telemetry",
    ]);

    const imported = await importVibeSkinPackage(
      new File([JSON.stringify(builtInSkin)], "starship-cockpit.aiskin", {
        type: "application/json",
      }),
    );

    expect(imported.id).toBe("starship-cockpit");
    expect(getVibeSkinBlocks(imported).showcase.title).toBe("");
    expect(getVibeSkinBlocks(imported).showcase.body).toBe("");
    expect(getVibeSkinBlocks(imported).launch.sendLabel).toBe("跃迁启动");
    const importedVariables = skinToCssVariables(imported) as Record<string, unknown>;
    expect(importedVariables["--vibe-composer-addon-padding"]).toBe("0.3rem 0.8rem");
    expect(importedVariables["--vibe-composer-addon-line-height"]).toBe("1.4");
    expect(imported.decorations).toEqual(builtInSkin?.decorations);
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

  it("imports taskbar blocks, taskbar icon assets, and taskbar region variables", async () => {
    const zip = new JSZip();
    zip.file(
      "skin.json",
      JSON.stringify({
        id: "taskbar-skin",
        name: "Taskbar Skin",
        ui: {
          accent: "#1678d8",
          background: "#0f6bc4",
        },
        regions: {
          taskbar: {
            background: "linear-gradient(#4bb5ff, #0d65bd)",
            border: "rgba(5,82,150,0.65)",
          },
          taskbarStartButton: {
            backgroundImage: "assets/start-bg.png",
            borderRadius: "999px",
          },
          taskbarStartMenu: {
            background: "linear-gradient(#ffffff, #c7ecff)",
          },
          taskbarMenuItem: {
            color: "#12375f",
          },
          taskbarItemActive: {
            background: "linear-gradient(#ffffff, #80caff)",
          },
          taskbarClock: {
            color: "#ffffff",
          },
        },
        blocks: {
          taskbar: {
            enabled: true,
            startButton: {
              label: "开始",
              icon: "assets/start.png",
            },
            startMenu: {
              items: [
                { label: "外观设置", action: "openAppearance" },
                { label: "切换亮色主题", action: "setTheme", theme: "light" },
                { label: "导入皮肤...", action: "importSkin" },
                { type: "separator" },
                { label: "禁用项", disabled: true },
                { label: "非法项", action: "launchNativeWindow" },
              ],
            },
            items: [
              { label: "AI Switch 终端", icon: "assets/app.png", active: true },
              { label: "资料卡", active: false },
            ],
            tray: ["Vibe", "在线"],
            clockFormat: "HH:mm",
          },
        },
      }),
    );
    zip.file("assets/start.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/app.png", new Uint8Array([137, 80, 78, 71]));
    zip.file("assets/start-bg.png", new Uint8Array([137, 80, 78, 71]));

    const blob = await zip.generateAsync({ type: "blob" });
    const skin = await importVibeSkinPackage(
      new File([blob], "taskbar.zip", { type: "application/zip" }),
    );
    const blocks = getVibeSkinBlocks(skin);
    const variables = skinToCssVariables(skin) as Record<string, unknown>;

    expect(blocks.taskbar.enabled).toBe(true);
    expect(blocks.taskbar.startButton.label).toBe("开始");
    expect(blocks.taskbar.startButton.icon).toMatch(/^data:image\/png;base64,/);
    expect(blocks.taskbar.startMenu.items).toEqual([
      { label: "外观设置", action: "openAppearance" },
      { label: "切换亮色主题", action: "setTheme", theme: "light" },
      { label: "导入皮肤...", action: "importSkin" },
      { type: "separator" },
      { label: "禁用项", disabled: true },
    ]);
    expect(blocks.taskbar.items[0]).toMatchObject({
      label: "AI Switch 终端",
      active: true,
    });
    expect(blocks.taskbar.items[0]?.icon).toMatch(/^data:image\/png;base64,/);
    expect(blocks.taskbar.items[1]).toEqual({ label: "资料卡", active: false });
    expect(blocks.taskbar.tray).toEqual(["Vibe", "在线"]);
    expect(blocks.taskbar.clockFormat).toBe("HH:mm");
    expect(variables["--vibe-taskbar-background-layer"]).toBe(
      "linear-gradient(#4bb5ff, #0d65bd)",
    );
    expect(variables["--vibe-taskbar-start-button-background-image"]).toMatch(
      /^url\("data:image\/png;base64,/,
    );
    expect(variables["--vibe-taskbar-start-menu-background-layer"]).toBe(
      "linear-gradient(#ffffff, #c7ecff)",
    );
    expect(variables["--vibe-taskbar-clock-color"]).toBe("#ffffff");
  });

  it("resolves taskbar defaults and preserves custom disabled taskbars", () => {
    const baseUi = {
      accent: "#1678d8",
      accentText: "#ffffff",
      background: "#0f6bc4",
      backgroundOverlay: "transparent",
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
    };
    const builtIn = getVibeSkinBlocks({
      id: "minimal",
      name: "Minimal",
      ui: baseUi,
    });
    const disabled = getVibeSkinBlocks({
      id: "disabled",
      name: "Disabled",
      ui: baseUi,
      blocks: {
        taskbar: {
          enabled: false,
        },
      },
    });

    expect(builtIn.taskbar.enabled).toBe(true);
    expect(builtIn.taskbar.startButton.label).toBe("开始");
    expect(builtIn.taskbar.startMenu.items).toContainEqual({
      label: "外观设置",
      action: "openAppearance",
    });
    expect(builtIn.taskbar.startMenu.items).toContainEqual({
      label: "切换暗色主题",
      action: "setTheme",
      theme: "dark",
    });
    expect(builtIn.taskbar.items).toContainEqual({
      label: "AI Switch 终端",
      active: true,
    });
    expect(builtIn.taskbar.tray).toEqual(["Vibe", "在线"]);
    expect(disabled.taskbar.enabled).toBe(false);
    expect(disabled.taskbar.startButton.label).toBe("开始");
  });
});
