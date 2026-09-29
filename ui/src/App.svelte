<script lang="ts">
  import { api, type Overview, type StoreStatus, type SyncState } from "./lib";
  import Storage from "./Storage.svelte";
  import Accounts from "./Accounts.svelte";
  import Buckets from "./Buckets.svelte";
  import Income from "./Income.svelte";
  import Budget from "./Budget.svelte";
  import History from "./History.svelte";
  import Holdings from "./Holdings.svelte";
  import Retirement from "./Retirement.svelte";
  import Reconcile from "./Reconcile.svelte";
  import Goals from "./Goals.svelte";
  import Planning from "./Planning.svelte";
  import Spending from "./Spending.svelte";
  import Dashboard from "./Dashboard.svelte";
  import { ledger as ledgerApi, money, type LedgerView } from "./ledger";
  import { storage, type Setup } from "./storage";

  let overview = $state<Overview | null>(null);
  let stores = $state<StoreStatus[]>([]);
  let sync = $state<SyncState | null>(null);
  let error = $state("");
  let busy = $state(false);
  let setup = $state<Setup | null>(null);
  let view = $state<LedgerView | null>(null);
  // Bumped on every reload, for screens that fetch figures of their own.
  let revision = $state(0);
  // In the plugin's order: the landing page, what you have, what comes and
  // goes, what you are aiming at, then the record and the plumbing.
  const tabs = [
    ["dashboard", "Dashboard"],
    ["accounts", "Accounts"],
    ["holdings", "Holdings"],
    ["retirement", "Retirement"],
    ["income", "Income"],
    ["budget", "Budget"],
    ["savings", "Savings"],
    ["reconcile", "Reconcile"],
    ["spending", "Spending"],
    ["goals", "Goals"],
    ["planning", "Planning"],
    ["history", "History"],
    ["storage", "Storage"],
  ] as const;
  type Tab = (typeof tabs)[number][0];
  let tab = $state<Tab>("dashboard");

  function open(page: string) {
    const found = tabs.find(([value]) => value === page);
    if (found) tab = found[0];
  }

  const firstRun = $derived(setup !== null && !setup.setupComplete);

  // Applied to the document rather than a wrapper, because the ground is
  // painted on body and the window behind it is transparent.
  $effect(() => {
    if (setup) {
      document.documentElement.style.setProperty("--surface-alpha", String(setup.opacity));
    }
  });

  async function load() {
    busy = true;
    error = "";
    try {
      [overview, stores, sync, setup, view] = await Promise.all([
        api.overview(),
        api.stores(),
        api.syncState(),
        storage.setup(),
        ledgerApi.read(),
      ]);
      revision += 1;
      if (setup && !setup.setupComplete) tab = "storage";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
    if (setup?.setupComplete) {
      accrueOnce();
      snapshotOnce();
    }
  }

  // One point a day of net worth history, taken the first time the app opens
  // that day. Its own call, so a failure never stops the ledger loading.
  let snapped = false;
  async function snapshotOnce() {
    if (snapped) return;
    snapped = true;
    try {
      if (await ledgerApi.takeSnapshot()) revision += 1;
    } catch (e) {
      console.warn("snapshot not taken", e);
    }
  }

  // Once per launch, as the prototype does: months that went by while the app
  // was closed are added now. Null means nothing was due.
  let accrued = false;
  async function accrueOnce() {
    if (accrued) return;
    accrued = true;
    try {
      if (await ledgerApi.apply({ op: "retirement-accrue", id: "" })) await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function flush() {
    busy = true;
    try {
      sync = await api.flush();
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    load();
  });

  function syncLabel(s: SyncState | null): string {
    if (!s) return "…";
    switch (s.state) {
      case "synced":
        return "Up to date";
      case "behind":
        return `${s.queued} waiting`;
      case "blocked":
        return "Needs attention";
      case "unconfigured":
        return "No store";
    }
  }

  // The line under the title, which is what the plugin puts there: the two or
  // three figures worth knowing before you look at anything else.
  const summary = $derived.by(() => {
    if (!view || !overview) return "";
    const parts = [
      `${view.accounts.length} accounts`,
      `net ${money(overview.net)}`,
      `${money(overview.monthlyIncome)} in`,
      `${money(overview.monthlyBudget)} out`,
    ];
    return parts.join("  ·  ");
  });
</script>

<header>
  <div class="title">
    <h1>Home Ledger</h1>
    {#if summary && !firstRun}
      <div class="summary">{summary}</div>
    {/if}
  </div>

  <div
    class="sync"
    class:warn-text={sync?.state === "behind"}
    class:neg={sync?.state === "blocked"}
  >
    <span>{syncLabel(sync)}</span>
    <button onclick={flush} disabled={busy}>Sync</button>
  </div>
</header>

{#if !firstRun}
  <nav>
    {#each tabs as [value, label] (value)}
      <button class:on={tab === value} onclick={() => (tab = value)}>{label}</button>
    {/each}
  </nav>
{/if}

<main class="page">
  {#if error}<p class="error">{error}</p>{/if}

  {#if overview?.stale}
    <p class="warn-box">
      Showing a copy from <strong>{overview.loadedFrom}</strong>. The source of truth
      could not be reached, so this may be behind.
    </p>
  {/if}

  {#if firstRun}
    <p class="note">
      Your ledger can live on this computer, in storage you own, or both. Nothing here
      is permanent — all of it can be changed later in Storage.
    </p>
  {/if}

  {#if tab === "storage" && setup}
    <Storage {setup} {stores} {firstRun} onchanged={(next) => { setup = next; load(); }} />
  {:else if tab === "dashboard"}
    <Dashboard {revision} onchanged={load} onopen={open} />
  {:else if tab === "accounts" && view}
    <Accounts accounts={view.accounts} {overview} onchanged={load} />
  {:else if tab === "holdings" && view}
    <Holdings
      holdings={view.holdings}
      accounts={view.accounts}
      buckets={view.buckets}
      types={view.investmentTypes}
      fixedTypes={view.fixedTypes.investment}
      onchanged={load}
    />
  {:else if tab === "retirement" && view}
    <Retirement retirement={view.retirement} onchanged={load} />
  {:else if tab === "reconcile" && view}
    <Reconcile
      reconciliations={view.reconciliations}
      members={view.members}
      accounts={view.accounts}
      buckets={view.buckets}
      onchanged={load}
    />
  {:else if tab === "income" && view}
    <Income
      income={view.income}
      accounts={view.accounts}
      earners={view.earners}
      {overview}
      onchanged={load}
    />
  {:else if tab === "budget" && view}
    <Budget
      budget={view.budget}
      accounts={view.accounts}
      buckets={view.buckets}
      budgetTypes={view.budgetTypes}
      fixedTypes={view.fixedTypes.budget}
      templates={view.templates}
      {overview}
      onchanged={load}
    />
  {:else if tab === "savings" && view}
    <Buckets buckets={view.buckets} onchanged={load} />
  {:else if tab === "goals" && view}
    <Goals goals={view.goals} totals={view.goalTotals} buckets={view.buckets} onchanged={load} />
  {:else if tab === "spending"}
    <Spending {revision} onchanged={load} />
  {:else if tab === "planning"}
    <Planning {revision} />
  {:else if tab === "history"}
    <History />
  {/if}
</main>

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.7rem 1rem 0.4rem;
  }
  .title {
    min-width: 0;
  }
  h1 {
    font-size: 1rem;
    margin: 0;
    font-weight: 600;
  }
  .summary {
    font-size: 0.72rem;
    color: var(--dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Its own row, wrapping, so every tab stays one click away however narrow
     the window. */
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    padding: 0 1rem 0.5rem;
    border-bottom: 1px solid var(--hairline);
  }
  nav button.on {
    background: var(--raised-strong);
    border-color: var(--faint);
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.72rem;
    color: var(--dim);
    white-space: nowrap;
  }
</style>
