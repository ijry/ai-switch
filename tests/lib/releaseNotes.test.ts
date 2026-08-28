import { describe, expect, it } from "vitest";
import {
  localizeReleaseNotes,
  prefersChineseReleaseNotes,
  splitBilingualReleaseNotes,
} from "../../src/lib/releaseNotes";

const RULE = "-".repeat(29);

function bilingual(chinese: string, english: string) {
  return `${chinese}\n\n${RULE}\n\n${english}`;
}

describe("splitBilingualReleaseNotes", () => {
  it("splits on the published rule and labels each half by language", () => {
    const body = bilingual(
      "中文发布说明\n\n修复\n- 修正了写入客户端配置时缺少鉴权令牌的问题。",
      "English Release Notes\n\nFixes\n- Write the auth token when generating client config.",
    );

    const { chinese, english } = splitBilingualReleaseNotes(body);

    expect(chinese).toContain("中文发布说明");
    expect(chinese).not.toContain("English Release Notes");
    expect(english).toContain("English Release Notes");
    expect(english).not.toContain("中文发布说明");
  });

  it("labels the halves by content, not by position", () => {
    const body = bilingual(
      "English Release Notes\n\nFixes\n- Write the auth token when generating client config.",
      "中文发布说明\n\n修复\n- 修正了写入客户端配置时缺少鉴权令牌的问题。",
    );

    const { chinese, english } = splitBilingualReleaseNotes(body);

    expect(english).toContain("English Release Notes");
    expect(chinese).toContain("中文发布说明");
  });

  it("ignores a short rule used to divide sections", () => {
    const body = [
      "English Release Notes",
      "",
      "---",
      "",
      "Fixes",
      "- Something in English only.",
    ].join("\n");

    expect(splitBilingualReleaseNotes(body)).toEqual({ chinese: null, english: null });
  });

  it("ignores a long rule whose halves read in the same language", () => {
    const body = bilingual(
      "English Release Notes\n\nFixes\n- One fix.",
      "More English\n\nImprovements\n- One improvement.",
    );

    expect(splitBilingualReleaseNotes(body)).toEqual({ chinese: null, english: null });
  });

  it("ignores a rule inside a fenced code block", () => {
    const body = [
      "中文发布说明",
      "",
      "```text",
      "",
      RULE,
      "",
      "```",
      "",
      "修复",
      "- 一条中文修复。",
    ].join("\n");

    expect(splitBilingualReleaseNotes(body)).toEqual({ chinese: null, english: null });
  });

  it("requires a blank line above the rule so heading underlines are not split points", () => {
    const body = ["中文发布说明", RULE, "English Release Notes"].join("\n");

    expect(splitBilingualReleaseNotes(body)).toEqual({ chinese: null, english: null });
  });

  it("returns null halves when the body carries no rule", () => {
    const body = "中文发布说明\n\nEnglish Release Notes";

    expect(splitBilingualReleaseNotes(body)).toEqual({ chinese: null, english: null });
  });

  it("normalizes CRLF bodies", () => {
    const body = bilingual(
      "中文发布说明\n\n修复\n- 一条中文修复。",
      "English Release Notes\n\nFixes\n- One fix.",
    ).replace(/\n/g, "\r\n");

    const { chinese, english } = splitBilingualReleaseNotes(body);

    expect(chinese).toContain("中文发布说明");
    expect(english).toContain("English Release Notes");
  });
});

describe("prefersChineseReleaseNotes", () => {
  it("reads the Chinese half for Chinese locales", () => {
    expect(prefersChineseReleaseNotes("zh-CN")).toBe(true);
    expect(prefersChineseReleaseNotes("zh_TW")).toBe(true);
  });

  it("falls back to English for every other locale", () => {
    expect(prefersChineseReleaseNotes("en")).toBe(false);
    expect(prefersChineseReleaseNotes("ja-JP")).toBe(false);
  });
});

describe("localizeReleaseNotes", () => {
  const body = bilingual(
    "中文发布说明\n\n修复\n- 一条中文修复。",
    "English Release Notes\n\nFixes\n- One fix.",
  );

  it("returns the half matching the interface language", () => {
    expect(localizeReleaseNotes(body, "zh-CN")).toContain("中文发布说明");
    expect(localizeReleaseNotes(body, "zh-CN")).not.toContain("English Release Notes");
    expect(localizeReleaseNotes(body, "en")).toContain("English Release Notes");
    expect(localizeReleaseNotes(body, "en")).not.toContain("中文发布说明");
  });

  it("returns the whole body when it is not bilingual", () => {
    const single = "English Release Notes\n\nFixes\n- One fix.";

    expect(localizeReleaseNotes(single, "zh-CN")).toBe(single);
  });

  it("returns an empty string for a blank body", () => {
    expect(localizeReleaseNotes("   \n  ", "en")).toBe("");
  });
});
