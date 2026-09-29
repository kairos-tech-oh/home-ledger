<script lang="ts">
  import LineChart from "./LineChart.svelte";
  import {
    chartRanges,
    ledger,
    money,
    units,
    when,
    type Detail,
    type HoldingView,
    type Series,
  } from "./ledger";

  let { holding, onclose }: { holding: HoldingView; onclose: () => void } = $props();

  let range = $state<string>("1y");
  let detail = $state<Detail | null>(null);
  let loading = $state(false);
  let failed = $state("");

  const lookupable = $derived(/^[A-Za-z0-9][A-Za-z0-9.:-]{0,15}$/.test(holding.ticker));

  $effect(() => {
    const want = range;
    if (!lookupable) return;
    loading = true;
    failed = "";
    ledger
      .detail(holding.ticker, want, false)
      .then((found) => {
        detail = found;
      })
      .catch((e) => {
        failed = String(e);
      })
      .finally(() => {
        loading = false;
      });
  });

  async function refresh() {
    if (!lookupable) return;
    loading = true;
    failed = "";
    try {
      detail = await ledger.detail(holding.ticker, range, true);
    } catch (e) {
      failed = String(e);
    } finally {
      loading = false;
    }
  }

  const chart = $derived(detail?.chart ?? null);
  const first = $derived(chart?.points[0]?.[1] ?? null);
  const last = $derived(chart?.points[chart.points.length - 1]?.[1] ?? null);
  const rising = $derived(first !== null && last !== null && last >= first);

  const series = $derived.by((): Series[] => {
    if (!chart) return [];
    return [
      {
        label: holding.ticker,
        colour: rising ? "var(--positive)" : "var(--negative)",
        points: chart.points.map(([at, close]) => [at, close] as [number, number]),
        fill: true,
      },
    ];
  });

  const move = $derived.by(() => {
    if (first === null || last === null || first === 0) return null;
    return { amount: last - first, percent: ((last - first) / first) * 100 };
  });

  /// What the position itself did over the window, which is the figure that
  /// matters here: the ticker's move times what is actually held.
  const positionMove = $derived.by(() => {
    if (!move) return null;
    return move.amount * Number(holding.quantity);
  });

  const trades = $derived([...holding.trades].reverse());

  /// Zero is what an unrecorded basis looks like once it has been through the
  /// app, and `holding_gain` already refuses to treat it as a real cost.
  const knownCost = $derived(holding.costBasis !== null && Number(holding.costBasis) !== 0);

  /// The ledger keeps its own price, refreshed when someone asks it to. Worth
  /// saying when the market has moved away from it since.
  const drifted = $derived.by(() => {
    const held = holding.price === null ? null : Number(holding.price);
    const live = chart?.price ?? null;
    if (held === null || live === null || held === 0 || holding.fixedPrice) return null;
    const gap = (live - held) / held;
    return Math.abs(gap) < 0.005 ? null : { live, percent: gap * 100 };
  });

  function price(value: number | null): string {
    if (value === null) return "—";
    return value.toLocaleString(undefined, {
      style: "currency",
      currency: chart?.currency || "USD",
      minimumFractionDigits: 2,
      maximumFractionDigits: value < 10 ? 4 : 2,
    });
  }

  function day(at: number): string {
    const d = new Date(at * 1000);
    if (range === "5d") {
      return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric" });
    }
    return d.toLocaleDateString(undefined, {
      year: range === "5y" ? "numeric" : undefined,
      month: "short",
      day: "numeric",
    });
  }

  function axisPrice(value: number): string {
    return value.toLocaleString(undefined, {
      maximumFractionDigits: value < 10 ? 2 : 0,
    });
  }
</script>

