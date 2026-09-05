import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { version as packageVersion } from "../package.json";
import {
  OFFICIAL_SITE_URL,
  QQ_GROUP_NAME,
  QQ_GROUP_URL,
} from "../src/components/about/catalog";
import { I18nProvider } from "../src/lib/i18n";
import { openExternal } from "../src/lib/openExternal";
import { AboutScreen } from "../src/screens/AboutScreen";

vi.mock("../src/lib/openExternal", () => ({
  openExternal: vi.fn(async () => {}),
}));

const originalExecCommand = document.execCommand;

function renderAbout() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <AboutScreen />
    </I18nProvider>,
  );
}

function stubClipboard(writeText = vi.fn().mockResolvedValue(undefined)) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  return writeText;
}

describe("AboutScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(openExternal).mockReset();
    vi.mocked(openExternal).mockResolvedValue(undefined);
  });

  afterEach(() => {
    document.execCommand = originalExecCommand;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
  });

  it("shows the shipped version, license, and the official site", () => {
    renderAbout();

    expect(screen.getByRole("heading", { level: 1, name: "AI Switch" })).toBeInTheDocument();
    expect(screen.getByText(`v${packageVersion}`)).toBeInTheDocument();
    expect(screen.getByText(/开源许可.*MIT/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开官网" })).toHaveAttribute(
      "title",
      OFFICIAL_SITE_URL,
    );
    expect(screen.getByText(OFFICIAL_SITE_URL)).toBeInTheDocument();
  });

  it("credits the open source projects it depends on", () => {
    renderAbout();

    expect(screen.getByText("依赖的开源项目")).toBeInTheDocument();
    expect(screen.getByText(/感谢每一位维护者/)).toBeInTheDocument();
    expect(screen.getByText("桌面与前端")).toBeInTheDocument();
    expect(screen.getByText("Rust 核心与服务端")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开Tauri" })).toHaveAttribute(
      "title",
      "https://tauri.app",
    );
    expect(screen.getByRole("button", { name: "打开codeg" })).toBeInTheDocument();
  });

  it("lists the friendly links with their addresses", async () => {
    renderAbout();

    expect(screen.getByText("友情链接")).toBeInTheDocument();
    expect(screen.getByText("https://getmcode.lingyun.net")).toBeInTheDocument();
    expect(screen.getByText("https://airdb.lingyun.net/")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "打开uview-plus" }));

    expect(openExternal).toHaveBeenCalledWith("https://uview-plus.jiangruyi.com/");
  });

  it("opens the QQ group invite and keeps the group name and address visible", async () => {
    renderAbout();

    expect(screen.getByRole("heading", { name: "加入交流群" })).toBeInTheDocument();
    // The invite link expires; the group name is what lets the user search for it.
    expect(screen.getByText(`QQ 群「${QQ_GROUP_NAME}」`)).toBeInTheDocument();
    expect(screen.getByText(QQ_GROUP_URL)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "加入 QQ 群聊" }));

    expect(openExternal).toHaveBeenCalledWith(QQ_GROUP_URL);
  });

  it("explains a refused group invite instead of dropping the rejection", async () => {
    vi.mocked(openExternal).mockRejectedValue(new Error("forbidden url"));
    renderAbout();

    await userEvent.click(screen.getByRole("button", { name: "加入 QQ 群聊" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "打不开链接，请复制地址到浏览器里打开。",
    );
    expect(screen.getByText(QQ_GROUP_URL)).toBeInTheDocument();
  });

  it("copies a share blurb that carries the website address", async () => {
    const writeText = stubClipboard();
    renderAbout();

    const shareButton = screen.getByRole("button", { name: "复制分享文案" });
    expect(screen.getByTestId("about-share-text")).toHaveTextContent(OFFICIAL_SITE_URL);

    await userEvent.click(shareButton);

    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = writeText.mock.calls[0][0] as string;
    expect(copied).toContain("AI Switch");
    expect(copied).toContain(OFFICIAL_SITE_URL);
    expect(screen.getByRole("button", { name: "已复制到剪贴板" })).toBeInTheDocument();
  });

  it("asks for a manual copy when the clipboard is unavailable", async () => {
    // Plain-HTTP web access is not a secure context, so neither path is there.
    document.execCommand = vi.fn(() => false);
    renderAbout();

    await userEvent.click(screen.getByRole("button", { name: "复制分享文案" }));

    expect(screen.getByRole("status")).toHaveTextContent("复制失败，请手动选中下面的文案复制。");
    expect(screen.getByRole("button", { name: "复制分享文案" })).toBeInTheDocument();
  });

  it("explains that a link could not be opened instead of failing silently", async () => {
    vi.mocked(openExternal).mockRejectedValue(new Error("forbidden url"));
    renderAbout();

    await userEvent.click(screen.getByRole("button", { name: "打开源码仓库" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "打不开链接，请复制地址到浏览器里打开。",
    );
  });
});
