import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import {
  expandDisplayModelMappings,
  ModelMappingSummary,
} from "../src/components/accounts/ModelMappingSummary";

describe("ModelMappingSummary", () => {
  it("shows wildcard state for empty mappings", () => {
    render(<ModelMappingSummary platform="codex" mappings={[]} />);

    expect(screen.getByText("模型通配")).toBeInTheDocument();
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
        { from: "claude-sonnet-5", to: "up-sonnet", label: "Sonnet", supports_1m: true },
        { from: "claude-opus-4-8[1m]", to: "up-opus", supports_1m: true },
      ]),
    ).toEqual([
      {
        alias: "claude-sonnet-5",
        target: "up-sonnet",
        label: "Sonnet",
        oneM: false,
      },
      {
        alias: "claude-sonnet-5[1m]",
        target: "up-sonnet",
        label: "Sonnet",
        oneM: true,
      },
      {
        alias: "claude-opus-4-8[1m]",
        target: "up-opus",
        label: null,
        oneM: false,
      },
    ]);
  });
});
