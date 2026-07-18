import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  createTerminalSession,
  killTerminalSession,
  listSessions,
} from "../src/lib/api/client";
import type { SessionMeta, TerminalSession } from "../src/lib/api/types";
import { createQueryClient } from "../src/lib/query/queryClient";
import { __resetTransportForTests } from "../src/lib/transport";
import { VIBE_SKIN_STORAGE_KEY } from "../src/lib/vibeSkin";
import { VibeScreen } from "../src/screens/VibeScreen";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/components/terminal/XtermPane", () => ({
  XtermPane: ({
    session,
    themeOverride,
    transparentSurface,
  }: {
    session: TerminalSession;
    themeOverride?: unknown;
    transparentSurface?: boolean;
  }) => (
    <div
      data-testid={`terminal-pane-${session.id}`}
      data-theme-override={themeOverride ? "yes" : "no"}
      data-transparent-surface={transparentSurface ? "yes" : "no"}
    >
      {session.title}
    </div>
  ),
}));

vi.mock("../src/screens/AccountsScreen", () => ({
  AccountsScreen: () => <div>Agent accounts placeholder</div>,
}));

vi.mock("../src/lib/api/client", () => ({
  createTerminalSession: vi.fn(),
  killTerminalSession: vi.fn(),
  listSessions: vi.fn(),
}));

const sessions: SessionMeta[] = [
  {
    providerId: "codex",
    sessionId: "s1",
    title: "Fix terminal bug",
    projectDir: "D:/repo/app",
    createdAt: 1,
    lastActiveAt: 2,
    sourcePath: "D:/repo/app/session.jsonl",
    resumeCommand: "codex resume s1",
  },
  {
    providerId: "claude",
    sessionId: "s2",
    title: "Missing resume",
    projectDir: "D:/repo/app",
    createdAt: 1,
    lastActiveAt: 2,
    sourcePath: "D:/repo/app/session-2.jsonl",
    resumeCommand: null,
  },
];

function renderScreen() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <VibeScreen />
    </QueryClientProvider>,
  );
}

async function expandProjectDirectory() {
  await userEvent.click(await screen.findByRole("button", { name: "Expand folder D:/repo/app" }));
}

async function openAppearanceDialog() {
  await userEvent.click(await screen.findByRole("button", { name: "Switch Vibe theme" }));
  return screen.findByRole("dialog", { name: "Appearance" });
}

async function switchThemeFromAppearance(theme: "Solarized Dark" | "Light" | "Skin") {
  await openAppearanceDialog();
  await userEvent.click(await screen.findByRole("button", { name: theme }));
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
}

async function switchToSkinTheme() {
  await switchThemeFromAppearance("Skin");
}

