<script lang="ts">
  import { untrack } from "svelte";
  import {
    kindLabel,
    localToday,
    spendingBreakdownOptions,
    spendingLimits,
    spendingPeriods,
    spendingStatOptions,
    spendingStatuses,
    wholeMoney,
    widgetId,
    type Choice,
    type DashboardView,
    type SpendingOptions,
    type Widget,
  } from "./ledger";

  // Adding a widget, or changing one: its kind (when adding), a title of its
  // own, its width, what it is pointed at, and for spending what it shows.
  let {
    existing = null,
    dash,
    onsave,
    oncancel,
  }: {
    existing?: Widget | null;
    dash: DashboardView;
    onsave: (widget: Widget, adding: boolean) => void;
    oncancel: () => void;
  } = $props();

  const MAX_REFS = 50;
  const defaults: SpendingOptions = {
    period: "1m",
    status: "all",
    from: "",
    to: "",
    stats: ["total", "fromBuckets", "everyday", "unattributed"],
    breakdowns: ["people", "items"],
    limit: 5,
  };

  const was = untrack(() => existing);
  let kind = $state(was?.kind ?? "");
  let title = $state(was?.title ?? "");
  let span = $state<1 | 2>(was?.span ?? 1);
  let refs = $state<string[]>(was ? [...was.refs] : []);
  let options = $state<SpendingOptions>(structuredClone($state.snapshot(was?.options) ?? defaults));

  const def = $derived(dash.kinds.find((k) => k.kind === kind) ?? null);
  const choices = $derived<Choice[]>(def?.picks ? dash.choices[def.picks] : []);
  const customValid = $derived(
    options.period !== "custom" ||
      (/^\d{4}-\d{2}-\d{2}$/.test(options.from) &&
        /^\d{4}-\d{2}-\d{2}$/.test(options.to) &&
        options.from <= options.to),
  );
  const valid = $derived(def !== null && (kind !== "spending" || customValid));

  function choose(next: string) {
    const k = dash.kinds.find((d) => d.kind === next);
    if (!k) return;
    kind = next;
    span = k.span;
    refs = k.picks ? [...dash.suggested[k.picks]] : [];
    options = structuredClone(defaults);
  }

  // The order things are ticked is the order they show in.
  function toggle(id: string) {
    if (refs.includes(id)) refs = refs.filter((r) => r !== id);
    else if (refs.length < MAX_REFS) refs = [...refs, id];
  }

  function toggleIn(list: "stats" | "breakdowns", key: string) {
    const held = options[list];
    options[list] = held.includes(key) ? held.filter((k) => k !== key) : [...held, key];
  }

  function choosePeriod(next: string) {
    options.period = next;
    if (next === "custom") {
      if (!options.from) {
        // A month back on this machine's calendar, not in UTC.
        const d = new Date();
        d.setMonth(d.getMonth() - 1);
        const pad = (n: number) => String(n).padStart(2, "0");
        options.from = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
      }
      if (!options.to) options.to = localToday();
    }
  }

  function describe(c: Choice): string {
    if (def?.picks === "accounts") return `${kindLabel(c.kind)} · ${wholeMoney(c.value)}${c.owes ? " owed" : ""}`;
    if (def?.picks === "goals") return c.percent === null ? "no target" : `${c.percent.toFixed(1)}%`;
    return wholeMoney(c.value) + (c.target ? ` of ${wholeMoney(c.target)}` : "");
  }

  function submit() {
    if (!valid) return;
    const widget: Widget = {
      id: existing?.id ?? widgetId(),
      kind,
      title: title.trim(),
      span,
      refs: def?.picks ? refs : [],
    };
    if (kind === "spending") widget.options = $state.snapshot(options);
    onsave(widget, existing === null);
  }
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <h3>{existing ? `Edit ${existing.title || def?.label || "widget"}` : "Add a widget"}</h3>

  {#if !existing}
    <div class="kinds">
      {#each dash.kinds as k (k.kind)}
        <button type="button" class="kind" class:on={kind === k.kind} onclick={() => choose(k.kind)}>
          <strong>{k.label}</strong>
          <span>{k.description}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if def}
    <label class="field">
      <span>Title</span>
      <input bind:value={title} maxlength="60" placeholder={def.label} />
    </label>
    <label class="field">
      <span>Width</span>
      <select bind:value={span}>
        <option value={1}>Half a row</option>
        <option value={2}>The whole row</option>
      </select>
    </label>

    {#if def.picks}
      <div class="picks-head">
        <span class="muted">Show these {def.picks}, in the order ticked</span>
        <span>
          <button type="button" class="bare" onclick={() => (refs = choices.slice(0, MAX_REFS).map((c) => c.id))}>
            All
          </button>
          <button type="button" class="bare" onclick={() => (refs = [])}>None</button>
        </span>
      </div>
      {#if choices.length === 0}
        <p class="empty">There are no {def.picks} yet.</p>
      {:else}
        <ul class="picks">
          {#each choices as c (c.id)}
            <li>
              <label class="field check">
                <input type="checkbox" checked={refs.includes(c.id)} onchange={() => toggle(c.id)} />
                <span class="pick">
                  <span>{c.name}</span>
                  <span class="muted">{describe(c)}</span>
                  {#if refs.includes(c.id)}<span class="order">{refs.indexOf(c.id) + 1}</span>{/if}
                </span>
              </label>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}

    {#if kind === "spending"}
      <label class="field">
        <span>Period</span>
        <select value={options.period} onchange={(e) => choosePeriod(e.currentTarget.value)}>
          {#each spendingPeriods as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
        </select>
      </label>
      {#if options.period === "custom"}
        <label class="field"><span>From</span><input type="date" bind:value={options.from} /></label>
        <label class="field"><span>To</span><input type="date" bind:value={options.to} /></label>
        {#if !customValid}<p class="error">Choose dates with From on or before To.</p>{/if}
      {/if}
      <label class="field">
        <span>Status</span>
        <select bind:value={options.status}>
          {#each spendingStatuses as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
        </select>
      </label>
      <fieldset>
        <legend>Figures</legend>
        {#each spendingStatOptions as s (s.value)}
          <label class="field check">
            <input type="checkbox" checked={options.stats.includes(s.value)} onchange={() => toggleIn("stats", s.value)} />
            <span>{s.label}</span>
          </label>
        {/each}
      </fieldset>
      <fieldset>
        <legend>Breakdowns</legend>
        {#each spendingBreakdownOptions as b (b.value)}
          <label class="field check">
            <input
              type="checkbox"
              checked={options.breakdowns.includes(b.value)}
              onchange={() => toggleIn("breakdowns", b.value)}
            />
            <span>{b.label}</span>
          </label>
        {/each}
      </fieldset>
      <label class="field">
        <span>Rows per breakdown</span>
        <select bind:value={options.limit}>
          {#each spendingLimits as n (n)}<option value={n}>{n}</option>{/each}
        </select>
      </label>
    {/if}
  {/if}

  <div class="actions">
    <button type="submit" disabled={!valid}>{existing ? "Save" : "Add"}</button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  .kinds {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: 0.4rem;
    margin-bottom: 0.6rem;
  }
  .kind {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
    text-align: left;
    white-space: normal;
    padding: 0.5rem 0.6rem;
  }
  .kind span {
    font-size: 0.75rem;
    color: var(--dim);
  }
  .kind.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .picks-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 0.5rem;
  }
  .picks {
    list-style: none;
    margin: 0.25rem 0 0.5rem;
    padding: 0;
    max-height: 16rem;
    overflow: auto;
  }
  .pick {
    display: flex;
    gap: 0.6rem;
    align-items: baseline;
  }
  .order {
    font-size: 0.7rem;
    color: var(--accent);
  }
  fieldset {
    border: 1px solid var(--hairline);
    border-radius: 0.35rem;
    margin: 0.4rem 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
  }
  legend {
    font-size: 0.75rem;
    color: var(--dim);
  }
</style>
