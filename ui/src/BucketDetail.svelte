<script lang="ts">
  import LineChart from "./LineChart.svelte";
  import Modal from "./Modal.svelte";
  import {
    ledger,
    money,
    units,
    type BucketDetail,
    type BucketView,
    type BudgetView,
    type GoalView,
    type HoldingView,
    type ReconciliationView,
    type Series,
  } from "./ledger";

  type Mode = "add" | "spend" | "set" | "move";

  let {
    bucket,
    holdings = [],
    budget = [],
    goals = [],
    reconciliations = [],
    canMove,
    cannotDelete,
    busy,
    onaction,
    onedit,
    ondelete,
    onclose,
  }: {
    bucket: BucketView;
    holdings?: HoldingView[];
    budget?: BudgetView[];
    goals?: GoalView[];
    reconciliations?: ReconciliationView[];
    canMove: boolean;
    /** Why it cannot be deleted yet, or "" when it can. */
    cannotDelete: string;
    busy: boolean;
    onaction: (mode: Mode) => void;
    onedit: () => void;
    ondelete: () => void;
    onclose: () => void;
  } = $props();

  let detail = $state<BucketDetail | null>(null);
  let failed = $state("");

  // Read again whenever the bucket does: an add or a spend made from here
  // reloads the ledger, which hands this a new bucket.
  $effect(() => {
    const id = bucket.id;
    void bucket.total;
    failed = "";
    ledger
      .bucketDetail(id)
      .then((found) => (detail = found))
      .catch((e) => (failed = String(e)));
  });

  const held = $derived(
    holdings
      .filter((h) => h.bucketId === bucket.id)
      .slice()
      .sort((a, b) => Number(b.value) - Number(a.value)),
  );
  const funding = $derived(budget.filter((b) => b.bucketId === bucket.id));
  const linkedGoals = $derived(goals.filter((g) => g.bucketId === bucket.id));
  const openCharges = $derived(
    reconciliations
      .filter((r) => r.status !== "settled")
      .flatMap((r) =>
        r.lines.filter((l) => l.bucketId === bucket.id).map((l) => ({ ...l, card: r.card })),
      ),
  );

  const total = $derived(Number(bucket.total));
  const cashShare = $derived(
    total > 0 ? Math.max(0, Math.min(100, (Number(bucket.cash) / total) * 100)) : 0,
  );
  const makeup = $derived(
    bucket.invested === "0.00" && bucket.contributions === "0.00"
      ? "cash only"
      : bucket.cash === "0.00"
        ? "invested only"
        : "cash and investments",
  );

  const series = $derived.by((): Series[] => {
    const points = detail?.series ?? [];
    if (points.length < 2) return [];
    const rising = points[points.length - 1][1] >= points[0][1];
    return [
      {
        label: bucket.name,
        colour: rising ? "var(--positive)" : "var(--negative)",
        points,
        fill: true,
      },
    ];
  });

  const whenShortSays = $derived(
    bucket.whenShort === "bucket"
      ? `takes the rest from ${bucket.coverBucketName || "another bucket"}`
      : bucket.whenShort === "everyday"
        ? "charges the rest to everyday spending"
        : bucket.whenShort === "negative"
          ? "goes below zero"
          : "asks when settling",
  );

  function signed(value: string): string {
    return value.startsWith("-") ? `−${money(value.slice(1))}` : `+${money(value)}`;
  }

  /** A `yyyy-mm-dd` as the day it names here, not as midnight in London. */
  function day(date: string): string {
    const [y, m, d] = date.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  function stamp(iso: string): string {
    const at = new Date(iso);
    return Number.isNaN(at.getTime())
      ? iso
      : at.toLocaleString(undefined, {
          month: "short",
          day: "numeric",
          hour: "numeric",
          minute: "2-digit",
        });
  }

  function monthName(month: string): string {
    const [y, m] = month.split("-").map(Number);
    return new Date(y, m - 1, 1).toLocaleDateString(undefined, { month: "short", year: "numeric" });
  }

  function axisMoney(value: number): string {
    return value.toLocaleString(undefined, { maximumFractionDigits: 0 });
  }

  function axisDay(at: number): string {
    return new Date(at * 1000).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      timeZone: "UTC",
    });
  }

  function share(value: string): string {
    if (total <= 0) return "";
    return `${((Number(value) / total) * 100).toFixed(0)}%`;
  }
</script>

