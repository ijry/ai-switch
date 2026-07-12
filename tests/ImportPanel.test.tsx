import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ImportPanel } from "../src/components/imports/ImportPanel";

describe("ImportPanel", () => {
  it("requires a batch name before import", async () => {
    const onImport = vi.fn();
    render(<ImportPanel onImport={onImport} />);

    fireEvent.change(screen.getByLabelText(/json/i), {
      target: { value: "{\"providers\":[],\"accounts\":[]}" },
    });
    await userEvent.click(screen.getByRole("button", { name: /import/i }));

    expect(screen.getByText("Batch name is required.")).toBeInTheDocument();
    expect(onImport).not.toHaveBeenCalled();
  });
});
