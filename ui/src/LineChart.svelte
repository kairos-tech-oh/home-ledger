<script lang="ts">
  import type { Series } from "./ledger";

  let {
    series,
    height = 240,
    yFormat = (v: number) => String(v),
    xFormat = (v: number) => String(v),
    baseline = null,
  }: {
    series: Series[];
    height?: number;
    yFormat?: (v: number) => string;
    xFormat?: (v: number) => string;
    baseline?: number | null;
  } = $props();

  const pad = { top: 8, right: 8, bottom: 20, left: 62 };
  const width = 1000;

  let hoverX = $state<number | null>(null);

  const bounds = $derived.by(() => {
    const xs: number[] = [];
    const ys: number[] = baseline === null ? [] : [baseline];
    for (const s of series) {
      for (const [x, y] of s.points) {
        xs.push(x);
        ys.push(y);
      }
    }
    if (xs.length === 0) return null;
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    let minY = Math.min(...ys);
    let maxY = Math.max(...ys);
    if (minY === maxY) {
      minY -= 1;
      maxY += 1;
    }
    const room = (maxY - minY) * 0.08;
    const floor = minY - room;
    return {
      minX,
      maxX: maxX === minX ? minX + 1 : maxX,
      // A chart of money that never goes negative should not show a negative
      // floor just because the padding reached past zero.
      minY: minY >= 0 ? Math.max(0, floor) : floor,
      maxY: maxY + room,
    };
  });

  function sx(x: number): number {
    if (!bounds) return pad.left;
    const span = bounds.maxX - bounds.minX;
    return pad.left + ((x - bounds.minX) / span) * (width - pad.left - pad.right);
  }

  function sy(y: number): number {
    if (!bounds) return pad.top;
    const span = bounds.maxY - bounds.minY;
    return height - pad.bottom - ((y - bounds.minY) / span) * (height - pad.top - pad.bottom);
  }

  function path(points: [number, number][]): string {
    return points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${sx(x)},${sy(y)}`).join(" ");
  }

  function area(points: [number, number][]): string {
    if (points.length === 0) return "";
    const floor = height - pad.bottom;
    return `${path(points)} L${sx(points[points.length - 1][0])},${floor} L${sx(points[0][0])},${floor} Z`;
  }

  /// Four lines is enough to read a scale by and few enough to stay quiet.
  const ticks = $derived.by(() => {
    if (!bounds) return [];
    const steps = 4;
    return Array.from({ length: steps + 1 }, (_, i) => {
      const value = bounds.minY + ((bounds.maxY - bounds.minY) * i) / steps;
      return { value, y: sy(value) };
    });
  });

  const xTicks = $derived.by(() => {
    if (!bounds) return [];
    const steps = 4;
    return Array.from({ length: steps + 1 }, (_, i) => {
      const value = bounds.minX + ((bounds.maxX - bounds.minX) * i) / steps;
      return { value, x: sx(value) };
    });
  });

  /// The nearest real point on each line, so the readout never invents a value
  /// between two days.
  const readout = $derived.by(() => {
    const at = hoverX;
    if (at === null || !bounds) return null;
    const rows = series
      .map((s) => {
        let best: [number, number] | null = null;
        let gap = Infinity;
        for (const p of s.points) {
          const d = Math.abs(p[0] - at);
          if (d < gap) {
            gap = d;
            best = p;
          }
        }
        return best ? { label: s.label, colour: s.colour, x: best[0], y: best[1] } : null;
      })
      .filter((r) => r !== null);
    return rows.length > 0 ? rows : null;
  });

  function track(event: MouseEvent) {
    if (!bounds) return;
    const box = (event.currentTarget as SVGElement).getBoundingClientRect();
    const at = ((event.clientX - box.left) / box.width) * width;
    const ratio = (at - pad.left) / (width - pad.left - pad.right);
    hoverX = bounds.minX + Math.min(Math.max(ratio, 0), 1) * (bounds.maxX - bounds.minX);
  }
</script>

{#if bounds}
  <div class="chart">
    <svg
      viewBox="0 0 {width} {height}"
      preserveAspectRatio="none"
      role="img"
      aria-label="price history"
      onmousemove={track}
      onmouseleave={() => (hoverX = null)}
    >
      {#each ticks as tick (tick.value)}
        <line class="grid" x1={pad.left} x2={width - pad.right} y1={tick.y} y2={tick.y} />
        <text class="axis" x={pad.left - 8} y={tick.y + 4} text-anchor="end">
          {yFormat(tick.value)}
        </text>
      {/each}

      {#each xTicks as tick, i (tick.value)}
        <text
          class="axis"
          x={tick.x}
          y={height - 6}
          text-anchor={i === 0 ? "start" : i === xTicks.length - 1 ? "end" : "middle"}
        >
          {xFormat(tick.value)}
        </text>
      {/each}

      {#if baseline !== null}
        <line
          class="baseline"
          x1={pad.left}
          x2={width - pad.right}
          y1={sy(baseline)}
          y2={sy(baseline)}
        />
      {/if}

      {#each series as s (s.label)}
        {#if s.fill}
          <path d={area(s.points)} fill={s.colour} opacity="0.12" />
        {/if}
        <path d={path(s.points)} fill="none" stroke={s.colour} stroke-width="2" />
      {/each}

      {#if readout}
        <line class="crosshair" x1={sx(readout[0].x)} x2={sx(readout[0].x)} y1={pad.top} y2={height - pad.bottom} />
        {#each readout as row (row.label)}
          <circle cx={sx(row.x)} cy={sy(row.y)} r="3" fill={row.colour} />
        {/each}
      {/if}
    </svg>

    {#if readout}
      <div class="readout">
        <span class="at">{xFormat(readout[0].x)}</span>
        {#each readout as row (row.label)}
          <span class="value" style:color={row.colour}>
            {#if series.length > 1}{row.label}{/if}
            {yFormat(row.y)}
          </span>
        {/each}
      </div>
    {/if}
  </div>
{:else}
  <p class="empty">Nothing to plot.</p>
{/if}

<style>
  .chart {
    position: relative;
  }
  svg {
    width: 100%;
    display: block;
    height: auto;
  }
  .grid {
    stroke: var(--hairline);
    stroke-width: 1;
  }
  .baseline {
    stroke: var(--faint);
    stroke-width: 1;
    stroke-dasharray: 4 4;
  }
  .crosshair {
    stroke: var(--faint);
    stroke-width: 1;
  }
  .axis {
    fill: var(--faint);
    font-size: 11px;
    font-family: inherit;
  }
  .readout {
    position: absolute;
    top: 0;
    right: 0;
    display: flex;
    gap: 0.6rem;
    align-items: baseline;
    background: rgba(var(--ground), 0.85);
    border-radius: 0.3rem;
    padding: 0.1rem 0.4rem;
    font-size: 0.7rem;
    pointer-events: none;
  }
  .at {
    color: var(--dim);
  }
  .value {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
</style>
