import { readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { skillDisplayCopy, skillSearchHaystack } from "../src/components/skills/catalog";
import { bundledSkillCopy, bundledSkillIds } from "../src/components/skills/skillCopy";
import { supportedLanguages } from "../src/lib/i18n";

const PACKAGE_ROOT = resolve(process.cwd(), "src-tauri/resources/skill-packages");

function bundledResourceIds() {
  return readdirSync(PACKAGE_ROOT, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .flatMap((pack) =>
      readdirSync(resolve(PACKAGE_ROOT, pack.name), { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => entry.name),
    )
    .sort();
}

describe("bundled Skill copy", () => {
  it("covers every Skill shipped in a Skill package, in every language", () => {
    const missing: string[] = [];
    for (const id of bundledResourceIds()) {
      for (const { code } of supportedLanguages) {
        const copy = bundledSkillCopy(id, code);
        if (!copy?.name.trim() || !copy?.summary.trim()) missing.push(`${id} (${code})`);
      }
    }

    expect(missing).toEqual([]);
    expect(bundledSkillIds().sort()).toEqual(bundledResourceIds());
  });

  it("never falls back to the kebab-case id as a display name", () => {
    const rawIds = bundledSkillIds().filter((id) => bundledSkillCopy(id, "zh-CN")?.name === id);

    expect(rawIds).toEqual([]);
  });

  it("prefers bundled copy over the agent-facing frontmatter", () => {
    const chinese = bundledSkillCopy("brainstorming", "zh-CN")!;
    const item = {
      id: "brainstorming",
      name: "brainstorming",
      description: "You MUST use this before any creative work.",
    };

    expect(skillDisplayCopy(item, "zh-CN")).toEqual({
      name: chinese.name,
      description: chinese.summary,
    });
    expect(skillDisplayCopy(item, "en").name).toBe(bundledSkillCopy("brainstorming", "en")!.name);
  });

  it("falls back to a Skill's own metadata when nothing is bundled for it", () => {
    expect(skillDisplayCopy({ id: "my-skill", name: "My Skill", description: "Mine." }, "zh-CN")).toEqual({
      name: "My Skill",
      description: "Mine.",
    });
    expect(skillDisplayCopy({ id: "my-skill", name: "  ", description: "  " }, "en")).toEqual({
      name: "my-skill",
      description: null,
    });
  });

  it("matches a filter typed in either language", () => {
    const haystack = skillSearchHaystack({ id: "brainstorming", name: "brainstorming", description: null });

    expect(haystack).toContain("头脑风暴");
    expect(haystack).toContain("brainstorming");
    expect(skillSearchHaystack({ id: "my-skill", name: "My Skill", description: null })).toContain("my skill");
  });
});
