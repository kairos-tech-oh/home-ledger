<script lang="ts">
  import { kindLabel, money, wholeMoney, type WidgetView } from "./ledger";

  // One dashboard card. Every figure arrives worked out; this only draws it.
  let {
    widget,
    editing,
    first,
    last,
    onmove,
    onspan,
    onedit,
    onremove,
    onopen,
  }: {
    widget: WidgetView;
    editing: boolean;
    first: boolean;
    last: boolean;
    onmove: (delta: -1 | 1) => void;
    onspan: () => void;
    onedit: () => void;
    onremove: () => void;
    onopen: (page: string) => void;
  } = $props();

  const f = $derived(widget.figures);
  const title = $derived(
    (widget.title || widget.label).toUpperCase() +
      (widget.title ? `  ·  ${widget.label.toUpperCase()}` : ""),
  );

  const negative = (v: string) => v.startsWith("-");
  const unsigned = (v: string) => v.replace(/^-/, "");
  const signed = (v: string) => `${negative(v) ? "−" : "+"}${wholeMoney(unsigned(v))}`;
  const pct = (n: number) => `${n.toFixed(1)}%`;

  /** A sparkline path over 0..1 values, oldest first. */
  function spark(values: number[]): string {
    if (values.length < 2) return "";
    const step = 100 / (values.length - 1);
    return values.map((v, i) => `${(i * step).toFixed(2)},${(28 - v * 26).toFixed(2)}`).join(" ");
  }

  function pickNote(kind: "buckets" | "accounts" | "goals"): string {
    return editing ? `Choose ${kind} with Edit.` : `No ${kind} chosen. Customize the dashboard to pick some.`;
  }
</script>

