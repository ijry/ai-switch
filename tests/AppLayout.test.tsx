import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppLayout } from "../src/components/layout/AppLayout";
import { I18nProvider } from "../src/lib/i18n";

describe("AppLayout", () => {
  it("renders system utility nav entries and navigates to their screens", async () => {
    const onNavigate = vi.fn();

    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout activeScreen="Codex" onNavigate={onNavigate}>
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    await userEvent.click(screen.getByRole("button", { name: /加解密/ }));
    await userEvent.click(screen.getByRole("button", { name: /OCR识别/ }));

    expect(onNavigate).toHaveBeenCalledWith("CryptoTools");
    expect(onNavigate).toHaveBeenCalledWith("OCR");
  });

  it("only highlights the active system utility entry", () => {
    render(
      <I18nProvider initialLanguage="zh-CN">
        <AppLayout activeScreen="OCR" onNavigate={vi.fn()}>
          <div>content</div>
        </AppLayout>
      </I18nProvider>,
    );

    expect(screen.getByRole("button", { name: /OCR识别/ })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: /设置/ })).not.toHaveAttribute("aria-current");
  });
});
