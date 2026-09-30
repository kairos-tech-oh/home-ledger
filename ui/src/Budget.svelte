<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import TypesPanel from "./TypesPanel.svelte";
  import TemplatesPanel from "./TemplatesPanel.svelte";
  import {
    ledger,
    money,
    type AccountView,
    type BucketView,
    type BudgetView,
    type TemplateView,
  } from "./ledger";
  import type { Overview } from "./lib";

  let {
    budget,
    accounts,
    buckets,
    budgetTypes,
    fixedTypes,
    templates,
    overview,
    onchanged,
  }: {
    budget: BudgetView[];
    accounts: AccountView[];
    buckets: BucketView[];
    budgetTypes: string[];
    fixedTypes: string[];
    templates: TemplateView[];
    overview: Overview | null;
    onchanged: () => void;
  } = $props();

  let editing = $state<BudgetView | null>(null);
  let adding = $state(false);
  let busy = $state(false);
  let error = $state("");
  let confirmDelete = $state<BudgetView | null>(null);
  let view = $state<"cards" | "table">("table");
  let managingTypes = $state(false);
  let managingTemplates = $state(false);

  let name = $state("");
  let kind = $state("");
  let monthly = $state("");
  let accountId = $state("");
  let bucketId = $state("");
  let notes = $state("");

  const open = $derived(adding || editing !== null);

  // Sections rolled up by a click on their heading. Held apart from the view,
  // so switching between cards and table leaves them as they were.
  const collapsed = new SvelteSet<string>();

  function fold(type: string) {
    if (!collapsed.delete(type)) collapsed.add(type);
  }

  // Grouped the way a budget is read: by what each line is filed under, in the
  // order the types are listed.
  const grouped = $derived.by(() => {
    const seen = new Map<string, BudgetView[]>();
    for (const item of budget) {
      const list = seen.get(item.kind) ?? [];
      list.push(item);
      seen.set(item.kind, list);
    }
    const order = [...budgetTypes, ...[...seen.keys()].filter((k) => !budgetTypes.includes(k))];
    return order
      .filter((type) => seen.has(type))
      .map((type) => {
        const items = seen.get(type)!;
        const subtotal = items.reduce((sum, i) => sum + Number(i.monthlyAmount), 0);
        return { type, items, subtotal: subtotal.toFixed(2) };
      });
  });

  const unallocated = $derived.by(() => {
    if (!overview) return null;
    return (Number(overview.monthlyIncome) - Number(overview.monthlyBudget)).toFixed(2);
  });

  const leftPercent = $derived.by(() => {
    if (!overview || unallocated === null) return null;
    const income = Number(overview.monthlyIncome);
    if (!Number.isFinite(income) || income <= 0) return null;
    return Math.round((Number(unallocated) / income) * 100);
  });

  const annual = $derived(
    overview ? (Number(overview.monthlyBudget) * 12).toFixed(2) : null,
  );

  /// One pill colour per type, stable so a type keeps its colour between runs.
  function pillColour(type: string): string {
    const palette = ["blue", "amber", "green", "violet", "red"];
    let hash = 0;
    for (const ch of type) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
    return palette[hash % palette.length];
  }

  function edit(item: BudgetView | null) {
    editing = item;
    adding = item === null;
    name = item?.name ?? "";
    kind = item?.kind ?? budgetTypes[0] ?? "Living";
    monthly = item?.monthlyAmount ?? "";
    accountId = item?.accountId ?? "";
    bucketId = item?.bucketId ?? "";
    notes = item?.notes ?? "";
    error = "";
  }

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      editing = null;
      adding = false;
      confirmDelete = null;
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function save() {
    run(() =>
      ledger.apply({
        op: "set",
        kind: "budget",
        id: editing?.id ?? "",
        record: {
          name: name.trim(),
          type: kind,
          monthlyAmount: monthly.trim() || "0",
          accountId,
          bucketId,
          notes: notes.trim(),
        },
      }),
    );
  }
</script>