<article class="widget" class:editing>
  <header>
    <span class="title">{title}</span>
    <span class="actions">
      {#if editing}
        <button class="bare" disabled={first} title="Move earlier" onclick={() => onmove(-1)}>‹</button>
        <button class="bare" disabled={last} title="Move later" onclick={() => onmove(1)}>›</button>
        <button
          class="bare"
          title={widget.span === 2 ? "Take half a row" : "Take the whole row"}
          onclick={onspan}>{widget.span === 2 ? "Half" : "Full"}</button
        >
        <button class="bare" onclick={onedit}>Edit</button>
        <button class="bare danger" title="Remove from the dashboard" onclick={onremove}>×</button>
      {:else if widget.page}
        <button class="bare" title="Open the page behind this" onclick={() => onopen(widget.page)}>→</button>
      {/if}
    </span>
  </header>

  {#if f.type === "networth"}
    <div class="split">
      <div>
        <div class="display" class:neg={negative(f.net)} class:pos={!negative(f.net)}>{money(f.net)}</div>
        <div class="caption">{wholeMoney(f.assets)} held · {wholeMoney(f.debts)} owed</div>
      </div>
      <div class="trend">
        {#if f.change !== null}
          <div class="caption strong" class:neg={negative(f.change)} class:pos={!negative(f.change)}>
            {signed(f.change)} over 30 days
          </div>
        {:else}
          <div class="caption faint">History builds a point a day</div>
        {/if}
        {#if f.history.length > 1}
          <svg viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
            <polyline
              points={spark(f.history)}
              class:neg={f.change !== null && negative(f.change)}
              vector-effect="non-scaling-stroke"
            />
          </svg>
        {/if}
      </div>
    </div>
  {:else if f.type === "rows" && widget.kind === "buckets"}
    {#if f.rows.length === 0}<p class="faint">{pickNote("buckets")}</p>{/if}
    {#each f.rows as row (row.id)}
      <div class="item">
        <div class="line">
          <span class="name">{row.name}</span>
          <span>{wholeMoney(row.value)}{row.target ? ` of ${wholeMoney(row.target)}` : ""}</span>
        </div>
        {#if row.percent !== null}
          <span class="track"><span class="fill" class:done={row.percent >= 100} style:width="{row.percent}%"></span></span>
        {/if}
      </div>
    {/each}
    {#if f.total !== null}<div class="caption right">{wholeMoney(f.total)} across these</div>{/if}
  {:else if f.type === "rows" && widget.kind === "accounts"}
    {#if f.rows.length === 0}<p class="faint">{pickNote("accounts")}</p>{/if}
    {#each f.rows as row (row.id)}
      <div class="line item">
        <span class="stack">
          <span class="name">{row.name}</span>
          <span class="caption">
            {kindLabel(row.kind)}{row.available !== null ? ` · ${wholeMoney(row.available)} available` : ""}
          </span>
        </span>
        <span class="strong" class:neg={row.owes && Number(row.value) > 0}>
          {wholeMoney(row.value)}{row.owes ? " owed" : ""}
        </span>
      </div>
    {/each}
  {:else if f.type === "rows" && widget.kind === "goals"}
    {#if f.rows.length === 0}<p class="faint">{pickNote("goals")}</p>{/if}
    {#each f.rows as row (row.id)}
      <div class="item">
        <div class="line">
          <span class="name">{row.name}</span>
          <span class="strong" class:pos={row.percent !== null && row.percent >= 100}>
            {row.percent === null ? "no target" : pct(row.percent)}
          </span>
        </div>
        {#if row.percent !== null}
          <span class="track"><span class="fill" class:done={row.percent >= 100} style:width="{row.percent}%"></span></span>
        {/if}
        <div class="caption">
          {wholeMoney(row.value)}{row.target ? ` of ${wholeMoney(row.target)}` : ""}{row.note ? ` · ${row.note}` : ""}
        </div>
      </div>
    {/each}
  {:else if f.type === "retirement"}
    <div class="display">{money(f.total)}</div>
    {#if Number(f.total) > 0}
      <div class="bar2">
        <span class="roth" style:width="{f.rothShare * 100}%"></span>
        <span class="trad" style:width="{(1 - f.rothShare) * 100}%"></span>
      </div>
    {/if}
    <div class="figures">
      <div><span class="label">Roth</span><span class="pos">{wholeMoney(f.roth)}</span></div>
      <div><span class="label">Traditional</span><span class="accent">{wholeMoney(f.traditional)}</span></div>
    </div>
    {#if Number(f.rothContributions) > 0}
      <div class="caption">
        {wholeMoney(f.rothContributions)} of Roth contributions, which come out without penalty
      </div>
    {/if}
  {:else if f.type === "reconciliation"}
    <div class="headline" class:neg={f.daysSince !== null && f.daysSince > 14}>
      {f.daysSince === null
        ? "Nothing reconciled yet"
        : f.daysSince === 0
          ? "Reconciled today"
          : `Last reconciled ${f.daysSince} day${f.daysSince === 1 ? "" : "s"} ago`}
    </div>
    <div class="figures three">
      <div>
        <span class="label">Open</span>
        <span class:accent={f.open > 0}>{f.open === 0 ? "None" : f.open}</span>
      </div>
      <div><span class="label">Spending {f.year}</span><span>{wholeMoney(f.yearSpending)}</span></div>
      <div><span class="label">From buckets</span><span>{wholeMoney(f.yearBuckets)}</span></div>
    </div>
    {#if f.open > 0}<div class="caption accent">{wholeMoney(f.openBalance)} still to account for</div>{/if}
  {:else if f.type === "cashflow"}
    <div class="figures three">
      <div><span class="label">Income</span><span>{wholeMoney(f.income)}</span></div>
      <div><span class="label">Budgeted</span><span>{wholeMoney(f.budgeted)}</span></div>
      <div>
        <span class="label">{negative(f.left) ? "Over" : "Left"}</span>
        <span class:neg={negative(f.left)} class:pos={!negative(f.left)}>{wholeMoney(unsigned(f.left))}</span>
      </div>
    </div>
    <span class="track">
      <span class="fill" class:over={negative(f.left)} style:width="{Math.min(f.percent ?? 0, 100)}%"></span>
    </span>
    <div class="caption">
      {f.percent === null ? "No income recorded" : `${pct(f.percent)} of monthly income is budgeted`}
    </div>
  {:else if f.type === "credit"}
    <div class="display">{f.count === 0 ? "—" : money(f.available)}</div>
    {#if f.utilisation !== null}
      <!-- Above 30% of a limit is where utilisation starts to cost a credit score. -->
      <span class="track"><span class="fill" class:over={f.utilisation > 30} class:done={f.utilisation <= 30} style:width="{Math.min(f.utilisation, 100)}%"></span></span>
    {/if}
    <div class="caption">
      {f.count === 0
        ? "No credit cards or HELOCs"
        : f.recorded === 0
          ? "Enter available credit on each card or HELOC"
          : f.utilisation === null
            ? `Across ${f.recorded} accounts`
            : `${pct(f.utilisation)} in use · ${wholeMoney(f.limit)} total limit`}
    </div>
  {:else if f.type === "holdings"}
    <div class="display">{money(f.value)}</div>
    {#if Number(f.basis) > 0}
      <div class="strong" class:neg={negative(f.gain)} class:pos={!negative(f.gain)}>
        {signed(f.gain)}{f.percent === null ? "" : ` (${negative(f.gain) ? "−" : "+"}${pct(Math.abs(f.percent))})`} on cost
      </div>
    {:else}
      <div class="faint">No cost basis recorded</div>
    {/if}
    <div class="caption">{f.count} holding{f.count === 1 ? "" : "s"}</div>
  {:else if f.type === "spending"}
    <div class="caption" class:neg={!f.valid}>{f.period}</div>
    {#if f.valid && f.count === 0}<p class="faint">No reconciled charges in this period.</p>{/if}
    {#if f.stats.length === 0 && f.breakdowns.length === 0}
      <p class="faint">
        {editing ? "Choose figures and breakdowns with Edit." : "Nothing chosen to show. Customize the dashboard to pick some."}
      </p>
    {/if}
    {#if f.stats.length > 0}
      <div class="stats">
        {#each f.stats as stat (stat.key)}
          <div>
            <span class="label">{stat.label}</span>
            <span class="heading" class:accent={stat.key === "unattributed" && Number(stat.value) > 0}>
              {!stat.money ? stat.value : stat.key === "average" ? money(stat.value) : wholeMoney(stat.value)}
            </span>
          </div>
        {/each}
      </div>
    {/if}
    {#if f.breakdowns.length > 0 && f.count > 0}
      <div class="breakdowns">
        {#each f.breakdowns as b (b.key)}
          <div>
            <span class="label">{b.label}</span>
            {#if b.rows.length === 0}<p class="faint">Nothing here for this period.</p>{/if}
            {#each b.rows as row, i (i)}
              <div class="item">
                <div class="line">
                  <span class="name">{row.name}</span>
                  <span class="dim">{b.frequency ? `${row.count}× · ` : ""}{wholeMoney(row.amount)}</span>
                </div>
                <span class="track"><span class="fill" style:width="{row.share * 100}%"></span></span>
              </div>
            {/each}
            {#if b.more > 0}<div class="caption faint">+{b.more} more on the Spending page</div>{/if}
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="faint">This widget comes from a newer version of the app.</p>
  {/if}

  {#if f.type === "rows" && f.missing > 0}
    <div class="caption faint">
      {f.missing} chosen {f.missing === 1 ? "one has" : "have"} since been deleted
    </div>
  {/if}
</article>

<style>
  .widget {
    background: var(--raised);
    border: 1px solid var(--hairline);
    border-radius: 0.6rem;
    padding: 0.85rem 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    min-width: 0;
  }
  .widget.editing {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: 1.6rem;
  }
  .title {
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 0.1rem;
    flex-shrink: 0;
  }
  .display {
    font-size: 1.6rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .headline {
    font-size: 1.05rem;
    font-weight: 600;
  }
  .heading {
    font-size: 1.05rem;
    font-weight: 700;
  }
  .caption {
    font-size: 0.75rem;
    color: var(--dim);
  }
  .right {
    text-align: right;
  }
  .strong {
    font-weight: 600;
  }
  .faint {
    color: var(--faint);
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
  .accent {
    color: var(--accent);
  }
  .split {
    display: grid;
    grid-template-columns: 55% 1fr;
    gap: 1rem;
    align-items: center;
  }
  .trend {
    text-align: right;
  }
  svg {
    width: 100%;
    height: 30px;
    margin-top: 0.25rem;
  }
  polyline {
    fill: none;
    stroke: var(--positive);
    stroke-width: 1.5;
  }
  polyline.neg {
    stroke: var(--negative);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.75rem;
    font-variant-numeric: tabular-nums;
  }
  .stack {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    display: block;
    height: 5px;
    border-radius: 3px;
    background: var(--hairline);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .fill.done {
    background: var(--positive);
  }
  .fill.over {
    background: var(--negative);
  }
  .bar2 {
    display: flex;
    gap: 2px;
    height: 8px;
  }
  .bar2 span {
    border-radius: 3px;
  }
  .roth {
    background: var(--positive);
  }
  .trad {
    background: var(--accent);
  }
  .figures {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0.5rem;
  }
  .figures.three {
    grid-template-columns: repeat(3, 1fr);
  }
  .figures > div,
  .stats > div {
    display: flex;
    flex-direction: column;
    font-variant-numeric: tabular-nums;
  }
  .label {
    font-size: 0.7rem;
    font-weight: 600;
    color: var(--dim);
    text-transform: uppercase;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.6rem;
  }
  .breakdowns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: 0.8rem 1.25rem;
  }
  .breakdowns > div {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 0;
  }
</style>
