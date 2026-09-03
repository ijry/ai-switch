import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getSessionMessages, listSessions, openSessionTerminal } from "../src/lib/api/client";
import { I18nProvider } from "../src/lib/i18n";
import type { SessionMessage, SessionMeta } from "../src/lib/api/types";
import { createQueryClient } from "../src/lib/query/queryClient";
import { SessionsScreen } from "../src/screens/SessionsScreen";
import { isDesktop } from "../src/lib/transport";

vi.mock("../src/lib/api/client", () => ({
  getSessionMessages: vi.fn(),
  listSessions: vi.fn(),
  openSessionTerminal: vi.fn(),
}));

vi.mock("../src/lib/transport", () => ({
  isDesktop: vi.fn(() => true),
}));

const session: SessionMeta = {
  providerId: "codex",
  sessionId: "session-1",
  title: "布局修复会话",
  projectDir: "D:/repo/ai-switch",
  createdAt: 1_700_000_000,
  lastActiveAt: 1_700_000_100,
  sourcePath: "D:/repo/ai-switch/session.jsonl",
  resumeCommand: "codex resume session-1",
};

const messages: SessionMessage[] = [
  { role: "user", content: "请检查会话布局", ts: 1_700_000_000 },
  { role: "assistant", content: "已调整为左右双栏", ts: 1_700_000_010 },
];

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <QueryClientProvider client={createQueryClient()}>
        <SessionsScreen />
      </QueryClientProvider>
    </I18nProvider>,
  );
}

describe("SessionsScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(listSessions).mockReset().mockResolvedValue([session]);
    vi.mocked(getSessionMessages).mockReset().mockResolvedValue(messages);
    vi.mocked(openSessionTerminal).mockReset().mockResolvedValue(undefined);
    vi.mocked(isDesktop).mockReturnValue(true);
  });

  it("uses a desktop split pane and keeps quick navigation collapsed by default", async () => {
    renderScreen();

    const heading = await screen.findByRole("heading", { name: "会话管理" });
    const layout = heading.closest("section");
    expect(layout).toHaveClass("md:grid-cols-[minmax(360px,0.9fr)_minmax(0,1.35fr)]");
    expect(await screen.findByText("已调整为左右双栏")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "按智能体筛选" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "分组" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "平铺" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: /D:\/repo\/ai-switch/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: /布局修复会话/ })).not.toBeInTheDocument();
    expect(screen.queryByText("D:/repo/ai-switch/session.jsonl")).not.toBeInTheDocument();
    expect(screen.queryByText("codex resume session-1")).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "快速导航" })).not.toBeInTheDocument();
  });

  it("opens the detail view on mobile and returns to the session list", async () => {
    renderScreen();

    const layout = (await screen.findByRole("heading", { name: "会话管理" })).closest("section");
    const listPane = layout?.children[0];
    const detailPane = layout?.children[1];
    await userEvent.click(await screen.findByRole("button", { name: /D:\/repo\/ai-switch/ }));
    const sessionItem = await screen.findByRole("button", { name: /布局修复会话/ });

    expect(sessionItem).not.toHaveTextContent("D:/repo/ai-switch/session.jsonl");
    expect(sessionItem).not.toHaveTextContent("codex resume session-1");
    expect(listPane).not.toHaveClass("hidden");
    expect(detailPane).toHaveClass("hidden");

    await userEvent.click(sessionItem);
    expect(listPane).toHaveClass("hidden");
    expect(detailPane).not.toHaveClass("hidden");

    await userEvent.click(screen.getByRole("button", { name: "返回会话列表" }));
    expect(listPane).not.toHaveClass("hidden");
    expect(detailPane).toHaveClass("hidden");
  });

  it("opens quick navigation from its button and closes it with Escape", async () => {
    renderScreen();

    const navigationButton = await screen.findByRole("button", { name: "快速导航" });
    expect(navigationButton).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(navigationButton);
    expect(navigationButton).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("dialog", { name: "快速导航" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /请检查会话布局/ })).toHaveAttribute(
      "href",
      "#session-message-0",
    );

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "快速导航" })).not.toBeInTheDocument());
  });

  it("keeps provider and directory details hidden and groups copy actions in the menu", async () => {
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: /D:\/repo\/ai-switch/ }));
    await userEvent.click(await screen.findByRole("button", { name: /布局修复会话/ }));

    expect(screen.queryByText("供应商")).not.toBeInTheDocument();
    expect(screen.queryByText("项目")).not.toBeInTheDocument();
    expect(screen.queryByText("session-1")).not.toBeInTheDocument();
    expect(screen.getByText(/更新时间/)).toBeInTheDocument();
    const openTerminalButton = screen.getByRole("button", { name: "在系统终端中恢复" });
    expect(openTerminalButton).toBeEnabled();
    expect(screen.queryByRole("menu", { name: "会话操作" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "复制目录" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "会话操作" }));
    expect(screen.getByRole("menu", { name: "会话操作" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "复制目录" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "复制源文件" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "复制恢复命令" })).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu", { name: "会话操作" })).not.toBeInTheDocument());

    await userEvent.click(openTerminalButton);
    expect(openSessionTerminal).toHaveBeenCalledWith({
      cwd: "D:/repo/ai-switch",
      command: "codex resume session-1",
    });
  });

  it("disables system terminal recovery when session launch data is incomplete", async () => {
    vi.mocked(listSessions).mockResolvedValue([
      { ...session, projectDir: null, resumeCommand: null },
    ]);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: /未知目录/ }));
    await userEvent.click(await screen.findByRole("button", { name: /布局修复会话/ }));

    expect(screen.getByRole("button", { name: "在系统终端中恢复" })).toBeDisabled();
    expect(openSessionTerminal).not.toHaveBeenCalled();
  });

  it("disables system terminal recovery in the web runtime", async () => {
    vi.mocked(isDesktop).mockReturnValue(false);
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: /D:\/repo\/ai-switch/ }));
    await userEvent.click(await screen.findByRole("button", { name: /布局修复会话/ }));

    expect(screen.getByRole("button", { name: "在系统终端中恢复" })).toBeDisabled();
    expect(openSessionTerminal).not.toHaveBeenCalled();
  });

  it("reports an unavailable clipboard instead of throwing", async () => {
    // Served over plain HTTP to a non-loopback host the origin is not a secure
    // context, so navigator.clipboard is undefined and the fire-and-forget
    // caller turned that into a silent unhandled rejection.
    const original = navigator.clipboard;
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: /D:\/repo\/ai-switch/ }));
    await userEvent.click(await screen.findByRole("button", { name: /布局修复会话/ }));
    await userEvent.click(screen.getByRole("button", { name: "会话操作" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "复制目录" }));

    expect(await screen.findByText("当前环境无法访问剪切板。")).toBeInTheDocument();
    Object.defineProperty(navigator, "clipboard", { value: original, configurable: true });
  });
});
