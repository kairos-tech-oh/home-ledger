<script lang="ts">
  import { ledger, wholeMoney, type ProjectionView } from "./ledger";

  const colours = ["#7c9ef8", "#34d399", "#fbbf24", "#f472b6"];
  const monthNames = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const padTop = 10;
  const padBottom = 30;
  const padRight = 52;
  const height = 180;

  let projection = $state<ProjectionView | null>(null);
  let failed = $state("");
  let width = $state(0);
  let hoverX = $state(-1);
  let dateW = $state(0);

  $effect(() => {
    ledger
      .projection()
      .then((found) => {
        projection = found;
        failed = "";
      })
      .catch((e) => (failed = String(e)));
  });

  const years = $derived(projection ? Number(projection.years) : 0);
  const plotW = $derived(Math.max(0, width - padRight));
  const plotH = $derived(height - padTop - padBottom);
  const ceiling = $derived.by(() => {
    let top = 0;
    for (const line of projection?.lines ?? [])
      for (const p of line.points) top = Math.max(top, Number(p.value));
    return top;
  });
  const drawn = $derived(projection !== null && years > 0 && ceiling > 0 && plotW > 0);
  const hovering = $derived(drawn && hoverX >= 0 && hoverX <= plotW);
  const hoverMonths = $derived(
    hovering && projection
      ? Math.min(projection.months, Math.round((hoverX / plotW) * years * 12))
      : 0,
  );
  const hoverDate = $derived.by(() => {
    if (!hovering || !projection) return "";
    const month = projection.startMonth + hoverMonths;
    return `${monthNames[month % 12]} ${Math.floor(month / 12)}`;
  });
  const header = $derived(
    hovering
      ? `Projection to ${hoverDate}`
      : projection?.targetYear
        ? `Projection to ${projection.targetYear}`
        : "Projection",
  );
  const cards = $derived(
    (projection?.lines ?? []).map((line) => {
      const at = hovering ? line.months[hoverMonths] : line;
      return { rate: line.rate, value: at.value, contributed: at.contributed, growth: at.growth };
    }),
  );
  const columns = $derived(Math.max(1, Math.floor(width / 180)));

  function x(month: number): number {
    return (month / 12 / years) * plotW;
  }
  function y(value: number): number {
    return padTop + (1 - value / ceiling) * plotH;
  }
  function path(points: { month: number; value: string }[]): string {
    return points.map((p, i) => `${i ? "L" : "M"}${x(p.month)},${y(Number(p.value))}`).join(" ");
  }
</script>

<div class="projection">
  <h2 class="header">{header}</h2>

  {#if failed}<p class="error">{failed}</p>{/if}

  <div class="chart" style:height="{height}px" bind:clientWidth={width}>
    {#if drawn && projection}
      <svg {width} {height} aria-hidden="true">
        <line class="grid" x1="0" x2={plotW} y1={padTop + plotH} y2={padTop + plotH} />
        {#if Number(projection.start) > 0}
          <line
            class="grid"
            stroke-dasharray="3 4"
            x1="0"
            x2={plotW}
            y1={y(Number(projection.start))}
            y2={y(Number(projection.start))}
          />
        {/if}
        {#each projection.lines as line, i (line.rate)}
          <path d={path(line.points)} stroke={colours[i % colours.length]} />
        {/each}
        {#if hovering}
          <line
            class="crosshair"
            stroke-dasharray="3 3"
            x1={hoverX}
            x2={hoverX}
            y1={padTop}
            y2={padTop + plotH}
          />
        {/if}
      </svg>

      <div
        class="hover"
        role="presentation"
        style:top="{padTop}px"
        style:width="{plotW}px"
        style:height="{plotH}px"
        onmousemove={(e) => (hoverX = e.offsetX)}
        onmouseleave={() => (hoverX = -1)}
      ></div>

      {#each projection.lines as line, i (line.rate)}
        <span
          class="rate-label"
          style:left="{plotW + 6}px"
          style:top="{y(Number(line.value))}px"
          style:color={colours[i % colours.length]}>{line.rate}%</span
        >
      {/each}

      <span class="axis" style:left="0" style:opacity={hovering ? 0 : 1}>now</span>
      <span class="axis end" style:right="{padRight}px" style:opacity={hovering ? 0 : 1}
        >{projection.targetYear ?? ""}</span
      >

      {#if hovering}
        <span
          class="date"
          bind:offsetWidth={dateW}
          style:top="{height - padBottom + 3}px"
          style:left="{Math.max(0, Math.min(plotW - dateW, hoverX - dateW / 2))}px"
          >{hoverDate}</span
        >
      {/if}
    {:else if projection}
      <p class="empty-chart">Set a target retirement year to project forward</p>
    {/if}
  </div>

  {#if projection && years > 0}
    <div class="rate-cards" style:grid-template-columns="repeat({columns}, 1fr)">
      {#each cards as card, i (card.rate)}
        <div class="rate-card">
          <div class="rate" style:color={colours[i % colours.length]}>{card.rate}% A YEAR</div>
          <div class="value">{wholeMoney(card.value)}</div>
          <div class="split">
            {wholeMoney(card.contributed)} paid in · {wholeMoney(card.growth)} growth
          </div>
        </div>
      {/each}
    </div>

    <p class="footnote">
      From {wholeMoney(projection.start)} today with {wholeMoney(projection.monthly)} a month
      going in. Contributions are held flat: no raise, no rebalance, no employer match beyond
      what you record.
    </p>
  {/if}
</div>

<style>
  .projection {
    display: grid;
    gap: 8px;
    margin-top: 0.75rem;
  }
  .header {
    font-size: 10px;
    font-weight: 700;
    margin: 0;
  }
  .chart {
    position: relative;
  }
  svg {
    position: absolute;
    inset: 0;
    overflow: visible;
  }
  path {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
  }
  .grid {
    stroke: rgba(var(--ink), 0.12);
    stroke-width: 1;
  }
  .crosshair {
    stroke: rgba(var(--ink), 0.48);
    stroke-width: 1;
  }
  .hover {
    position: absolute;
    left: 0;
  }
  .rate-label {
    position: absolute;
    transform: translateY(-50%);
    font-size: 10px;
    font-weight: 700;
    pointer-events: none;
  }
  .axis {
    position: absolute;
    bottom: 0;
    font-size: 10px;
    color: var(--faint);
    transition: opacity 0.12s;
    pointer-events: none;
  }
  .date {
    position: absolute;
    padding: 3px 6px;
    border-radius: 3px;
    background: rgb(var(--ground));
    border: 1px solid var(--accent);
    color: var(--text);
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
    pointer-events: none;
  }
  .empty-chart {
    position: absolute;
    inset: 0;
    margin: auto 20px;
    height: fit-content;
    text-align: center;
    font-size: 11px;
    color: var(--faint);
  }
  .rate-cards {
    display: grid;
    gap: 8px;
  }
  .rate-card {
    border-radius: 5px;
    background: rgba(var(--ink), 0.045);
    padding: 8px;
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .rate {
    font-size: 10px;
    font-weight: 700;
  }
  .value {
    font-size: 16px;
    font-weight: 700;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .split {
    font-size: 10px;
    color: var(--dim);
  }
  .footnote {
    font-size: 10px;
    color: var(--faint);
    margin: 0;
  }
</style>
