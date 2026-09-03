import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import {
  displayMappingTitle,
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

  it("shows Claude role names, keeping the internal alias in the tooltip", async () => {
    const user = userEvent.setup();
    render(
      <ModelMappingSummary
        platform="claude"
        mappings={[
          { from: "claude-sonnet-alias", to: "claude-opus-5", label: "Sonnet" },
          { from: "claude-opus-alias", to: "claude-opus-5", label: "Opus" },
          { from: "claude-subagent", to: "claude-opus-5" },
          { from: "claude-model", to: "claude-opus-5" },
        ]}
      />,
    );

    // The role name is the title; `claude-opus-alias → claude-opus-5` read as a
    // misconfiguration.
    const sonnet = screen.getByText("Sonnet");
    expect(sonnet).toBeInTheDocument();
    expect(sonnet).toHaveAttribute("title", "claude-sonnet-alias → claude-opus-5 · Sonnet");
    expect(screen.queryByText("claude-sonnet-alias")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+1" }));
    expect(screen.getByText("默认兜底模型 → claude-opus-5")).toBeInTheDocument();
  });

  it("titles a hand-written alias verbatim and marks the 1M variant", () => {
    expect(
      displayMappingTitle({ alias: "claude-opus-alias", target: "x", oneM: false }),
    ).toBe("Opus");
    expect(displayMappingTitle({ alias: "claude-opus-alias", target: "x", oneM: true })).toBe(
      "Opus · 1M",
    );
    expect(
      displayMappingTitle({ alias: "claude-opus-alias[1m]", target: "x", oneM: false }),
    ).toBe("Opus · 1M");
    // No role: showing what the user typed is correct.
    expect(displayMappingTitle({ alias: "my-own-alias", target: "x", oneM: false })).toBe(
      "my-own-alias",
    );
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
