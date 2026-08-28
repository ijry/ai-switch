import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ReleaseNotes } from "../src/components/updates/ReleaseNotes";
import { I18nProvider } from "../src/lib/i18n";

function renderNotes(notes: string, language: "zh-CN" | "en" = "zh-CN") {
  return render(
    <I18nProvider initialLanguage={language}>
      <ReleaseNotes notes={notes} />
    </I18nProvider>,
  );
}

describe("ReleaseNotes", () => {
  it("renders markdown structure rather than raw text", () => {
    renderNotes("## 修复\n\n- 一条 `settings.json` 修复\n\n[文档](https://example.com/docs)");

    expect(screen.getByRole("heading", { level: 2, name: "修复" })).toBeInTheDocument();
    expect(screen.getByRole("listitem")).toBeInTheDocument();
    expect(screen.getByText("settings.json").tagName).toBe("CODE");
    expect(screen.getByRole("link", { name: "文档" })).toHaveAttribute(
      "href",
      "https://example.com/docs",
    );
  });

  it("renders GFM tables", () => {
    renderNotes(["| 槽位 | 别名 |", "| --- | --- |", "| Sonnet | claude-sonnet-alias |"].join("\n"));

    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "槽位" })).toBeInTheDocument();
  });

  it("renders nothing when the notes are blank", () => {
    const { container } = renderNotes("   \n  ");

    expect(container).toBeEmptyDOMElement();
  });

  it("carries the typography classes so the panel is not unstyled", () => {
    const { container } = renderNotes("- 一条修复");

    expect(container.firstElementChild?.className).toContain("[&_ul]:list-disc");
  });
});