describe("VibeScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    __resetTransportForTests();
    vi.mocked(createTerminalSession).mockReset();
    vi.mocked(killTerminalSession).mockReset();
    vi.mocked(listSessions).mockReset();
    vi.mocked(listSessions).mockResolvedValue(sessions);
    vi.mocked(createTerminalSession).mockResolvedValue({
      id: "term-1",
      title: "Fix terminal bug",
      platform: "codex",
      cwd: "D:/repo/app",
      command: "codex resume s1",
      status: "running",
      createdAt: 123,
    });
    vi.mocked(killTerminalSession).mockResolvedValue(undefined);
  });

  it("groups local sessions by project directory", async () => {
    renderScreen();

    expect(await screen.findByText("repo/app")).toBeInTheDocument();
    expect(screen.queryByText("D:/repo/app")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand folder D:/repo/app" })).toHaveAttribute(
      "title",
      "D:/repo/app",
    );
    expect(screen.queryByText("Fix terminal bug")).not.toBeInTheDocument();
    expect(screen.queryByText("Missing resume")).not.toBeInTheDocument();

    await expandProjectDirectory();

    expect(screen.getByText("Fix terminal bug")).toBeInTheDocument();
    expect(screen.getByText("Missing resume")).toBeInTheDocument();
  });

  it("launches a resume terminal from a complete session", async () => {
    renderScreen();

    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "resume",
        platform: "codex",
        command: "codex resume s1",
        title: "Fix terminal bug",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
      }),
    );
    expect(await screen.findByTestId("terminal-pane-term-1")).toBeInTheDocument();
  });

  it("does not launch a session without resume metadata", async () => {
    renderScreen();

    await expandProjectDirectory();
    const disabled = await screen.findByRole("button", {
      name: /Cannot resume Missing resume/,
    });
    expect(disabled).toBeDisabled();
    expect(createTerminalSession).not.toHaveBeenCalled();
  });

  it("opens Vibe from the app navigation", async () => {
    render(<App />);

    await userEvent.click(screen.getByRole("button", { name: "Switch to Vibe mode" }));

    expect(
      await screen.findByRole("heading", { name: "Terminal workspace · Vibe mode" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Agent accounts placeholder")).not.toBeInTheDocument();
  });

  it("opens appearance settings and switches Vibe themes from the dialog", async () => {
    renderScreen();

    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await openAppearanceDialog();
    expect(screen.getByRole("button", { name: "Solarized Dark" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Light" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    await userEvent.click(screen.getByRole("button", { name: "Light" }));

    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Light");
    expect(screen.getByRole("button", { name: "Expand folder D:/repo/app" })).toHaveClass("text-stone-800");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("bg-white/85");
    expect(screen.getByText("Start or resume a session")).toHaveClass("text-stone-900");

    await userEvent.click(screen.getByRole("button", { name: "Skin" }));

    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Skin");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");
    expect(screen.getByText("No terminal tabs yet.").parentElement).toHaveClass("vibe-skin-tabbar");
  });

  it("renders built-in QQ2007 skin blocks with Chinese decorative UI", async () => {
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getAllByText("AI Switch 终端").length).toBeGreaterThan(0);
    expect(screen.getByText("QQ2007 蓝色经典")).toBeInTheDocument();
    expect(screen.getAllByText("皮肤模式").length).toBeGreaterThan(0);
    expect(screen.getAllByText("在线").length).toBeGreaterThan(0);
    expect(screen.getByText("正在使用 Vibe 终端")).toBeInTheDocument();
    expect(screen.getByText("经典蓝钻")).toBeInTheDocument();
    expect(screen.getByText("QQ秀展示")).toBeInTheDocument();
    expect(screen.getByText("我的QQ秀")).toBeInTheDocument();
    expect(screen.getByText("自定义展示区")).toBeInTheDocument();
    expect(screen.getByText("AI Switch 已连接")).toBeInTheDocument();
    expect(screen.getByText("皮肤区域已启用")).toBeInTheDocument();

    const controls = screen.getByTestId("vibe-window-controls");
    expect(controls).toHaveAttribute("aria-hidden", "true");
    expect(controls).toHaveTextContent("—");
    expect(controls).toHaveTextContent("□");
    expect(controls).toHaveTextContent("×");
    expect(
      screen.queryByRole("button", { name: /minimize|maximize|close window/i }),
    ).not.toBeInTheDocument();
  });

  it("does not render QQ2007 decorative skin blocks in dark or light themes", async () => {
    renderScreen();

    expect(await screen.findByText("repo/app")).toBeInTheDocument();
    expect(screen.queryByText("QQ秀展示")).not.toBeInTheDocument();
    expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();

    await switchThemeFromAppearance("Light");

    expect(screen.queryByText("QQ秀展示")).not.toBeInTheDocument();
    expect(screen.queryByTestId("vibe-window-controls")).not.toBeInTheDocument();
  });

  it("renders custom skin showcase regions", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "showcase-skin",
        name: "Showcase Skin",
        ui: {
          accent: "#00ffee",
          background: "#001018",
          panel: "rgba(2,28,40,0.78)",
          panelStrong: "rgba(4,42,58,0.92)",
          panelSubtle: "rgba(0,255,238,0.12)",
          border: "rgba(0,255,238,0.35)",
          text: "#f4fbff",
          mutedText: "#9be7ff",
          button: "#00ffee",
          buttonText: "#001018",
          buttonHover: "#54fff5",
        },
        regions: {
          rightRail: { background: "#123456" },
          terminalShell: { background: "#010203" },
          showcaseStage: { background: "#102030" },
        },
        blocks: {
          titlebar: {
            title: "霓虹终端",
            subtitle: "自定义标题栏",
            badge: "自定义皮肤",
          },
          profile: {
            name: "霓虹用户",
            status: "忙碌",
            signature: "正在调试右侧展示区",
            badge: "VIP",
            avatar: "data:image/png;base64,AAAA",
          },
          showcase: {
            title: "右侧QQ秀",
            subtitle: "Neon Figure",
            body: "blocks.showcase 控制展示内容。",
            badge: "Custom Rail",
            figure: "data:image/png;base64,BBBB",
            footer: "region keys",
          },
          statusbar: {
            left: "霓虹已连接",
            right: "状态栏右侧",
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    await openAppearanceDialog();
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("showcase-skin");
    expect(screen.getByText("霓虹终端")).toBeInTheDocument();
    expect(screen.getByText("霓虹用户")).toBeInTheDocument();
    expect(screen.getByText("忙碌")).toBeInTheDocument();
    expect(screen.getByText("右侧QQ秀")).toBeInTheDocument();
    expect(screen.getByText("Custom Rail")).toBeInTheDocument();
    expect(screen.getByText("terminalShell")).toBeInTheDocument();
    expect(screen.getByText("rightRail")).toBeInTheDocument();
    expect(screen.getByText("showcaseStage")).toBeInTheDocument();
    expect(screen.getByText("region keys")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "霓虹用户 avatar" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AAAA",
    );
    expect(screen.getByRole("img", { name: "右侧QQ秀 figure" })).toHaveAttribute(
      "src",
      "data:image/png;base64,BBBB",
    );
  });

  it("renders legacy showcase content when blocks.showcase is absent", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "legacy-showcase-skin",
        name: "Legacy Showcase Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        showcase: {
          enabled: true,
          title: "旧版展示标题",
          badge: "旧版徽标",
          image: "data:image/png;base64,CCCC",
          footer: "旧版页脚",
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByText("旧版展示标题")).toBeInTheDocument();
    expect(screen.getByText("旧版徽标")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "旧版展示标题 figure" })).toHaveAttribute(
      "src",
      "data:image/png;base64,CCCC",
    );
    expect(screen.getByText("旧版页脚")).toBeInTheDocument();
  });

  it("imports a custom Vibe skin package and applies its terminal theme", async () => {
    renderScreen();

    const skinFile = new File(
      [
        JSON.stringify({
          id: "custom-neon",
          name: "Custom Neon",
          ui: {
            accent: "#00ffee",
            text: "#f4fbff",
            mutedText: "#9be7ff",
            background: "linear-gradient(135deg, #001018, #06405a)",
            panel: "rgba(2, 28, 40, 0.78)",
            panelStrong: "rgba(4, 42, 58, 0.92)",
            panelSubtle: "rgba(0, 255, 238, 0.12)",
            border: "rgba(0, 255, 238, 0.35)",
            button: "linear-gradient(180deg, #00ffee, #0088ff)",
            buttonHover: "linear-gradient(180deg, #54fff5, #2da4ff)",
            tabActive: "rgba(0, 255, 238, 0.22)",
          },
          terminal: {
            background: "#010203",
            foreground: "#eafcff",
          },
        }),
      ],
      "custom.aiskin",
      { type: "application/json" },
    );

    await openAppearanceDialog();
    await userEvent.upload(screen.getByLabelText("Choose Vibe skin package"), skinFile);

    await waitFor(() => expect(screen.getByLabelText("Vibe skin")).toHaveValue("custom-neon"));
    expect(screen.getByRole("button", { name: "Switch Vibe theme" })).toHaveTextContent("Skin");
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toContain("Custom Neon");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await expandProjectDirectory();
    await userEvent.click(await screen.findByRole("button", { name: /Resume Fix terminal bug/ }));

    expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
      "data-theme-override",
      "yes",
    );
    expect(await screen.findByTestId("terminal-pane-term-1")).toHaveAttribute(
      "data-transparent-surface",
      "yes",
    );
  });

  it("renders the skin taskbar and replaces the old skin status bar when enabled", async () => {
    renderScreen();

    await switchToSkinTheme();

    expect(screen.getByRole("button", { name: "开始" })).toBeInTheDocument();
    expect(screen.getAllByText("AI Switch 终端").length).toBeGreaterThan(0);
    expect(screen.getByText("AI Switch 已连接")).toBeInTheDocument();
    expect(screen.getByText("皮肤区域已启用")).toBeInTheDocument();
    expect(screen.getByText("Vibe")).toBeInTheDocument();
    expect(screen.getAllByText("在线").length).toBeGreaterThan(0);
    expect(document.querySelector(".vibe-skin-taskbar")).toBeTruthy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeFalsy();
  });

  it("falls back to the skin status bar when a custom skin disables the taskbar", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "no-taskbar",
        name: "No Taskbar",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            enabled: false,
          },
          statusbar: {
            left: "自定义左侧状态",
            right: "自定义右侧状态",
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(document.querySelector(".vibe-skin-taskbar")).toBeFalsy();
    expect(document.querySelector(".vibe-skin-status-bar")).toBeTruthy();
    expect(screen.getByText("自定义左侧状态")).toBeInTheDocument();
    expect(screen.getByText("自定义右侧状态")).toBeInTheDocument();
  });

  it("opens the taskbar start menu and executes only safe actions", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "menu-skin",
        name: "Menu Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
        blocks: {
          taskbar: {
            startMenu: {
              items: [
                { label: "外观设置", action: "openAppearance" },
                { label: "切换暗色主题", action: "setTheme", theme: "dark" },
                { label: "非法动作", action: "nativeCloseWindow" },
                { type: "separator" },
                { label: "不可点击", disabled: true },
              ],
            },
          },
        },
      }),
    );
    renderScreen();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));

    expect(screen.getByRole("menu", { name: "开始菜单" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "外观设置" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "切换暗色主题" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "非法动作" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "不可点击" })).toBeDisabled();

    await userEvent.click(screen.getByRole("menuitem", { name: "切换暗色主题" }));

    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(screen.getByText("Solarized Dark")).toBeInTheDocument();

    await switchToSkinTheme();
    await userEvent.click(screen.getByRole("button", { name: "开始" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "外观设置" }));

    expect(await screen.findByRole("dialog", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.queryByRole("menu", { name: "开始菜单" })).not.toBeInTheDocument();
  });

  it("keeps skin select, import, and clear controls inside the appearance dialog", async () => {
    window.localStorage.setItem(
      VIBE_SKIN_STORAGE_KEY,
      JSON.stringify({
        id: "stored-popup-skin",
        name: "Stored Popup Skin",
        ui: {
          accent: "#1678d8",
          background: "#dff5ff",
          panel: "rgba(232,247,255,0.78)",
          panelStrong: "rgba(255,255,255,0.92)",
          panelSubtle: "rgba(216,239,255,0.68)",
          border: "rgba(15,99,184,0.34)",
          text: "#0d315d",
          mutedText: "#386b9e",
          button: "#1678d8",
          buttonText: "#ffffff",
          buttonHover: "#0f61ae",
        },
      }),
    );
    renderScreen();

    expect(screen.queryByLabelText("Vibe skin")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Import skin" })).not.toBeInTheDocument();

    await openAppearanceDialog();

    const dialog = screen.getByRole("dialog", { name: "Appearance" });
    expect(dialog).toContainElement(screen.getByLabelText("Vibe skin"));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Import skin" }));
    expect(dialog).toContainElement(screen.getByRole("button", { name: "Clear custom skin" }));

    await userEvent.selectOptions(screen.getByLabelText("Vibe skin"), "codex-2007-blue");
    expect(screen.getByLabelText("Vibe skin")).toHaveValue("codex-2007-blue");

    await userEvent.click(screen.getByRole("button", { name: "Clear custom skin" }));
    expect(window.localStorage.getItem(VIBE_SKIN_STORAGE_KEY)).toBeNull();
  });

  it("creates a new agent session through the modal", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "New session" }));
    await screen.findByRole("dialog", { name: "Create session" });
    await userEvent.selectOptions(screen.getByLabelText("Existing folder"), "D:/repo/app");
    await userEvent.selectOptions(screen.getByLabelText("Agent"), "claude");
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() =>
      expect(createTerminalSession).toHaveBeenCalledWith({
        kind: "agent",
        platform: "claude",
        command: null,
        title: "claude - D:/repo/app",
        cwd: "D:/repo/app",
        cols: 100,
        rows: 30,
      }),
    );
  });
});
