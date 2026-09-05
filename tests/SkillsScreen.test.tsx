import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  skillsDelete,
  skillsInstallPackage,
  skillsList,
  skillsListAgents,
  skillsListPackages,
  skillsRead,
  skillsReadPackage,
  skillsSave,
  skillsUninstallPackage,
} from "../src/lib/api/client";
import { ApiClientError } from "../src/lib/api/errors";
import { I18nProvider } from "../src/lib/i18n";
import type { SkillItem, SkillPackage } from "../src/lib/api/types";
import { SkillsScreen } from "../src/screens/SkillsScreen";

vi.mock("../src/lib/api/client", () => ({
  skillsDelete: vi.fn(),
  skillsInstallPackage: vi.fn(),
  skillsList: vi.fn(),
  skillsListAgents: vi.fn(),
  skillsListPackages: vi.fn(),
  skillsRead: vi.fn(),
  skillsReadPackage: vi.fn(),
  skillsSave: vi.fn(),
  skillsUninstallPackage: vi.fn(),
}));

const skillFixture: SkillItem = {
  id: "demo",
  name: "Demo Skill",
  scope: "global",
  layout: "skill_directory",
  path: "C:/Users/example/.codex/skills/demo",
  description: "A demo skill",
  read_only: false,
};

/// A bundled Skill as it actually lands on disk: the frontmatter name is the id and
/// the description is the agent-facing trigger sentence.
const bundledSkillFixture: SkillItem = {
  id: "brainstorming",
  name: "brainstorming",
  scope: "global",
  layout: "skill_directory",
  path: "C:/Users/example/.codex/skills/brainstorming",
  description: "You MUST use this before any creative work - creating features, building components.",
  read_only: false,
};

