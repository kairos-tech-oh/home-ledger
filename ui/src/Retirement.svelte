<script lang="ts">
  import PlanForm from "./PlanForm.svelte";
  import Projection from "./Projection.svelte";
  import { kindLabel, ledger, money, type RetirementView } from "./ledger";

  let { retirement, onchanged }: { retirement: RetirementView[]; onchanged: () => void } =
    $props();

  let editing = $state<RetirementView | null>(null);
  let busy = $state(false);
  let error = $state("");
  let said = $state("");

  async function run(action: () => Promise<string>) {
    busy = true;
    error = "";
    said = "";
    try {
      said = await action();
      editing = null;
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function savePlan(id: string, record: Record<string, unknown>) {
    run(async () => {
      await ledger.apply({ op: "retirement-set", id, record });
      return "";
    });
  }

  function topUp() {
    run(async () => {
      const done = await ledger.apply({ op: "retirement-accrue", id: "" });
      return done
        ? `Added ${money(done.entry.amount)} — ${done.entry.name}.`
        : "Nothing is due: every automatic top-up is up to date.";
    });
  }

  let view = $state<"cards" | "table">("table");

  const totals = $derived.by(() => {
    const sum = (pick: (r: RetirementView) => string) =>
      retirement.reduce((t, r) => t + Number(pick(r)), 0);
    return {
      value: sum((r) => r.value).toFixed(2),
      monthly: (sum((r) => r.monthly) + sum((r) => r.fromBudget)).toFixed(2),
      contributions: sum((r) => r.contributions).toFixed(2),
    };
  });

  const roth = $derived(retirement.filter((r) => r.kind === "retirement-roth"));
</script>

<section class="tiles">
  <article class="tile">
    <div class="tile-label">Put away</div>
    <div class="tile-figure pos">{money(totals.value)}</div>
    <div class="tile-note">across {retirement.length} accounts</div>
  </article>
  <article class="tile">
    <div class="tile-label">Going in</div>
    <div class="tile-figure">{money(totals.monthly)}</div>
    <div class="tile-note">a month, typed and from the budget</div>
  </article>
  <article class="tile">
    <div class="tile-label">Contributions</div>
    <div class="tile-figure">{money(totals.contributions)}</div>
    <div class="tile-note">
      {roth.length} Roth account{roth.length === 1 ? "" : "s"}, withdrawable
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Annual</div>
    <div class="tile-figure">{money((Number(totals.monthly) * 12).toFixed(2))}</div>
    <div class="tile-note">twelve times what goes in</div>
  </article>
</section>

<div class="section-head">
  <h2>{retirement.length} retirement accounts</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    <button onclick={topUp} disabled={busy}>Top up now</button>
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#if said}<p class="ok">{said}</p>{/if}

{#if editing}
  <PlanForm
    account={editing}
    {busy}
    onsave={(f) => savePlan(editing!.id, f)}
    oncancel={() => (editing = null)}
  />
{/if}

{#if view === "cards"}
  <div class="cards">
    {#each retirement as account (account.id)}
      <article class="card">
        <div class="card-top">
          <span class="pill violet">{kindLabel(account.kind)}</span>
          <span class="card-where">{account.institution}</span>
        </div>
        <div class="card-name">{account.name}</div>
        <div class="card-figure pos">{money(account.value)}</div>
        <div class="card-note">
          {money(account.monthly)} typed
          {#if Number(account.fromBudget) > 0} · {money(account.fromBudget)} from budget{/if}
          · {account.holdings} holding{account.holdings === 1 ? "" : "s"}
        </div>
        {#if Number(account.contributions) > 0}
          <div class="card-note">{money(account.contributions)} contributed</div>
        {/if}
        {#if account.autoContribute}
          <div class="card-note pos">
            topping up automatically{account.accruedThrough
              ? ` · through ${account.accruedThrough}`
              : ""}
          </div>
        {/if}
        {#if account.sleeves.length > 0}
          <div class="sleeves">
            {#each account.sleeves as sleeve (sleeve.name)}
              <span class="pill muted">{sleeve.name} {Number(sleeve.percent).toFixed(0)}%</span>
            {/each}
          </div>
        {/if}
        <div class="card-actions">
          <button class="bare" onclick={() => (editing = account)} disabled={busy}>Plan</button>
        </div>
      </article>
    {:else}
      <p class="empty">No retirement accounts yet.</p>
    {/each}
  </div>
{:else}
  <ul class="rows">
    {#each retirement as account (account.id)}
      <li class="row">
        <div class="row-main">
          <div class="head">
            <span class="pill violet">{kindLabel(account.kind)}</span>
            <span class="row-name">{account.name}</span>
            <span class="muted small">{account.institution}</span>
          </div>
          <div class="row-meta">
            {money(account.monthly)} typed
            {#if Number(account.fromBudget) > 0} · {money(account.fromBudget)} from budget{/if}
            · {account.holdings} holding{account.holdings === 1 ? "" : "s"}
            {#if Number(account.contributions) > 0}
              · {money(account.contributions)} contributed
            {/if}
            {#if account.autoContribute} · topping up automatically{/if}
            {#if account.sleeves.length > 0} · {account.sleeves.length} sleeves{/if}
          </div>
        </div>
        <span class="row-amount pos">{money(account.value)}</span>
        <div class="row-actions">
          <button class="bare" onclick={() => (editing = account)} disabled={busy}>Plan</button>
        </div>
      </li>
    {:else}
      <p class="empty">No retirement accounts yet.</p>
    {/each}
  </ul>
{/if}

<Projection />

<p class="note">
  Balances and holdings are edited on the Accounts and Holdings tabs; Plan sets
  the monthly contribution, automatic top-ups and the target weights.
</p>

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
  .small {
    font-size: 0.7rem;
  }
  .sleeves {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
    margin-top: 0.2rem;
  }
</style>
