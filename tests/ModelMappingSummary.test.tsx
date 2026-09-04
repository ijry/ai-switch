import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import {
  expandDisplayModelMappings,
  ModelMappingSummary,
} from "../src/components/accounts/ModelMappingSummary";

describe("ModelMappingSummary", () => {
  it("shows the baseline models for empty mappings", async () => {
    const user = userEvent.setup();
    render(<ModelMappingSummary platform="codex" mappings={[]} />);

    const label = screen.getByText("基线模型");
    const tooltip = screen.getByRole("tooltip", { hidden: true });
    expect(label.parentElement).toHaveAttribute("aria-describedby", tooltip.id);
    expect(tooltip).toHaveTextContent(
      "未配置模型映射，仅匹配基线模型：gpt-5.6-sol、gpt-5.6-terra、gpt-5.6-luna、gpt-5.5",
    );
    expect(tooltip).toHaveClass("group-hover:block", "whitespace-normal");
    await user.hover(label);
  });

  it("shows three aliases and opens the remaining mappings", async () => {
    const user = userEvent.setup();
    render(
      <ModelMappingSummary
        platform="codex"
        mappings={[
          { from: "a", to: "up-a" },
          { from: "b", to: "up-b" },
          { from: "c", to: "up-c" },
          { from: "d", to: "up-d" },
        ]}
      />,
    );

    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
    expect(screen.getByText("c")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "+1" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+1" }));

    expect(screen.getByText("d → up-d")).toBeInTheDocument();
  });

  it("closes the popover with Escape and an outside click", async () => {
    const user = userEvent.setup();
    render(
      <ModelMappingSummary
        platform="codex"
        mappings={[
          { from: "a", to: "up-a" },
          { from: "b", to: "up-b" },
          { from: "c", to: "up-c" },
          { from: "d", to: "up-d" },
        ]}
      />,
    );

    await user.click(screen.getByRole("button", { name: "+1" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+1" }));
    await user.click(document.body);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows the Claude upstream models rather than the shared role names", async () => {
    const user = userEvent.setup();
    render(
      <ModelMappingSummary
        platform="claude"
        mappings={[
          { from: "claude-sonnet-alias", to: "glm-5.2", label: "Sonnet" },
          { from: "claude-opus-alias", to: "glm-5.2", label: "Opus" },
          { from: "claude-haiku-alias", to: "glm-5.2-air" },
          { from: "claude-subagent", to: "glm-5.2-air" },
          { from: "claude-model", to: "kimi-k3" },
        ]}
      />,
    );

    // Every Claude account fills the same role set, so role names made all cards
    // identical. The upstream target is what tells them apart.
    expect(screen.queryByText("Sonnet")).not.toBeInTheDocument();
    const primary = screen.getByText("glm-5.2");
    expect(primary).toHaveAttribute("title", "Sonnet、Opus → glm-5.2");
    // Roles sharing one upstream model collapse into a single tag.
    expect(screen.getByText("glm-5.2-air")).toHaveAttribute(
      "title",
      "Haiku、Subagent → glm-5.2-air",
    );
    expect(screen.getByText("kimi-k3")).toHaveAttribute("title", "默认兜底模型 → kimi-k3");
    expect(screen.queryByRole("button", { name: /^\+/ })).not.toBeInTheDocument();

    await user.hover(primary);
  });

  it("collapses the Claude 1M variant into its upstream model and marks it", async () => {
    const user = userEvent.setup();
    render(
      <ModelMappingSummary
        platform="claude"
        mappings={[
          { from: "claude-sonnet-alias", to: "up-a", supports_1m: true },
          { from: "claude-opus-alias", to: "up-b" },
          { from: "claude-fable-alias", to: "up-c" },
          { from: "claude-haiku-alias", to: "up-d" },
        ]}
      />,
    );

    // The `[1m]` clone shares the target, so it adds a marker, not a tag.
    expect(screen.getByText("up-a")).toHaveAttribute("title", "Sonnet → up-a · 1M");
    expect(screen.getAllByTitle(/→ up-/)).toHaveLength(3);

    await user.click(screen.getByRole("button", { name: "+1" }));
    expect(screen.getByText("Haiku → up-d")).toBeInTheDocument();
  });

  it("expands Claude 1M mappings without duplicating an existing suffix", () => {
    expect(
      expandDisplayModelMappings("claude", [
        { from: "claude-sonnet-alias", to: "up-sonnet", label: "Sonnet", supports_1m: true },
        { from: "claude-opus-alias[1m]", to: "up-opus", supports_1m: true },
      ]),
    ).toEqual([
      {
        alias: "claude-sonnet-alias",
        target: "up-sonnet",
        label: "Sonnet",
        oneM: false,
      },
      {
        alias: "claude-sonnet-alias[1m]",
        target: "up-sonnet",
        label: "Sonnet",
        oneM: true,
      },
      {
        alias: "claude-opus-alias[1m]",
        target: "up-opus",
        label: null,
        oneM: false,
      },
    ]);
  });

  it("carries the Codex catalog extras only for a hand-picked list", () => {
    expect(
      expandDisplayModelMappings("codex", [
        { from: "gpt-5.5", to: "up-gpt", context_window: 400_000, reasoning_levels: ["max"] },
        // Following the baseline adds nothing the model id does not already say.
        { from: "gpt-5.6-sol", to: "up-sol" },
      ]),
    ).toEqual([
      {
        alias: "gpt-5.5",
        target: "up-gpt",
        label: null,
        oneM: false,
        contextWindow: 400_000,
        reasoningLevels: ["max"],
      },
      {
        alias: "gpt-5.6-sol",
        target: "up-sol",
        label: null,
        oneM: false,
        contextWindow: null,
        reasoningLevels: null,
      },
    ]);
  });

  it("puts the Codex context window and efforts in the row tooltip", () => {
    render(
      <ModelMappingSummary
        platform="codex"
        mappings={[
          { from: "gpt-5.5", to: "up-gpt", context_window: 200_000, reasoning_levels: ["medium", "max"] },
        ]}
      />,
    );

    expect(screen.getByText("gpt-5.5")).toHaveAttribute(
      "title",
      "gpt-5.5 → up-gpt · 200K · medium/max",
    );
  });
});