<div class="section-head">
  <h2>
    {#if holding.ticker}<span class="pill violet">{holding.ticker}</span>{/if}
    {holding.name}
  </h2>
  <div class="controls">
    {#if lookupable}
      <div class="toggle">
        {#each chartRanges as option (option.value)}
          <button class:on={range === option.value} onclick={() => (range = option.value)}>
            {option.label}
          </button>
        {/each}
      </div>
      <button onclick={refresh} disabled={loading}>{loading ? "…" : "Refresh"}</button>
    {/if}
    <button onclick={onclose}>← Holdings</button>
  </div>
</div>

<section class="tiles">
  <article class="tile">
    <div class="tile-label">Position</div>
    <div class="tile-figure pos">{money(holding.value)}</div>
    <div class="tile-note">{units(holding.quantity)} at {money(holding.price)}</div>
  </article>
  <article class="tile">
    <div class="tile-label">Cost</div>
    <div class="tile-figure">{knownCost ? money(holding.costBasis) : "—"}</div>
    <div class="tile-note">
      {knownCost ? `${money(holding.avgCost)} a unit` : "never recorded"}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Gain</div>
    <div
      class="tile-figure"
      class:pos={holding.gain !== null && Number(holding.gain) >= 0}
      class:neg={holding.gain !== null && Number(holding.gain) < 0}
    >
      {money(holding.gain)}
    </div>
    <div class="tile-note">
      {holding.gainPercent === null ? "against an unknown cost" : `${holding.gainPercent.toFixed(1)}% since bought`}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">This {chartRanges.find((r) => r.value === range)?.label ?? "window"}</div>
    {#if move}
      <div class="tile-figure" class:pos={move.amount >= 0} class:neg={move.amount < 0}>
        {move.percent >= 0 ? "+" : ""}{move.percent.toFixed(2)}%
      </div>
      <div class="tile-note">
        {positionMove === null
          ? ""
          : `${positionMove >= 0 ? "+" : "−"}${money(Math.abs(positionMove).toFixed(2))} on what is held`}
      </div>
    {:else}
      <div class="tile-figure">—</div>
      <div class="tile-note">no price history</div>
    {/if}
  </article>
</section>

{#if failed}<p class="error">{failed}</p>{/if}

{#if !lookupable}
  <p class="note">
    {holding.ticker
      ? `"${holding.ticker}" is not a symbol anything can be looked up by.`
      : "This holding has no ticker, so there is nothing to chart. Its figures above come from the ledger."}
  </p>
{:else if chart}
  {#if detail?.error}
    <p class="warn-box">
      {detail.error} — showing the last copy that loaded{chart.fetchedAt
        ? `, from ${when(new Date(chart.fetchedAt * 1000).toISOString())}`
        : ""}.
    </p>
  {/if}

  <LineChart {series} yFormat={axisPrice} xFormat={day} baseline={first} height={260} />

  <div class="facts">
    {#if chart.name}<span><span class="muted">Name</span> {chart.name}</span>{/if}
    {#if chart.exchange}<span><span class="muted">Exchange</span> {chart.exchange}</span>{/if}
    {#if chart.instrument}<span><span class="muted">Type</span> {chart.instrument}</span>{/if}
    <span><span class="muted">Last</span> {price(chart.price)}</span>
    {#if chart.dayLow !== null || chart.dayHigh !== null}
      <span><span class="muted">Day</span> {price(chart.dayLow)} – {price(chart.dayHigh)}</span>
    {/if}
    {#if chart.yearLow !== null || chart.yearHigh !== null}
      <span><span class="muted">52 week</span> {price(chart.yearLow)} – {price(chart.yearHigh)}</span>
    {/if}
    {#if chart.currency}<span><span class="muted">Currency</span> {chart.currency}</span>{/if}
  </div>
{:else if loading}
  <p class="note">Loading {holding.ticker}…</p>
{/if}

<div class="section-head">
  <h2>In the ledger</h2>
</div>

<div class="facts">
  <span><span class="muted">Kind</span> {holding.kind || "—"}</span>
  <span><span class="muted">Account</span> {holding.accountName || "none"}</span>
  {#if holding.bucketName}<span><span class="muted">Bucket</span> {holding.bucketName}</span>{/if}
  <span>
    <span class="muted">Price</span>
    {#if holding.fixedPrice}
      fixed at {money(holding.price)}
    {:else if holding.price === null}
      never fetched, valued at cost
    {:else}
      {money(holding.price)}{holding.priceAt ? ` on ${when(holding.priceAt)}` : ""}
      {#if holding.priceStale}<span class="warn-text">stale</span>{/if}
      {#if drifted}
        <span class="warn-text">
          market {price(drifted.live)}, {drifted.percent >= 0 ? "+" : ""}{drifted.percent.toFixed(
            1,
          )}%
        </span>
      {/if}
    {/if}
  </span>
</div>

{#if trades.length > 0}
  <div class="section-head">
    <h2>{trades.length} trade{trades.length === 1 ? "" : "s"}</h2>
  </div>
  <ul class="rows">
    {#each trades as trade, i (i)}
      <li class="row">
        <div class="row-main">
          <div class="head">
            <span class="pill {trade.kind === 'sell' ? 'red' : 'green'}">{trade.kind || "buy"}</span>
            <span class="row-name">{units(trade.quantity)} at {money(trade.price)}</span>
          </div>
          <div class="row-meta">{trade.at || "no date"}{trade.notes ? ` · ${trade.notes}` : ""}</div>
        </div>
        <span class="row-amount">{money(trade.total)}</span>
      </li>
    {/each}
  </ul>
{:else}
  <p class="note">No trades recorded against this holding.</p>
{/if}

<style>
  .controls {
    display: flex;
    gap: 0.6rem;
  }
  .section-head h2 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1.4rem;
    font-size: 0.75rem;
    padding: 0.5rem 0.1rem;
  }
  .facts .muted {
    margin-right: 0.3rem;
  }
</style>