{#if overview}
  <section class="tiles">
    <article class="tile">
      <div class="tile-label">Budgeted</div>
      <div class="tile-figure">{money(overview.monthlyBudget)}</div>
      <div class="tile-note">a month across {budget.length} lines</div>
    </article>
    <article class="tile">
      <div class="tile-label">Income</div>
      <div class="tile-figure">{money(overview.monthlyIncome)}</div>
      <div class="tile-note">a month from every stream</div>
    </article>
    <article class="tile">
      <div class="tile-label">Unallocated</div>
      <div
        class="tile-figure"
        class:pos={unallocated !== null && Number(unallocated) >= 0}
        class:neg={unallocated !== null && Number(unallocated) < 0}
      >
        {money(unallocated)}
      </div>
      <div class="tile-note">
        {leftPercent === null ? "of income" : `${leftPercent}% of income left over`}
      </div>
    </article>
    <article class="tile">
      <div class="tile-label">Annual</div>
      <div class="tile-figure">{money(annual)}</div>
      <div class="tile-note">twelve times the monthly budget</div>
    </article>
  </section>
{/if}

<div class="section-head">
  <h2>Monthly budget</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    <button onclick={() => (managingTypes = !managingTypes)}>Types</button>
    <button onclick={() => (managingTemplates = !managingTemplates)}>Templates</button>
    {#if !open}
      <button onclick={() => edit(null)} disabled={busy}>+ Line</button>
    {/if}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#if managingTemplates}
  <TemplatesPanel
    {templates}
    {budget}
    monthly={overview?.monthlyBudget ?? "0"}
    {onchanged}
    onclose={() => (managingTemplates = false)}
  />
{/if}
{#if managingTypes}
  <TypesPanel
    list="budget"
    types={budgetTypes}
    fixed={fixedTypes}
    {onchanged}
    onclose={() => (managingTypes = false)}
  />
{/if}

{#if adding || editing}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <h3>{editing ? `Edit ${editing.name}` : "Add a budget line"}</h3>
    <label class="field"><span>Name</span><input bind:value={name} required /></label>
    <label class="field">
      <span>Filed under</span>
      <select bind:value={kind}>
        {#each budgetTypes as type (type)}<option value={type}>{type}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Monthly amount</span>
      <input bind:value={monthly} inputmode="decimal" required />
    </label>
    <label class="field">
      <span>Paid from</span>
      <select bind:value={accountId}>
        <option value="">No account</option>
        {#each accounts as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Feeds bucket</span>
      <select bind:value={bucketId}>
        <option value="">No bucket</option>
        {#each buckets as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
      </select>
    </label>
    <label class="field"><span>Notes</span><input bind:value={notes} placeholder="optional" /></label>
    {#if editing && editing.draws > 0}
      <p class="note">
        {editing.draws} planned draw{editing.draws === 1 ? "" : "s"} sit under this line,
        worth {money(editing.drawsMonthly)} a month. Editing here leaves them alone.
      </p>
    {/if}
    <div class="actions">
      <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save"}</button>
      <button type="button" class="bare" onclick={() => { editing = null; adding = false; }}>
        Cancel
      </button>
    </div>
  </form>
{/if}

{#if confirmDelete}
  <div class="warn-box">
    <p class="confirm-text">
      Delete <strong>{confirmDelete.name}</strong>?
      {#if confirmDelete.draws > 0}
        Its {confirmDelete.draws} planned draw{confirmDelete.draws === 1 ? "" : "s"} go with it.
      {/if}
    </p>
    <div class="actions">
      <button
        disabled={busy}
        onclick={() =>
          run(() => ledger.apply({ op: "delete", kind: "budget", id: confirmDelete!.id }))}
      >
        Delete
      </button>
      <button class="bare" onclick={() => (confirmDelete = null)}>Keep it</button>
    </div>
  </div>
{/if}

{#each grouped as group (group.type)}
  <button
    class="group-head fold"
    onclick={() => fold(group.type)}
    aria-expanded={!collapsed.has(group.type)}
    title={collapsed.has(group.type) ? "Show these lines" : "Roll these lines up"}
  >
    <span class="chevron">{collapsed.has(group.type) ? "▸" : "▾"}</span>
    <span class="pill {pillColour(group.type)}">{group.type}</span>
    <span class="group-count">
      {group.items.length} line{group.items.length === 1 ? "" : "s"}
    </span>
    <span class="group-total {pillColour(group.type)}-text">{money(group.subtotal)}</span>
  </button>
  {#if collapsed.has(group.type)}
    <!-- rolled up: the heading and its subtotal only -->
  {:else if view === "cards"}
    <div class="cards">
      {#each group.items as item (item.id)}
        <article class="card">
          <div class="card-top">
            {#if item.accountName}<span class="muted">{item.accountName}</span>{/if}
            {#if item.bucketName}<span class="card-where">{item.bucketName}</span>{/if}
          </div>
          <div class="card-name">{item.name}</div>
          <div class="card-figure">{money(item.monthlyAmount)}</div>
          <div class="card-note">
            {item.split.map((s) => `${s.owner} ${money(s.amount)}`).join(" · ")}
          </div>
          <div class="card-actions">
            <button class="bare" onclick={() => edit(item)} disabled={busy}>Edit</button>
            <button
              class="bare danger"
              onclick={() => (confirmDelete = item)}
              disabled={busy}
              title="Delete">×</button
            >
          </div>
        </article>
      {/each}
    </div>
  {:else}
  <ul class="rows">
    {#each group.items as item (item.id)}
      <li class="row">
        <div class="row-main">
          <div class="row-name">{item.name}</div>
          <div class="row-meta">
            {#if item.accountName}from {item.accountName} · {/if}
            {#if item.draws > 0}
              {item.draws} planned draw{item.draws === 1 ? "" : "s"}
              {money(item.drawsMonthly)} ·
            {/if}
            {item.split.map((s) => `${s.owner} ${money(s.amount)}`).join(" · ")}
          </div>
        </div>
        <span class="row-amount">
          {money(item.monthlyAmount)}
          {#if item.bucketName}<span class="muted bucket">{item.bucketName}</span>{/if}
        </span>
        <div class="row-actions">
          <button class="bare" onclick={() => edit(item)} disabled={busy}>Edit</button>
          <button
            class="bare danger"
            onclick={() => (confirmDelete = item)}
            disabled={busy}
            title="Delete">×</button
          >
        </div>
      </li>
    {/each}
  </ul>
  {/if}
{:else}
  <p class="empty">No budget lines yet.</p>
{/each}

<style>
  .controls {
    display: flex;
    gap: 0.6rem;
  }
  /* The subtotal takes its group's colour, the way the plugin does it. */
  .blue-text {
    color: var(--pill-blue);
  }
  .violet-text {
    color: var(--pill-violet);
  }
  .green-text {
    color: var(--pill-green);
  }
  .amber-text {
    color: var(--pill-amber);
  }
  .red-text {
    color: var(--pill-red);
  }
  .bucket {
    font-size: 0.7rem;
    font-weight: 400;
    margin-left: 0.5rem;
  }
  .confirm-text {
    margin: 0 0 0.4rem;
  }
</style>
