<script lang="ts">
  import AccountForm from "./AccountForm.svelte";
  import { accountKinds, kindLabel, ledger, money, type AccountView } from "./ledger";
  import type { Overview } from "./lib";
  import { sections } from "./order";

  let {
    accounts,
    overview,
    onchanged,
  }: { accounts: AccountView[]; overview: Overview | null; onchanged: () => void } =
    $props();

  let editing = $state<AccountView | null>(null);
  let adding = $state(false);
  let busy = $state(false);
  let error = $state("");
  let confirmDelete = $state<AccountView | null>(null);
  let view = $state<"cards" | "table">("cards");

  const open = $derived(adding || editing !== null);

  // One section per kind of account, in the order the kinds are offered.
  const grouped = $derived(
    sections(accounts, (a) => a.kind, accountKinds.map((k) => k.value)).map((s) => ({
      ...s,
      net: s.items.reduce((t, a) => t + Number(a.net), 0).toFixed(2),
    })),
  );

  /// The plugin colours a badge by what kind of thing the account is.
  function pillColour(kind: string): string {
    if (kind.startsWith("retirement")) return "violet";
    if (kind === "credit" || kind === "heloc") return "red";
    if (kind === "loan") return "amber";
    if (kind === "investment") return "blue";
    return "green";
  }

  const debtRatio = $derived.by(() => {
    if (!overview) return null;
    const assets = Number(overview.assets);
    const debts = Number(overview.debts);
    if (!Number.isFinite(assets) || assets <= 0) return null;
    return Math.round((debts / assets) * 100);
  });

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      adding = false;
      editing = null;
      confirmDelete = null;
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if overview}
  <section class="tiles">
    <article class="tile">
      <div class="tile-label">Assets</div>
      <div class="tile-figure pos">{money(overview.assets)}</div>
      <div class="tile-note">held across these accounts</div>
    </article>
    <article class="tile">
      <div class="tile-label">Debts</div>
      <div class="tile-figure neg">{money(overview.debts)}</div>
      <div class="tile-note">cards, loans and mortgages</div>
    </article>
    <article class="tile">
      <div class="tile-label">Net worth</div>
      <div
        class="tile-figure"
        class:neg={overview.net.startsWith("-")}
        class:pos={!overview.net.startsWith("-")}
      >
        {money(overview.net)}
      </div>
      <div class="tile-note">assets less debts</div>
    </article>
    <article class="tile">
      <div class="tile-label">Debt ratio</div>
      <div class="tile-figure">{debtRatio === null ? "—" : `${debtRatio}%`}</div>
      <div class="tile-note">of what you hold is owed</div>
    </article>
  </section>
{/if}

<div class="section-head">
  <h2>{accounts.length} accounts</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    {#if !open}
      <button onclick={() => (adding = true)} disabled={busy}>+ Account</button>
    {/if}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if adding}
  <AccountForm
    onsave={(f) => run(() => ledger.apply({ op: "set", kind: "account", id: "", record: f }))}
    oncancel={() => (adding = false)}
    {busy}
  />
{:else if editing}
  <AccountForm
    existing={editing}
    onsave={(f) =>
      run(() => ledger.apply({ op: "set", kind: "account", id: editing!.id, record: f }))}
    oncancel={() => (editing = null)}
    {busy}
  />
{/if}

{#if confirmDelete}
  <div class="warn-box">
    <p class="confirm-text">
      Delete <strong>{confirmDelete.name}</strong>? Anything pointing at it — income,
      budget lines, holdings — keeps its figures but stops pointing anywhere.
    </p>
    <div class="actions">
      <button
        disabled={busy}
        onclick={() =>
          run(() => ledger.apply({ op: "delete", kind: "account", id: confirmDelete!.id }))}
      >
        Delete
      </button>
      <button class="bare" onclick={() => (confirmDelete = null)}>Keep it</button>
    </div>
  </div>
{/if}

{#each grouped as group (group.key)}
  <div class="group-head">
    <span class="pill {pillColour(group.key)}">{kindLabel(group.key)}</span>
    <span class="group-count">
      {group.items.length} account{group.items.length === 1 ? "" : "s"}
    </span>
    <span class="group-total" class:neg={group.net.startsWith("-")}>{money(group.net)}</span>
  </div>

  {#if view === "cards"}
    <div class="cards">
      {#each group.items as account (account.id)}
        <article class="card">
          {#if account.institution}
            <div class="card-top"><span class="card-where">{account.institution}</span></div>
          {/if}
          <div class="card-name">{account.name}</div>
          <div class="card-figure" class:neg={account.liability} class:pos={!account.liability}>
            {money(account.net)}
          </div>
          <div class="card-note">
            {account.liability ? "owed" : "cash"}
            {money(account.total)}
            {#if account.holdingCount > 0}
              · {account.holdingCount} holding{account.holdingCount === 1 ? "" : "s"}
            {/if}
            {#if account.availableCredit !== null} · {money(account.availableCredit)} available{/if}
          </div>
          <div class="card-actions">
            <button class="bare" onclick={() => (editing = account)} disabled={busy}>Edit</button>
            <button
              class="bare danger"
              onclick={() => (confirmDelete = account)}
              disabled={busy}
              title="Delete">×</button
            >
          </div>
        </article>
      {/each}
    </div>
  {:else}
    <ul class="rows">
      {#each group.items as account (account.id)}
        <li class="row">
          <div class="row-main">
            <div class="head">
              <span class="row-name">{account.name}</span>
              {#if account.institution}<span class="muted where">{account.institution}</span>{/if}
            </div>
            <div class="row-meta">
              {account.liability ? "owed" : "cash"}
              {money(account.total)}
              {#if account.holdingCount > 0}
                · {account.holdingCount} holding{account.holdingCount === 1 ? "" : "s"}
                {money(account.holdings)}
              {/if}
              {#if account.debtCount > 0}
                · {account.debtCount} debt{account.debtCount === 1 ? "" : "s"}
                {money(account.debts)}
              {/if}
              {#if account.availableCredit !== null}
                · {money(account.availableCredit)} available
              {/if}
            </div>
          </div>

          <span class="row-amount" class:neg={account.liability} class:pos={!account.liability}>
            {money(account.net)}
          </span>

          <div class="row-actions">
            <button class="bare" onclick={() => (editing = account)} disabled={busy}>Edit</button>
            <button
              class="bare danger"
              onclick={() => (confirmDelete = account)}
              disabled={busy}
              title="Delete">×</button
            >
          </div>
        </li>
      {/each}
    </ul>
  {/if}
{:else}
  <p class="empty">No accounts yet.</p>
{/each}

<style>
  .controls {
    display: flex;
    gap: 0.6rem;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    min-width: 0;
  }
  .where {
    font-size: 0.7rem;
  }
  .confirm-text {
    margin: 0 0 0.4rem;
  }
</style>