const corePackageFixture: SkillPackage = {
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

const listFixture = {
  supported: true,
  message: null,
  locations: [{ scope: "global" as const, path: "C:/Users/example/.codex/skills", exists: true }],
  skills: [skillFixture],
};

function renderScreen(language: "en" | "zh-CN") {
  return render(
    <I18nProvider initialLanguage={language}>
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <SkillsScreen />
      </QueryClientProvider>
    </I18nProvider>,
  );
}

/// Puts the screen on the Skill packages tab with the core pack selected.
async function openCorePackage() {
  vi.mocked(skillsListPackages).mockResolvedValue({
    packages: [corePackageFixture],
    skills: [],
    warnings: [],
  });
  vi.mocked(skillsReadPackage).mockResolvedValue({
    package: corePackageFixture,
    skills: [bundledSkillFixture],
    members: [
      {
        id: "brainstorming",
        name: "brainstorming",
        description: null,
        category: null,
        tags: [],
        language: "en",
        installed: true,
        skill: bundledSkillFixture,
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
  });
  renderScreen("zh-CN");
  const user = userEvent.setup();
  await vi.waitFor(() => expect(screen.getByRole("tab", { name: "技能包" })).toBeInTheDocument());
  await user.click(screen.getByRole("tab", { name: "技能包" }));
  await screen.findByText("头脑风暴");
  return user;
}

describe("SkillsScreen", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
    vi.mocked(skillsListAgents).mockResolvedValue([
      { agent_type: "codex", display_name: "Codex CLI", skills_capable: true },
    ]);
    vi.mocked(skillsList).mockResolvedValue(listFixture);
    vi.mocked(skillsRead).mockResolvedValue({ skill: skillFixture, content: "# Demo" });
    vi.mocked(skillsListPackages).mockResolvedValue({ packages: [], skills: [], warnings: [] });
    vi.mocked(skillsReadPackage).mockResolvedValue({
      package: {
        id: "ai-switch.core",
        name: "AI Switch Core Skill Pack",
        description: null,
        source: "builtin",
        version: null,
        manifest_path: null,
        skill_ids: [],
        skill_count: 0,
        installed_skill_ids: [],
        installed_count: 0,
        installed_at: null,
        read_only: true,
        target_clients: ["codex"],
      },
      skills: [],
      members: [],
    });
  });

  it("renders the Skills controls in Simplified Chinese", async () => {
    renderScreen("zh-CN");

    expect(await screen.findByRole("heading", { name: "技能" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新建技能" })).toBeInTheDocument();
    expect(screen.getByText("可用技能")).toBeInTheDocument();
    expect(await screen.findByText("Demo Skill")).toBeInTheDocument();
  });

  it("localizes a structured Skills error", async () => {
    vi.mocked(skillsList).mockRejectedValue(
      new ApiClientError("raw backend text", "skills.read_only", null, true, null),
    );

    renderScreen("zh-CN");

    expect(await screen.findByRole("alert")).toHaveTextContent("该技能为只读");
    expect(screen.getByRole("alert")).not.toHaveTextContent("raw backend text");
  });

  it("keeps the list and editor inside shrinkable containers", async () => {
    renderScreen("en");

    const list = await screen.findByRole("complementary");
    const editor = screen.getByRole("main");
    expect(list.className).toContain("min-h-0");
    expect(list.className).toContain("overflow-hidden");
    expect(editor.className).toContain("min-w-0");
    expect(editor.className).toContain("overflow-hidden");
  });

  it("keeps Skills as the top-level screen and exposes two internal tabs", async () => {
    renderScreen("zh-CN");

    expect(screen.getByRole("tab", { name: "技能" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "技能包" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "技能包" })).not.toBeInTheDocument();
  });

  it("shows AI Switch core and science packages", async () => {
    vi.mocked(skillsListPackages).mockResolvedValue({
      packages: [
        {
          id: "ai-switch.core",
          name: "AI Switch Core Skill Pack",
          description: null,
          source: "builtin",
          version: null,
          manifest_path: null,
          skill_ids: ["demo"],
          skill_count: 1,
          installed_skill_ids: ["demo"],
          installed_count: 1,
          installed_at: null,
          read_only: true,
          target_clients: ["codex"],
        },
        {
          id: "ai-switch.science",
          name: "AI Switch Science Skill Pack",
          description: null,
          source: "builtin",
          version: null,
          manifest_path: null,
          skill_ids: [],
          skill_count: 0,
          installed_skill_ids: [],
          installed_count: 0,
          installed_at: null,
          read_only: true,
          target_clients: ["codex"],
        },
      ],
      skills: [],
      warnings: [],
    });

    renderScreen("zh-CN");
    await vi.waitFor(() => expect(screen.getByRole("tab", { name: "技能包" })).toBeInTheDocument());
    await userEvent.setup().click(screen.getByRole("tab", { name: "技能包" }));

    expect(await screen.findByRole("button", { name: /AI Switch 核心技能包/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /AI Switch 科学技能包/ })).toBeInTheDocument();
  });

  it("shows bundled Chinese copy for a bundled Skill", async () => {
    vi.mocked(skillsList).mockResolvedValue({ ...listFixture, skills: [bundledSkillFixture] });

    renderScreen("zh-CN");

    expect(await screen.findByText("头脑风暴")).toBeInTheDocument();
    expect(
      screen.getByText("通过对话把想法整理成规格说明，得到确认后才允许动手写代码。"),
    ).toBeInTheDocument();
  });

  it("shows bundled English copy instead of the agent-facing trigger sentence", async () => {
    vi.mocked(skillsList).mockResolvedValue({ ...listFixture, skills: [bundledSkillFixture] });

    renderScreen("en");

    expect(await screen.findByText("Brainstorming")).toBeInTheDocument();
    expect(
      screen.getByText("Turns an idea into an approved design and spec before any code gets written."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/You MUST use this before any creative work/)).not.toBeInTheDocument();
  });

  it("filters Skills by their Chinese copy as well as their id", async () => {
    vi.mocked(skillsList).mockResolvedValue({
      ...listFixture,
      skills: [bundledSkillFixture, skillFixture],
    });

    renderScreen("zh-CN");
    const list = await screen.findByRole("complementary");
    await vi.waitFor(() => expect(within(list).getByText("Demo Skill")).toBeInTheDocument());
    await userEvent.setup().type(screen.getByLabelText("筛选技能"), "头脑");

    // The count is `matched / total`; asserting on it rather than on the removed
    // row keeps the test off `AnimatePresence`'s exit animation, which leaves the
    // filtered-out row in the DOM until it finishes.
    expect(within(list).getByText("1 / 2")).toBeInTheDocument();
    expect(within(list).getByText("头脑风暴")).toBeInTheDocument();
  });

  it("installs a single pack member without installing the whole pack", async () => {
    const user = await openCorePackage();

    await user.click(screen.getByRole("button", { name: "安装" }));

    expect(skillsInstallPackage).toHaveBeenCalledWith({
      packageId: "ai-switch.core",
      agentType: "codex",
      scope: "global",
      workspacePath: null,
      skillIds: ["writing-plans"],
    });
  });

  it("uninstalls a single pack member once the deletion is confirmed", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = await openCorePackage();

    await user.click(screen.getByRole("button", { name: "卸载" }));

    expect(confirm).toHaveBeenCalledOnce();
    expect(skillsUninstallPackage).toHaveBeenCalledWith({
      packageId: "ai-switch.core",
      agentType: "codex",
      scope: "global",
      workspacePath: null,
      skillIds: ["brainstorming"],
    });
    confirm.mockRestore();
  });

  it("leaves the Skill files alone when the uninstall confirmation is declined", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const user = await openCorePackage();

    await user.click(screen.getByRole("button", { name: "卸载已安装技能" }));

    expect(confirm).toHaveBeenCalledOnce();
    expect(skillsUninstallPackage).not.toHaveBeenCalled();
    confirm.mockRestore();
  });
});
