import { useCallback, useMemo, useRef, useState } from "react";
import { assignSeriesColors } from "../../lib/usageChartColors";
import { formatCompactCount, formatCostMicros, formatExactCount } from "../../lib/usageFormat";
import type { UsageBucketUnit, UsageTrendBucket, UsageTrendRow } from "../../lib/api/types";

/** Plot area, excluding the axis bands around it. */
const plotHeight = 168;
const padLeft = 46;
const padRight = 10;
/** Room above the tallest bar for its value label. */
const padTop = 18;
/** Room below the baseline for the bucket labels. */
const axisHeight = 20;
const gridLines = 4;
/** A bar never fills its band; the leftover is the breathing room. */
const maxBarWidth = 24;
/** White doing the separating, between stacked segments. */
const segmentGap = 2;
const cornerRadius = 4;
/** Under this band width a label cannot be placed without colliding. */
const minLabelBand = 34;
/** Assumed width until a ResizeObserver reports the real one. */
const fallbackWidth = 720;

const unitTitles: Record<UsageBucketUnit, string> = {
  hour: "按小时 Token 趋势",
  day: "按天 Token 趋势",
  week: "按周 Token 趋势",
  month: "按月 Token 趋势",
};

/** Round the axis top up to a clean number, so its gridline labels read. */
function niceMax(value: number) {
  if (value <= 0) {
    return 0;
  }
  const rough = value / gridLines;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step =
    [1, 2, 2.5, 5, 10]
      .map((factor) => factor * magnitude)
      .find((candidate) => candidate >= rough) ?? magnitude * 10;
  return step * gridLines;
}

/**
 * A rect with rounded top corners: the data-end of a stack is rounded, the
 * baseline end stays square.
 */
function topRoundedPath(x: number, y: number, width: number, height: number) {
  const radius = Math.max(0, Math.min(cornerRadius, width / 2, height));
  return [
    `M ${x} ${y + height}`,
    `L ${x} ${y + radius}`,
    `Q ${x} ${y} ${x + radius} ${y}`,
    `L ${x + width - radius} ${y}`,
    `Q ${x + width} ${y} ${x + width} ${y + radius}`,
    `L ${x + width} ${y + height}`,
    "Z",
  ].join(" ");
}

/**
 * Render at the container's real width rather than scaling a fixed viewBox:
 * scaled SVG text goes blurry and oversized, and the label-fits-or-not decision
 * below needs true pixels.
 */
function useMeasuredWidth() {
  const [width, setWidth] = useState(fallbackWidth);
  const observerRef = useRef<ResizeObserver | null>(null);
  const attach = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect();
    observerRef.current = null;
    if (!node || typeof ResizeObserver === "undefined") {
      return;
    }
    const update = () => setWidth(Math.max(240, node.clientWidth));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    observerRef.current = observer;
  }, []);
  return [width, attach] as const;
}

type Segment = {
  key: string;
  tokens: number;
  top: number;
  height: number;
};

type Column = {
  bucket: UsageTrendBucket;
  total: number;
  bandLeft: number;
  barLeft: number;
  stackTop: number;
  segments: Segment[];
};

export type UsageTrendChartProps = {
  unit: UsageBucketUnit;
  buckets: UsageTrendBucket[];
  /** The active dimension's series, biggest first, tail already folded. */
  rows: UsageTrendRow[];
  /** 模型 / 平台 / 账号 / 来源 — named in the subtitle so the stack is legible. */
  dimensionLabel: string;
  /** Requests with no timestamp, which no bar can account for. */
  undatedRequestCount: number;
  /** True while a newly selected period is still loading. */
  stale?: boolean;
};

/**
 * The hovered column's numbers.
 *
 * The value leads and the series name follows — the inverse of the legend's
 * hierarchy, because a reader who has already picked a bar wants the figure.
 */
