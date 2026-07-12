import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { BatchList } from "../src/components/batches/BatchList";
import { batchGroupsFixture } from "../src/test/fixtures";

describe("BatchList", () => {
  it("renders batches collapsed and expands child items", async () => {
    render(<BatchList groups={batchGroupsFixture} search="" />);

    expect(screen.getByText("July imports")).toBeInTheDocument();
    expect(screen.queryByText("Acme Claude")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /expand July imports/i }));

    expect(screen.getByText("Acme Claude")).toBeInTheDocument();
    expect(screen.getByText("Team Account")).toBeInTheDocument();
  });

  it("auto expands when search matches a child", () => {
    render(<BatchList groups={batchGroupsFixture} search="team@example.com" />);

    expect(screen.getByText("Team Account")).toBeInTheDocument();
  });
});
