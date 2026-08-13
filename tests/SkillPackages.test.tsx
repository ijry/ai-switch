import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import type { SkillItem, SkillPackage } from "../src/lib/api/types";
import { SkillPackageDetail } from "../src/components/skills/SkillPackageDetail";
import { SkillPackagesList } from "../src/components/skills/SkillPackagesList";

const packageFixture: SkillPackage = {
  id: "ai-switch.core",
  name: "AI Switch Core Skill Pack",
  description: null,
  source: "builtin",
  version: null,
  manifest_path: null,
  skill_ids: ["brainstorming", "writing-plans"],
  skill_count: 2,
  installed_skill_ids: ["brainstorming"],
  installed_count: 1,
  installed_at: null,
  read_only: true,
  target_clients: ["codex"],
};

const skillFixture: SkillItem = {
  id: "brainstorming",
  name: "Brainstorming",
  scope: "global",
  layout: "skill_directory",
  path: "C:/Users/Admin/.codex/skills/brainstorming",
  description: "Explore requirements before implementation.",
  read_only: false,
  package_id: "ai-switch.core",
  package_name: "AI Switch Core Skill Pack",
  category: null,
  tags: [],
  language: "en",
  source: "codex",
  version: "0.22.1",
  installed_at: "2026-08-11T06:08:45Z",
  target_clients: ["codex"],
};

function renderWithLanguage(node: React.ReactNode) {
  return render(<I18nProvider initialLanguage="zh-CN">{node}</I18nProvider>);
}

describe("Skill package components", () => {
  it("renders package metadata and warnings", () => {
    renderWithLanguage(
      <SkillPackagesList
        loading={false}
        onSelect={vi.fn()}
        packages={[packageFixture]}
        selectedId={packageFixture.id}
        warnings={[{ code: "skills.package_scan_failed", path: "skill-packages", message: "invalid" }]}
      />,
    );

    expect(screen.getByRole("button", { name: /AI Switch 核心技能包/ })).toBeInTheDocument();
    expect(screen.getByText("已安装 1/2")).toBeInTheDocument();
    expect(screen.getByText("部分技能包元数据无法读取。")).toBeInTheDocument();
  });

  it("emits a selected member for navigation back to Skills", async () => {
    const onSelectSkill = vi.fn();
    const onInstallMissing = vi.fn();
    renderWithLanguage(
      <SkillPackageDetail
        detail={{
          package: packageFixture,
          skills: [skillFixture],
          members: [
            {
              id: "brainstorming",
              name: "Brainstorming",
              description: null,
              category: null,
              tags: [],
              language: "en",
              installed: true,
              skill: skillFixture,
            },
            {
              id: "writing-plans",
              name: "writing-plans",
              description: null,
              category: null,
              tags: [],
              language: null,
              installed: false,
              skill: null,
            },
          ],
        }}
        installing={false}
        loading={false}
        onInstallMissing={onInstallMissing}
        onSelectSkill={onSelectSkill}
      />,
    );

    await userEvent.setup().click(screen.getByRole("button", { name: "安装缺失技能" }));
    await userEvent.setup().click(screen.getByRole("button", { name: /Brainstorming/ }));

    expect(onInstallMissing).toHaveBeenCalledOnce();
    expect(onSelectSkill).toHaveBeenCalledWith(skillFixture);
    expect(screen.getByText("未安装")).toBeInTheDocument();
  });
});
