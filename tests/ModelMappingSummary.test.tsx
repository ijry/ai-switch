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
});