function TrendTooltip({
  column,
  colors,
  left,
}: {
  column: Column;
  colors: Map<string, string>;
  left: number;
}) {
  const parts = [...column.segments].sort((first, second) => second.tokens - first.tokens);
  const cache = column.bucket.cache_write_tokens + column.bucket.cache_read_tokens;
  return (
    <div
      className="pointer-events-none absolute top-0 z-10 w-56 -translate-x-1/2 rounded-lg border border-stone-200 bg-white/95 p-2 shadow-lg"
      style={{ left }}
    >
      <p className="text-[11px] font-semibold text-stone-800">{column.bucket.title}</p>
      <p className="mt-0.5 text-[11px] text-stone-500">
        <span className="font-semibold text-stone-900">{formatCompactCount(column.total)}</span>
        {" Token · "}
        {formatExactCount(column.bucket.request_count)} 请求 ·{" "}
        {formatCostMicros(column.bucket.cost_micros)}
      </p>
      {cache > 0 ? (
        <p className="text-[11px] text-stone-400">
          缓存 {formatCompactCount(cache)}，不计入柱高
        </p>
      ) : null}
      {parts.length > 0 ? (
        <ul className="mt-1.5 space-y-0.5">
          {parts.map((segment) => (
            <li className="flex items-center gap-1.5 text-[11px]" key={segment.key}>
              <span
                aria-hidden="true"
                className="h-0.5 w-3 shrink-0 rounded-full"
                style={{ backgroundColor: colors.get(segment.key) }}
              />
              <span className="truncate text-stone-500">{segment.key}</span>
              <span className="ml-auto shrink-0 font-semibold text-stone-800">
                {formatCompactCount(segment.tokens)}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function UndatedNote({ count }: { count: number }) {
  return (
    <p className="mt-2 text-[11px] text-stone-400">
      另有 {formatExactCount(count)} 个请求没有时间戳，未计入图表。
    </p>
  );
}

export function UsageTrendChart({
  unit,
  buckets,
  rows,
  dimensionLabel,
  undatedRequestCount,
  stale,
}: UsageTrendChartProps) {
  const [width, attach] = useMeasuredWidth();
  const [active, setActive] = useState<number | null>(null);

  const colors = useMemo(() => assignSeriesColors(rows.map((row) => row.key)), [rows]);
  const totals = useMemo(
    () => buckets.map((_, index) => rows.reduce((sum, row) => sum + (row.tokens[index] ?? 0), 0)),
    [buckets, rows],
  );

  const axisMax = niceMax(Math.max(0, ...totals));
  const baseline = padTop + plotHeight;

  if (buckets.length === 0 || axisMax === 0) {
    return (
      <div className="rounded-xl border border-stone-200 bg-white p-3">
        <p className="text-[12px] font-semibold text-stone-800">{unitTitles[unit]}</p>
        <p className="mt-2 text-[12px] text-stone-500">当前筛选范围内没有数据。</p>
        {undatedRequestCount > 0 ? <UndatedNote count={undatedRequestCount} /> : null}
      </div>
    );
  }

  const band = Math.max(1, width - padLeft - padRight) / buckets.length;
  const barWidth = Math.max(2, Math.min(maxBarWidth, band - 4));
  // Every bar gets its total when there is room; otherwise only the peak does,
  // because a label per bar at this density overlaps its neighbours.
  const labelEveryBar = band >= minLabelBand;
  const peak = totals.indexOf(Math.max(...totals));
  const labelEvery = Math.max(1, Math.ceil(minLabelBand / band));

  const columns: Column[] = buckets.map((bucket, index) => {
    const bandLeft = padLeft + band * index;
    let cursor = baseline;
    const segments: Segment[] = [];
    for (const row of rows) {
      const tokens = row.tokens[index] ?? 0;
      if (tokens <= 0) {
        continue;
      }
      const height = (tokens / axisMax) * plotHeight;
      cursor -= height;
      segments.push({ key: row.key, tokens, top: cursor, height });
    }
    return {
      bucket,
      total: totals[index],
      bandLeft,
      barLeft: bandLeft + (band - barWidth) / 2,
      stackTop: cursor,
      segments,
    };
  });

  const activeColumn = active === null ? null : (columns[active] ?? null);

  return (
    <div
      className={`rounded-xl border border-stone-200 bg-white p-3 ${
        stale ? "opacity-60 transition-opacity" : ""
      }`}
    >
      <div className="flex flex-wrap items-baseline gap-x-2">
        <p className="text-[12px] font-semibold text-stone-800">{unitTitles[unit]}</p>
        <p className="text-[11px] text-stone-400">输入+输出 Token，按{dimensionLabel}堆叠</p>
      </div>
      <div className="relative mt-1" ref={attach}>
        <svg
          aria-label={`${unitTitles[unit]}，按${dimensionLabel}堆叠`}
          height={padTop + plotHeight + axisHeight}
          role="group"
          width={width}
        >
          {Array.from({ length: gridLines + 1 }, (_, index) => {
            const y = baseline - (plotHeight / gridLines) * index;
            return (
              <g key={index}>
                <line
                  stroke={index === 0 ? "#d6d3d1" : "#e7e5e4"}
                  x1={padLeft}
                  x2={width - padRight}
                  y1={y}
                  y2={y}
                />
                <text
                  dominantBaseline="middle"
                  fill="#a8a29e"
                  fontSize={10}
                  textAnchor="end"
                  x={padLeft - 6}
                  y={y}
                >
                  {formatCompactCount((axisMax / gridLines) * index)}
                </text>
              </g>
            );
          })}

          {columns.map((column, index) => (
            <g key={column.bucket.start}>
              {active === index ? (
                <rect
                  fill="#0b0b0b"
                  height={plotHeight}
                  opacity={0.04}
                  width={band}
                  x={column.bandLeft}
                  y={padTop}
                />
              ) : null}
              {column.segments.map((segment, segmentIndex) => {
                // The gap is shaved off the top of every segment but the last,
                // so the surface separates neighbours without a stroke around
                // them — and the data-end of the stack stays where the value is.
                const topmost = segmentIndex === column.segments.length - 1;
                const shave = topmost ? 0 : segmentGap;
                const height = Math.max(1, segment.height - shave);
                const color = colors.get(segment.key);
                return topmost ? (
                  <path
                    d={topRoundedPath(column.barLeft, segment.top, barWidth, height)}
                    fill={color}
                    key={segment.key}
                  />
                ) : (
                  <rect
                    fill={color}
                    height={height}
                    key={segment.key}
                    width={barWidth}
                    x={column.barLeft}
                    y={segment.top + shave}
                  />
                );
              })}
              {labelEveryBar || index === peak ? (
                <text
                  fill="#78716c"
                  fontSize={10}
                  textAnchor="middle"
                  x={column.bandLeft + band / 2}
                  y={column.stackTop - 5}
                >
                  {formatCompactCount(column.total)}
                </text>
              ) : null}
              {index % labelEvery === 0 ? (
                <text
                  fill="#a8a29e"
                  fontSize={10}
                  textAnchor="middle"
                  x={column.bandLeft + band / 2}
                  y={baseline + 13}
                >
                  {column.bucket.label}
                </text>
              ) : null}
            </g>
          ))}

          {columns.map((column, index) => (
            <rect
              aria-label={`${column.bucket.title}，${formatExactCount(column.total)} Token`}
              fill="transparent"
              height={baseline}
              key={column.bucket.start}
              onBlur={() => setActive(null)}
              onFocus={() => setActive(index)}
              onPointerEnter={() => setActive(index)}
              onPointerLeave={() =>
                setActive((current) => (current === index ? null : current))
              }
              tabIndex={0}
              width={band}
              x={column.bandLeft}
              y={0}
            />
          ))}
        </svg>
        {activeColumn ? (
          <TrendTooltip
            colors={colors}
            column={activeColumn}
            left={Math.min(
              Math.max(activeColumn.bandLeft + band / 2, 116),
              Math.max(width - 116, 116),
            )}
          />
        ) : null}
      </div>
      <ul className="mt-2 flex flex-wrap gap-x-3 gap-y-1">
        {rows.map((row) => {
          const total = row.tokens.reduce((sum, value) => sum + value, 0);
          return (
            <li
              className="flex items-center gap-1.5 text-[11px] text-stone-600"
              key={row.key}
              title={`${row.key}：${formatExactCount(total)} Token`}
            >
              <span
                aria-hidden="true"
                className="h-2 w-2 shrink-0 rounded-sm"
                style={{ backgroundColor: colors.get(row.key) }}
              />
              <span className="max-w-[12rem] truncate">{row.key}</span>
            </li>
          );
        })}
      </ul>
      {undatedRequestCount > 0 ? <UndatedNote count={undatedRequestCount} /> : null}
    </div>
  );
}
