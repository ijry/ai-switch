import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import type { SkillItem, SkillPackage, SkillPackageDetail as SkillPackageDetailData } from "../src/lib/api/types";
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
  name: "brainstorming",
  scope: "global",
  layout: "skill_directory",
  path: "C:/Users/Admin/.codex/skills/brainstorming",
  description: "You MUST use this before any creative work - creating features, building components.",
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

const detailFixture: SkillPackageDetailData = {
  package: packageFixture,
  skills: [skillFixture],
  members: [
    {
      id: "brainstorming",
      name: "brainstorming",
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
};

function renderWithLanguage(node: React.ReactNode) {
  return render(<I18nProvider initialLanguage="zh-CN">{node}</I18nProvider>);
}

function renderDetail(overrides: Partial<React.ComponentProps<typeof SkillPackageDetail>> = {}) {
  const props = {
    busySkillId: null,
    detail: detailFixture,
    installing: false,
    loading: false,
    onInstallMember: vi.fn(),
    onInstallMissing: vi.fn(),
    onSelectSkill: vi.fn(),
    onUninstallAll: vi.fn(),
    onUninstallMember: vi.fn(),
    uninstalling: false,
    ...overrides,
  };
  renderWithLanguage(<SkillPackageDetail {...props} />);
  return props;
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

  it("shows bundled Chinese copy for members instead of the raw id", () => {
    renderDetail();

    expect(screen.getByText("头脑风暴")).toBeInTheDocument();
    expect(screen.getByText("撰写方案")).toBeInTheDocument();
    expect(screen.getByText("通过对话把想法整理成规格说明，得到确认后才允许动手写代码。")).toBeInTheDocument();
    expect(screen.getByText("brainstorming")).toBeInTheDocument();
  });

  it("emits a selected member for navigation back to Skills", async () => {
    const props = renderDetail();

    await userEvent.setup().click(screen.getByRole("button", { name: /头脑风暴/ }));

    expect(props.onSelectSkill).toHaveBeenCalledWith(skillFixture);
    expect(screen.getByText("未安装")).toBeInTheDocument();
  });

  it("installs one missing member without touching the rest of the pack", async () => {
    const props = renderDetail();

    await userEvent.setup().click(screen.getByRole("button", { name: "安装" }));

    expect(props.onInstallMember).toHaveBeenCalledWith("writing-plans");
    expect(props.onInstallMissing).not.toHaveBeenCalled();
  });

  it("uninstalls a single installed member", async () => {
    const props = renderDetail();

    await userEvent.setup().click(screen.getByRole("button", { name: "卸载" }));

    expect(props.onUninstallMember).toHaveBeenCalledWith("brainstorming");
    expect(props.onUninstallAll).not.toHaveBeenCalled();
  });

  it("offers a pack-wide install and uninstall", async () => {
    const props = renderDetail();
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "安装缺失技能" }));
    await user.click(screen.getByRole("button", { name: "卸载已安装技能" }));

    expect(props.onInstallMissing).toHaveBeenCalledOnce();
    expect(props.onUninstallAll).toHaveBeenCalledOnce();
  });

  it("keeps a read-only member from being uninstalled", () => {
    renderDetail({
      detail: {
        ...detailFixture,
        members: [
          {
            ...detailFixture.members[0],
            skill: { ...skillFixture, read_only: true },
          },
        ],
      },
    });

    expect(screen.getByRole("button", { name: "卸载" })).toBeDisabled();
  });

  it("disables every action while one member is being installed", () => {
    renderDetail({ busySkillId: "writing-plans" });

    expect(screen.getByRole("button", { name: "安装缺失技能" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "卸载已安装技能" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "卸载" })).toBeDisabled();
  });
});
