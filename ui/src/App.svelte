<script lang="ts">
  import { untrack } from "svelte";
  import { look, setCustomThemes, toggleDark } from "./theme.svelte";
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
  import NavMenu from "./NavMenu.svelte";
  import UpdateBanner from "./UpdateBanner.svelte";
  import UnlockScreen from "./UnlockScreen.svelte";
  import NameThisComputer from "./NameThisComputer.svelte";
  import { encryption } from "./encryption";
  import { ledger as ledgerApi, money, type LedgerView } from "./ledger";
  import { storage, type Setup } from "./storage";
  import { blur, privacy, syncDemo } from "./privacy.svelte";

  let overview = $state<Overview | null>(null);
  let stores = $state<StoreStatus[]>([]);
  let sync = $state<SyncState | null>(null);
  let error = $state("");
  let busy = $state(false);
  let setup = $state<Setup | null>(null);
  let view = $state<LedgerView | null>(null);
  // Bumped on every reload, for screens that fetch figures of their own.
  let revision = $state(0);
  // The landing page, what you have, what comes and goes, what you are aiming
  // at, then the record and the plumbing.
  const groups = [
    { label: "Dashboard", pages: [["dashboard", "Dashboard"]] },
    {
      label: "Wealth",
      pages: [
        ["accounts", "Accounts"],
        ["holdings", "Holdings"],
        ["retirement", "Retirement"],
        ["savings", "Savings"],
      ],
    },
    {
      label: "Cash flow",
      pages: [
        ["income", "Income"],
        ["budget", "Budget"],
        ["reconcile", "Reconcile"],
        ["spending", "Spending"],
      ],
    },
    {
      label: "Plans",
      pages: [
        ["goals", "Goals"],
        ["planning", "Planning"],
      ],
    },
    {
      label: "Ledger",
      pages: [
        ["history", "History"],
        ["storage", "Settings"],
      ],
    },
  ] as const;
  type Tab = (typeof groups)[number]["pages"][number][0];
  const tabs: (readonly [Tab, string])[] = groups.flatMap(
    (g): readonly (readonly [Tab, string])[] => g.pages,
  );
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
      const alpha = look.style === "terminal" ? setup.opacity : 1;
      document.documentElement.style.setProperty("--surface-alpha", String(alpha));
    }
  });

  // Colour themes live in the ledger, so they arrive with every read. Written
  // untracked: wearing one reads the look, which must not re-run this.
  $effect(() => {
    // Only from a real read: before one, the device's copy of the worn theme stands.
    const themes = view?.themes;
    if (themes) untrack(() => setCustomThemes(themes));
  });

  // The ledger is encrypted and this machine does not hold the key yet.
  let locked = $state(false);

  async function load() {
    busy = true;
    error = "";
    try {
      locked = false;
      // Made from the real ledger, so it waits for an unlock like any read.
      await syncDemo();
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
      // A read refused because the ledger is encrypted and this machine has
      // no key: offer to unlock rather than showing an error.
      if (String(e).includes("encrypted")) {
        try {
          locked = (await encryption.status()).needsUnlock;
        } catch {
          // Fall through to showing the error.
        }
      }
      if (!locked) error = String(e);
    } finally {
      busy = false;
    }
    if (setup?.setupComplete) {
      // Top-ups belong in the real ledger; in demo mode they wait.
      if (!privacy.demo) accrueOnce();
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

  $effect(() => {
    blur(privacy.blur);
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

<header class="app-bar">
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
    <button
      class="bare mode-switch"
      onclick={toggleDark}
      title={look.dark ? "Switch to light" : "Switch to dark"}
      aria-label={look.dark ? "Switch to light" : "Switch to dark"}
    >
      {#if look.dark}
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><circle cx="8" cy="8" r="3.2" fill="currentColor" /><g stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><path d="M8 1.2v1.6M8 13.2v1.6M1.2 8h1.6M13.2 8h1.6M3.2 3.2l1.1 1.1M11.7 11.7l1.1 1.1M3.2 12.8l1.1-1.1M11.7 4.3l1.1-1.1" /></g></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M13.6 10.2A6 6 0 0 1 5.8 2.4a6 6 0 1 0 7.8 7.8z" fill="currentColor" /></svg>
      {/if}
    </button>
  </div>
</header>

{#if !firstRun && !locked}
  <div class="nav-row">
    <NavMenu {groups} current={tab} onchoose={(page) => (tab = page)} />
  </div>
{/if}

<main class="page">
  <UpdateBanner />
  {#if locked}
    <UnlockScreen onunlocked={load} />
  {:else}
  {#if setup && setup.setupComplete && setup.deviceIsDefault}
    <NameThisComputer {setup} onrenamed={(next) => (setup = next)} />
  {/if}
  {#if error}<p class="error">{error}</p>{/if}

  {#if privacy.demo && !firstRun}
    <p class="warn-box">
      <strong>Demo mode.</strong> Every figure is made up, and edits are thrown away. Turn it
      off in Settings.
    </p>
  {/if}

  {#if overview?.stale}
    <p class="warn-box">
      Showing a copy from <strong>{overview.loadedFrom}</strong>. The source of truth
      could not be reached, so this may be behind.
    </p>
  {/if}

  {#if firstRun}
    <p class="note">
      Your ledger can live on this computer, in storage you own, or both. Nothing here
      is permanent — all of it can be changed later in Settings.
    </p>
  {/if}

  {#if tab === "storage" && setup}
    <Storage {setup} {stores} {firstRun} onchanged={(next) => { setup = next; load(); }} />
  {:else if tab === "dashboard"}
    <Dashboard {revision} onchanged={load} onopen={open} />
  {:else if tab === "accounts" && view}
    <Accounts accounts={view.accounts} order={view.accountOrder} {overview} onchanged={load} />
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
    <Buckets
      buckets={view.buckets}
      accounts={view.accounts}
      paydays={view.paydays}
      holdings={view.holdings}
      budget={view.budget}
      goals={view.goals}
      reconciliations={view.reconciliations}
      onchanged={load}
    />
  {:else if tab === "goals" && view}
    <Goals goals={view.goals} totals={view.goalTotals} buckets={view.buckets} onchanged={load} />
  {:else if tab === "spending"}
    <Spending {revision} onchanged={load} />
  {:else if tab === "planning"}
    <Planning {revision} />
  {:else if tab === "history"}
    <History />
  {/if}
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
  .nav-row {
    padding: 0 1rem 0.5rem;
    border-bottom: 1px solid var(--hairline);
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.72rem;
    color: var(--dim);
    white-space: nowrap;
  }
  .mode-switch {
    display: inline-flex;
    align-items: center;
    padding: 0.25rem 0.35rem;
  }
</style>
