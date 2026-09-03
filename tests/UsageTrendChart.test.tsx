import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { UsageTrendChart } from "../src/components/accounts/UsageTrendChart";
import type { UsageTrendBucket, UsageTrendRow } from "../src/lib/api/types";

/** The plot box the component lays out inside; mirrors its own constants. */
const padTop = 18;
const plotHeight = 168;
const baseline = padTop + plotHeight;

function bucket(index: number, tokens: number): UsageTrendBucket {
  const day = String(index + 1).padStart(2, "0");
  return {
    start: `2026-08-${day}T00:00:00+08:00`,
    label: `08-${day}`,
    title: `2026-08-${day}`,
    request_count: 2,
    input_tokens: tokens,
    output_tokens: 0,
    cache_write_tokens: 0,
    cache_read_tokens: 0,
    cost_micros: tokens,
  };
}

/** Two series whose per-bucket totals are distinct and rise to a single peak. */
function fixture(count: number) {
  const values = Array.from({ length: count }, (_, index) => (index + 1) * 1_000);
  const buckets = values.map((tokens, index) => bucket(index, tokens));
  const rows: UsageTrendRow[] = [
    { key: "opus", tokens: values.map((value) => value * 0.6) },
    { key: "haiku", tokens: values.map((value) => value * 0.4) },
  ];
  return { buckets, rows };
}

function renderChart(count: number) {
  const { buckets, rows } = fixture(count);
  const view = render(
    <UsageTrendChart
      buckets={buckets}
      dimensionLabel="模型"
      rows={rows}
      undatedRequestCount={0}
      unit="day"
    />,
  );
  return view.container;
}

/** Bar totals are the middle-anchored labels sitting above the baseline. */
function barTotalLabels(container: HTMLElement) {
  return Array.from(container.querySelectorAll("text")).filter(
    (node) =>
      node.getAttribute("text-anchor") === "middle" &&
      Number(node.getAttribute("y")) < baseline,
  );
}

describe("UsageTrendChart", () => {
  it("labels every bar when the bands are wide enough", () => {
    const container = renderChart(7);

    expect(barTotalLabels(container)).toHaveLength(7);
  });

  it("labels only the peak once the bands are too narrow for a label each", () => {
    // A value above every bar at this density overlaps its neighbours, so the
    // axis and the tooltip carry the rest.
    const container = renderChart(30);

    const labels = barTotalLabels(container);
    expect(labels).toHaveLength(1);
    // The tallest bucket is the last one in this fixture.
    expect(labels[0]?.textContent).toBe("3.0万");
  });

  it("thins the bucket labels instead of overlapping them", () => {
    const container = renderChart(30);

    const axisLabels = Array.from(container.querySelectorAll("text")).filter(
      (node) => Number(node.getAttribute("y")) > baseline,
    );
    expect(axisLabels.length).toBeLessThan(30);
    expect(axisLabels.length).toBeGreaterThan(5);
    expect(axisLabels[0]?.textContent).toBe("08-01");
  });

  it("keeps every mark inside the plot box", () => {
    const container = renderChart(12);

    const marks = Array.from(container.querySelectorAll("rect")).filter(
      (node) => node.getAttribute("fill") !== "transparent",
    );
    expect(marks.length).toBeGreaterThan(0);
    for (const mark of marks) {
      const y = Number(mark.getAttribute("y"));
      const height = Number(mark.getAttribute("height"));
      expect(height).toBeGreaterThan(0);
      expect(y).toBeGreaterThanOrEqual(padTop);
      expect(y + height).toBeLessThanOrEqual(baseline + 0.001);
    }
  });

  it("caps the bar thickness rather than filling the band", () => {
    const container = renderChart(3);

    const bar = container.querySelector('rect[fill^="#"]');
    expect(Number(bar?.getAttribute("width"))).toBeLessThanOrEqual(24);
  });

  it("says so instead of drawing an empty frame when the window has no data", () => {
    const view = render(
      <UsageTrendChart
        buckets={[]}
        dimensionLabel="模型"
        rows={[]}
        undatedRequestCount={3}
        unit="day"
      />,
    );

    expect(view.getByText("当前筛选范围内没有数据。")).toBeInTheDocument();
    expect(view.getByText(/3 个请求没有时间戳/)).toBeInTheDocument();
  });
});
