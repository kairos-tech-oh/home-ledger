<script lang="ts">
  import {
    frequencies,
    frequencyLabel,
    ledger,
    money,
    type AccountView,
    type EarnerView,
    type IncomeView,
  } from "./ledger";
  import type { Overview } from "./lib";

  let {
    income,
    accounts,
    earners,
    overview,
    onchanged,
  }: {
    income: IncomeView[];
    accounts: AccountView[];
    earners: EarnerView[];
    overview: Overview | null;
    onchanged: () => void;
  } = $props();


  let editing = $state<IncomeView | null>(null);
  let adding = $state(false);
  let busy = $state(false);
  let error = $state("");
  let confirmDelete = $state<IncomeView | null>(null);
  let view = $state<"cards" | "table">("table");

  const open = $derived(adding || editing !== null);
  const annual = $derived(
    overview ? (Number(overview.monthlyIncome) * 12).toFixed(2) : null,
  );

  let name = $state("");
  let owner = $state("");
  let monthly = $state("");
  let frequency = $state("biweekly");
  let accountId = $state("");
  let notes = $state("");

  function edit(stream: IncomeView | null) {
    editing = stream;
    adding = stream === null;
    name = stream?.name ?? "";
    owner = stream?.owner ?? "";
    monthly = stream?.monthlyTotal ?? "";
    frequency = stream?.frequency ?? "biweekly";
    accountId = stream?.accountId ?? "";
    notes = stream?.notes ?? "";
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
        kind: "income",
        id: editing?.id ?? "",
        record: {
          name: name.trim(),
          owner: owner.trim(),
          monthlyTotal: monthly.trim() || "0",
          frequency,
          accountId,
          notes: notes.trim(),
        },
      }),
    );
  }
</script>

{#if overview}
  <section class="tiles">
    <article class="tile">
      <div class="tile-label">Monthly</div>
      <div class="tile-figure pos">{money(overview.monthlyIncome)}</div>
      <div class="tile-note">across {income.length} stream{income.length === 1 ? "" : "s"}</div>
    </article>
    <article class="tile">
      <div class="tile-label">Annual</div>
      <div class="tile-figure">{money(annual)}</div>
      <div class="tile-note">twelve times the monthly</div>
    </article>
    {#each earners.slice(0, 2) as earner (earner.owner)}
      <article class="tile">
        <div class="tile-label">{earner.owner}</div>
        <div class="tile-figure">{money(earner.monthly)}</div>
        <div class="tile-note">
          {earner.percent.toFixed(1)}% · {earner.paychecksPerMonth.toFixed(2)} paychecks a month
        </div>
      </article>
    {/each}
  </section>
{/if}

<div class="section-head">
  <h2>{income.length} streams</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    {#if !open}
      <button onclick={() => edit(null)} disabled={busy}>+ Stream</button>
    {/if}
  </div>
</div>

<p class="note">
  Earners come from who the streams name, and every budget line splits in these
  proportions.
</p>

{#if error}<p class="error">{error}</p>{/if}

{#if adding || editing}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <h3>{editing ? `Edit ${editing.name}` : "Add an income stream"}</h3>
    <label class="field"><span>Name</span><input bind:value={name} required /></label>
    <label class="field">
      <span>Who earns it</span>
      <input bind:value={owner} placeholder="leave blank for unassigned" />
    </label>
    <label class="field">
      <span>Monthly total</span>
      <input bind:value={monthly} inputmode="decimal" required />
    </label>
    <label class="field">
      <span>Paid</span>
      <select bind:value={frequency}>
        {#each frequencies as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>Into</span>
      <select bind:value={accountId}>
        <option value="">No account</option>
        {#each accounts as account (account.id)}
          <option value={account.id}>{account.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>Notes</span><input bind:value={notes} placeholder="optional" />
    </label>
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
      Delete <strong>{confirmDelete.name}</strong>? Every earner share and budget split
      is worked out from income, so removing this changes them.
    </p>
    <div class="actions">
      <button
        disabled={busy}
        onclick={() =>
          run(() => ledger.apply({ op: "delete", kind: "income", id: confirmDelete!.id }))}
      >
        Delete
      </button>
      <button class="bare" onclick={() => (confirmDelete = null)}>Keep it</button>
    </div>
  </div>
{/if}

{#if view === "cards"}
  <div class="cards">
    {#each income as stream (stream.id)}
      <article class="card">
        <div class="card-top">
          <span class="pill blue">{stream.owner || "Unassigned"}</span>
          <span class="card-where">{frequencyLabel(stream.frequency)}</span>
        </div>
        <div class="card-name">{stream.name}</div>
        <div class="card-figure">{money(stream.monthlyTotal)}</div>
        <div class="card-note">
          {money(stream.perPaycheck)} per paycheck
          {#if stream.accountName} · into {stream.accountName}{/if}
        </div>
        <div class="card-actions">
          <button class="bare" onclick={() => edit(stream)} disabled={busy}>Edit</button>
          <button
            class="bare danger"
            onclick={() => (confirmDelete = stream)}
            disabled={busy}
            title="Delete">×</button
          >
        </div>
      </article>
    {:else}
      <p class="empty">No income streams yet.</p>
    {/each}
  </div>
{:else}
<ul class="rows">
  {#each income as stream (stream.id)}
    <li class="row">
      <div class="row-main">
        <div class="head">
          <span class="row-name">{stream.name}</span>
          <span class="pill blue">{stream.owner || "Unassigned"}</span>
        </div>
        <div class="row-meta">
          {frequencyLabel(stream.frequency)} · {money(stream.perPaycheck)} per paycheck
          {#if stream.accountName} · into {stream.accountName}{/if}
        </div>
      </div>
      <span class="row-amount">{money(stream.monthlyTotal)}</span>
      <div class="row-actions">
        <button class="bare" onclick={() => edit(stream)} disabled={busy}>Edit</button>
        <button
          class="bare danger"
          onclick={() => (confirmDelete = stream)}
          disabled={busy}
          title="Delete">×</button
        >
      </div>
    </li>
  {:else}
    <p class="empty">No income streams yet.</p>
  {/each}
</ul>
{/if}

<style>
  .controls {
    display: flex;
    gap: 0.6rem;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }
  .confirm-text {
    margin: 0 0 0.4rem;
  }
</style>
