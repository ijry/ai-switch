import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
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
} from "../src/lib/api/client";
import { ApiClientError } from "../src/lib/api/errors";
import { I18nProvider } from "../src/lib/i18n";
import type { SkillItem } from "../src/lib/api/types";
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
});
