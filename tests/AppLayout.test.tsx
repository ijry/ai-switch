import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppLayout } from "../src/components/layout/AppLayout";
import { I18nProvider } from "../src/lib/i18n";

function dispatchPointerEvent(
  target: EventTarget,
  type: "pointerdown" | "pointermove" | "pointerup",
  {
    button = 0,
    clientX = 0,
    pointerId = 1,
  }: { button?: number; clientX?: number; pointerId?: number } = {},
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    button: { configurable: true, value: button },
    clientX: { configurable: true, value: clientX },
    clientY: { configurable: true, value: 0 },
    pointerId: { configurable: true, value: pointerId },
  });
  act(() => {
    target.dispatchEvent(event);
  });
}

function setViewportWidth(width: number) {
  act(() => {
    (window as Window & { innerWidth: number }).innerWidth = width;
    window.dispatchEvent(new Event("resize"));
  });
}

describe("AppLayout", () => {
  beforeEach(() => {
    window.localStorage.clear();
    (window as Window & { innerWidth: number }).innerWidth = 1024;
  });

  it("renders system utility nav entries and navigates to their screens", async () => {
    const onNavigate = vi.fn();

    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={onNavigate}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    await userEvent.click(screen.getByRole("button", { name: /MCP/ }));
    await userEvent.click(screen.getByRole("button", { name: /技能/ }));
    await userEvent.click(screen.getByRole("button", { name: /关于/ }));

    expect(onNavigate).toHaveBeenCalledWith("MCP");
    expect(onNavigate).toHaveBeenCalledWith("Skills");
    expect(onNavigate).toHaveBeenCalledWith("About");
  });

  it("highlights About on its own instead of the Settings area", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="About"
          onNavigate={vi.fn()}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByRole("button", { name: /关于/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: /设置/ })).not.toHaveAttribute("aria-current");
  });

  it("only highlights the active system utility entry", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="MCP"
          onNavigate={vi.fn()}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByRole("button", { name: /MCP/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: /设置/ })).not.toHaveAttribute("aria-current");
  });

  it("exposes the sidebar toggle state and compact desktop grid", () => {
    const onToggleSidebar = vi.fn();
    const { rerender } = render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={vi.fn()}
          onToggleSidebar={onToggleSidebar}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    const collapseButton = screen.getByRole("button", { name: "收起侧栏" });
    expect(collapseButton).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("app-shell")).toHaveClass("grid-cols-[56px_minmax(0,1fr)]");
    fireEvent.click(collapseButton);
    expect(onToggleSidebar).toHaveBeenCalledTimes(1);

    rerender(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={vi.fn()}
          onToggleSidebar={onToggleSidebar}
          sidebarCollapsed
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByRole("button", { name: "展开侧栏" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("app-shell")).toHaveClass("min-[600px]:grid-cols-[56px_minmax(0,1fr)]");
    expect(screen.getByRole("button", { name: "Codex" })).toHaveAttribute("title", "Codex");
    expect(screen.getByRole("button", { name: "Codex" }).querySelector("svg")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Codex" }).querySelector("span.font-medium")).toHaveClass("sr-only");
    expect(screen.getByRole("combobox", { name: "语言" }).parentElement).toHaveClass("hidden");
    expect(screen.getByText("智能体")).toHaveClass("hidden");
  });

  it("opens the full sidebar as a fixed drawer on narrow windows", async () => {
    setViewportWidth(500);
    const onNavigate = vi.fn();
    const onToggleSidebar = vi.fn();

    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={onNavigate}
          onToggleSidebar={onToggleSidebar}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    const sidebar = screen.getByTestId("app-sidebar");
    const expand = screen.getByRole("button", { name: "展开侧栏" });
    expect(expand).toHaveAttribute("aria-expanded", "false");
    expect(sidebar).not.toHaveClass("app-sidebar-drawer");
    expect(screen.queryByTestId("app-sidebar-drawer-backdrop")).not.toBeInTheDocument();

    await userEvent.click(expand);

    expect(onToggleSidebar).not.toHaveBeenCalled();
    expect(sidebar).toHaveClass("app-sidebar-drawer");
    expect(screen.getByTestId("app-sidebar-drawer-backdrop")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "收起侧栏" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByText("智能体")).not.toHaveClass("hidden");
    expect(screen.getByText("content").parentElement).toHaveClass("col-start-2");

    await userEvent.click(screen.getByTestId("app-sidebar-drawer-backdrop"));
    expect(sidebar).not.toHaveClass("app-sidebar-drawer");

    await userEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    await userEvent.keyboard("{Escape}");
    expect(sidebar).not.toHaveClass("app-sidebar-drawer");

    await userEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    await userEvent.click(screen.getByRole("button", { name: /MCP/ }));
    expect(onNavigate).toHaveBeenCalledWith("MCP");
    expect(sidebar).not.toHaveClass("app-sidebar-drawer");
  });

  it("defaults to a compact expanded width and resizes within bounds", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={vi.fn()}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    const shell = screen.getByTestId("app-shell");
    const handle = screen.getByTestId("sidebar-resize-handle");
    expect(shell.style.getPropertyValue("--app-sidebar-width")).toBe("216px");
    expect(handle).toHaveAttribute("role", "separator");
    expect(handle).toHaveAttribute("aria-valuenow", "216");

    dispatchPointerEvent(handle, "pointerdown", { clientX: 216, pointerId: 11 });
    dispatchPointerEvent(document, "pointermove", { clientX: 260, pointerId: 11 });
    expect(shell.style.getPropertyValue("--app-sidebar-width")).toBe("260px");

    dispatchPointerEvent(document, "pointermove", { clientX: 20, pointerId: 11 });
    expect(shell.style.getPropertyValue("--app-sidebar-width")).toBe("180px");

    dispatchPointerEvent(document, "pointermove", { clientX: 500, pointerId: 11 });
    expect(shell.style.getPropertyValue("--app-sidebar-width")).toBe("320px");
    dispatchPointerEvent(document, "pointerup", { pointerId: 11 });
  });

  it("restores a persisted width and hides the resize handle when collapsed", () => {
    window.localStorage.setItem("ai-switch.sidebar-width", "245");
    const { rerender } = render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={vi.fn()}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed={false}
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByTestId("app-shell").style.getPropertyValue("--app-sidebar-width")).toBe("245px");
    expect(screen.getByTestId("sidebar-resize-handle")).toHaveAttribute("aria-valuenow", "245");

    rerender(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout
          activeScreen="Codex"
          onNavigate={vi.fn()}
          onToggleSidebar={vi.fn()}
          sidebarCollapsed
        >
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByTestId("app-shell").style.getPropertyValue("--app-sidebar-width")).toBe("56px");
    expect(screen.queryByTestId("sidebar-resize-handle")).not.toBeInTheDocument();
  });
});
