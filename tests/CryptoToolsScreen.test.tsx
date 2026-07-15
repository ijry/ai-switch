import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { CryptoToolsScreen } from "../src/screens/CryptoToolsScreen";

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <CryptoToolsScreen />
    </I18nProvider>,
  );
}

describe("CryptoToolsScreen", () => {
  it("decodes Base64 text by default", async () => {
    renderScreen();

    await userEvent.type(screen.getByLabelText("输入文本"), "aGVsbG8g5LiW55WM");
    expect(screen.getByLabelText("输出文本")).toHaveValue("hello 世界");
  });

  it("shows a validation error for invalid hex input", async () => {
    renderScreen();

    await userEvent.selectOptions(screen.getByLabelText("转换方式"), "hex-decode");
    await userEvent.type(screen.getByLabelText("输入文本"), "abc");

    expect(screen.getByText("Hex 内容必须是偶数长度。")).toBeInTheDocument();
    expect(screen.getByLabelText("输出文本")).toHaveValue("");
  });
});