<Modal {onclose} wide lightDismiss label="Close {bucket.name}">
  <div class="detail">
    <header>
      <div class="title">
        <h2>{bucket.name}</h2>
        {#if bucket.locked}<span class="pill muted">locked</span>{/if}
        <span class="pill {makeup === 'cash only' ? 'green' : makeup === 'invested only' ? 'blue' : 'violet'}">
          {makeup}
        </span>
      </div>
      <div class="muted small" class:warn-text={!bucket.accountId}>
        Kept in {bucket.accountName || "no account"}{bucket.notes ? ` · ${bucket.notes}` : ""}
      </div>
    </header>

    <div class="actions">
      <button onclick={() => onaction("add")} disabled={busy}>+ Add</button>
      <button onclick={() => onaction("spend")} disabled={busy}>− Spend</button>
      <button
        onclick={() => onaction("move")}
        disabled={busy || !canMove}
        title={bucket.locked ? "This bucket is locked" : "Move to another bucket"}>→ Move</button
      >
      <button onclick={() => onaction("set")} disabled={busy}>= Set balance</button>
      <button class="bare" onclick={onedit} disabled={busy}>Edit</button>
      <button
        class="bare danger"
        onclick={ondelete}
        disabled={busy || cannotDelete !== ""}
        title={cannotDelete || "Delete"}>Delete</button
      >
    </div>

    <section class="tiles">
      <article class="tile">
        <div class="tile-label">Total</div>
        <div class="tile-figure" class:pos={total >= 0} class:neg={total < 0}>{money(bucket.total)}</div>
        <div class="tile-note">
          {#if bucket.target}
            {bucket.progress === null ? "" : `${bucket.progress.toFixed(0)}% of`} {money(bucket.target)}
          {:else}
            no target
          {/if}
        </div>
      </article>
      <article class="tile">
        <div class="tile-label">Growth a month</div>
        {#if detail?.growth}
          <div
            class="tile-figure"
            class:pos={!detail.growth.monthly.startsWith("-")}
            class:neg={detail.growth.monthly.startsWith("-")}
          >
            {signed(detail.growth.monthly)}
          </div>
          <div class="tile-note">
            {signed(detail.growth.delta)} since {day(detail.growth.fromAt)}, investments included
          </div>
        {:else}
          <div class="tile-figure">—</div>
          <div class="tile-note">needs a week of daily snapshots</div>
        {/if}
      </article>
      <article class="tile">
        <div class="tile-label">Added a month</div>
        <div class="tile-figure">{detail ? money(detail.addedMonthly) : "—"}</div>
        <div class="tile-note">
          {#if detail}
            averaged over {detail.historyDays} day{detail.historyDays === 1 ? "" : "s"}
          {/if}
          {#if bucket.fundedBy.length > 0} · budget plans {money(bucket.fundedMonthly)}{/if}
        </div>
      </article>
      <article class="tile">
        <div class="tile-label">Spent a month</div>
        <div class="tile-figure">{detail ? money(detail.spentMonthly) : "—"}</div>
        <div class="tile-note">
          {#if detail}
            {detail.movedMonthly === "0.00"
              ? "spends and settled statements"
              : `moves to other buckets ${signed(detail.movedMonthly)}`}
          {/if}
        </div>
      </article>
    </section>

    {#if failed}<p class="error">{failed}</p>{/if}

    <div class="makeup">
      <div class="makeup-bar" aria-hidden="true">
        <span class="makeup-cash" style:width="{cashShare}%"></span>
      </div>
      <div class="makeup-legend">
        <span><span class="dot cash"></span>Cash <strong>{money(bucket.cash)}</strong></span>
        <span><span class="dot invested"></span>Invested <strong>{money(bucket.invested)}</strong></span>
        {#if bucket.contributions !== "0.00"}
          <span class="muted">Roth contributions {money(bucket.contributions)}</span>
        {/if}
        {#if bucket.committed !== "0.00"}
          <span class:warn-text={bucket.committedShort !== null}>
            {money(bucket.committed)} on open statements{bucket.committedShort !== null
              ? ` · ${money(bucket.committedShort)} short`
              : ""}
          </span>
        {/if}
      </div>
    </div>

    {#if series.length > 0}
      <LineChart {series} yFormat={axisMoney} xFormat={axisDay} height={200} />
    {/if}

    <div class="columns">
      <section>
        <h3>Last month</h3>
        {#if !detail}
          <p class="note">{failed ? "" : "Loading…"}</p>
        {:else if detail.recent.length === 0}
          <p class="note">Nothing moved in or out since {day(detail.recentFrom)}.</p>
        {:else}
          <ul class="rows">
            {#each detail.recent as m (m.id)}
              <li class="row">
                <div class="row-main">
                  <div class="head">
                    <span class="pill {m.kind === 'spend' ? 'red' : m.kind === 'move' ? 'blue' : 'green'}">
                      {m.action.toLowerCase()}
                    </span>
                    <span class="row-name">{m.label}</span>
                  </div>
                  <div class="row-meta">
                    {stamp(m.at)}{m.actor ? ` · ${m.actor}` : ""} · left {money(m.balance)}
                  </div>
                </div>
                <span class="row-amount" class:pos={!m.delta.startsWith("-")} class:neg={m.delta.startsWith("-")}>
                  {signed(m.delta)}
                </span>
              </li>
            {/each}
          </ul>
        {/if}

        {#if detail && detail.months.length > 1}
          <h3>Month by month</h3>
          <table>
            <thead><tr><th>Month</th><th>Ended at</th><th>Change</th></tr></thead>
            <tbody>
              {#each [...detail.months].reverse() as month (month.month)}
                <tr>
                  <td>{monthName(month.month)}</td>
                  <td>{money(month.end)}</td>
                  <td class:pos={month.change !== null && !month.change.startsWith("-")} class:neg={month.change?.startsWith("-")}>
                    {month.change === null ? "—" : signed(month.change)}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>

      <section>
        <h3>Investments in its name</h3>
        {#if held.length === 0}
          <p class="note">None. Give a holding this bucket on the Holdings page to count it here.</p>
        {:else}
          <ul class="rows">
            {#each held as h (h.id)}
              <li class="row">
                <div class="row-main">
                  <div class="head">
                    {#if h.ticker}<span class="pill violet">{h.ticker}</span>{/if}
                    <span class="row-name">{h.name}</span>
                  </div>
                  <div class="row-meta">
                    {units(h.quantity)} units{h.accountName ? ` · ${h.accountName}` : ""}
                    {#if h.gain !== null}
                      · <span class:pos={Number(h.gain) >= 0} class:neg={Number(h.gain) < 0}>
                        {signed(h.gain)}{h.gainPercent === null ? "" : ` (${h.gainPercent.toFixed(1)}%)`}
                      </span>
                    {/if}
                  </div>
                </div>
                <span class="row-amount">
                  {money(h.value)} <span class="muted small">{share(h.value)}</span>
                </span>
              </li>
            {/each}
          </ul>
        {/if}

        <h3>Filled by the budget</h3>
        {#if funding.length === 0}
          <p class="note">No budget line feeds this bucket.</p>
        {:else}
          <ul class="plain">
            {#each funding as line (line.id)}
              <li><span>{line.name}</span><span>{money(line.monthlyAmount)}/mo</span></li>
            {/each}
          </ul>
        {/if}

        {#if linkedGoals.length > 0}
          <h3>Goals it saves for</h3>
          <ul class="plain">
            {#each linkedGoals as goal (goal.id)}
              <li>
                <span>{goal.name}</span>
                <span>{goal.target ? `${goal.progress?.toFixed(0) ?? 0}% of ${money(goal.target)}` : "no target"}</span>
              </li>
            {/each}
          </ul>
        {/if}

        {#if openCharges.length > 0}
          <h3>Waiting on open statements</h3>
          <ul class="plain">
            {#each openCharges as charge (charge.id)}
              <li>
                <span>{charge.label} <span class="muted small">{charge.card}{charge.spentOn ? ` · ${charge.spentOn}` : ""}</span></span>
                <span>{money(charge.amount)}</span>
              </li>
            {/each}
          </ul>
        {/if}

        <h3>Rules</h3>
        <ul class="plain">
          <li><span>When short</span><span>{whenShortSays}</span></li>
          <li><span>Moving money out</span><span>{bucket.locked ? "locked" : "allowed"}</span></li>
        </ul>
      </section>
    </div>
  </div>
</Modal>

<style>
  .detail {
    display: grid;
    gap: 0.75rem;
  }
  header {
    padding-right: 2rem;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
  }
  h3 {
    margin: 0.9rem 0 0.35rem;
    font-size: 0.78rem;
    letter-spacing: 0.02em;
    color: var(--dim);
  }
  section > h3:first-child {
    margin-top: 0;
  }
  .small {
    font-size: 0.72rem;
  }
  .actions {
    flex-wrap: wrap;
  }
  .tiles {
    margin-bottom: 0;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
  }
  .makeup {
    display: grid;
    gap: 0.35rem;
  }
  .makeup-bar {
    height: 6px;
    border-radius: 3px;
    background: var(--pill-blue);
    overflow: hidden;
  }
  .makeup-cash {
    display: block;
    height: 100%;
    background: var(--positive);
  }
  .makeup-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem 1.2rem;
    font-size: 0.75rem;
  }
  .dot {
    display: inline-block;
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    margin-right: 0.35rem;
  }
  .dot.cash {
    background: var(--positive);
  }
  .dot.invested {
    background: var(--pill-blue);
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr));
    gap: 1.25rem;
    align-items: start;
  }
  .columns section {
    min-width: 0;
  }
  /* Half a pop-up is too narrow to cut a holding's details off at one line. */
  .columns :global(.row-meta) {
    white-space: normal;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }
  .plain {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.3rem;
    font-size: 0.78rem;
  }
  .plain li {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid var(--hairline);
    padding-bottom: 0.3rem;
  }
  .plain li > span:last-child {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }
  th {
    text-align: left;
    font-weight: 500;
    color: var(--dim);
  }
  th,
  td {
    padding: 0.3rem 0.2rem;
    border-bottom: 1px solid var(--hairline);
  }
  th:not(:first-child),
  td:not(:first-child) {
    text-align: right;
  }
</style>
