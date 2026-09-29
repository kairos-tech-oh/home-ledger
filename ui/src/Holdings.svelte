<script lang="ts">
  import HoldingDetail from "./HoldingDetail.svelte";
  import HoldingForm from "./HoldingForm.svelte";
  import TypesPanel from "./TypesPanel.svelte";
  import {
    ledger,
    money,
    quotes,
    units,
    type AccountView,
    type BucketView,
    type HoldingView,
  } from "./ledger";

  let {
    holdings,
    accounts,
    buckets,
    types,
    fixedTypes,
    onchanged,
  }: {
    holdings: HoldingView[];
    accounts: AccountView[];
    buckets: BucketView[];
    types: string[];
    fixedTypes: string[];
    onchanged: () => void;
  } = $props();

  let adding = $state(false);
  let editing = $state<HoldingView | null>(null);
  let trading = $state<HoldingView | null>(null);
  let confirmDelete = $state<HoldingView | null>(null);
  let side = $state<"buy" | "sell">("buy");
  let quantity = $state("");
  let tradePrice = $state("");
  let tradeNotes = $state("");
  let busy = $state(false);
  let error = $state("");
  let managingTypes = $state(false);

  const open = $derived(adding || editing !== null || trading !== null);

  function close() {
    adding = false;
    editing = null;
    trading = null;
    confirmDelete = null;
  }

  function startTrade(holding: HoldingView, which: "buy" | "sell") {
    close();
    trading = holding;
    side = which;
    quantity = "";
    tradePrice = holding.price ?? "";
    tradeNotes = "";
    error = "";
  }

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      close();
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function save(id: string, record: Record<string, unknown>) {
    run(() => ledger.apply({ op: "set", kind: "holding", id, record }));
  }

  function trade() {
    const held = trading!;
    run(() =>
      ledger.apply({
        op: "trade",
        id: held.id,
        side,
        quantity: quantity.trim(),
        price: tradePrice.trim() || "0",
        notes: tradeNotes.trim(),
      }),
    );
  }

  let pricing = $state(false);
  let priceSaid = $state("");
  let priceError = $state("");

  async function refreshPrices() {
    pricing = true;
    priceError = "";
    priceSaid = "";
    try {
      const out = await quotes.refresh();
      priceSaid =
        out.attempted === 0
          ? "Nothing to price: every holding is priced by hand or has no symbol."
          : `${out.priced} of ${out.attempted} priced` +
            (out.failed > 0 ? ` · ${out.failed} left stale: ${out.errors.join(", ")}` : "");
      onchanged();
    } catch (e) {
      priceError = String(e);
    } finally {
      pricing = false;
    }
  }

  let opened = $state<string | null>(null);
  const showing = $derived(holdings.find((h) => h.id === opened) ?? null);

  let view = $state<"cards" | "table">("table");
  let groupBy = $state<"none" | "account">("account");

  const totals = $derived.by(() => {
    const value = holdings.reduce((t, h) => t + Number(h.value), 0);
    const basis = holdings.reduce((t, h) => t + Number(h.costBasis ?? 0), 0);
    return {
      value: value.toFixed(2),
      basis: basis.toFixed(2),
      gain: (value - basis).toFixed(2),
      percent: basis > 0 ? ((value - basis) / basis) * 100 : null,
    };
  });

  const grouped = $derived.by(() => {
    if (groupBy === "none") return [{ name: "", items: holdings, value: totals.value }];
    const seen = new Map<string, HoldingView[]>();
    for (const holding of holdings) {
      const key = holding.accountName || "No account";
      seen.set(key, [...(seen.get(key) ?? []), holding]);
    }
    return [...seen.entries()]
      .map(([name, items]) => ({
        name,
        items,
        value: items.reduce((t, h) => t + Number(h.value), 0).toFixed(2),
      }))
      .sort((a, b) => Number(b.value) - Number(a.value));
  });

  /// A fund with no live price is worth what it cost, which is worth saying.
  function priceNote(h: HoldingView): string {
    if (h.fixedPrice) return "fixed price";
    if (h.price === null) return `at cost ${money(h.avgCost)}`;
    return `${money(h.price)}${h.priceStale ? " (stale)" : ""}`;
  }
</script>

{#if showing}
  <HoldingDetail holding={showing} onclose={() => (opened = null)} />
{:else}
<section class="tiles">
  <article class="tile">
    <div class="tile-label">Value</div>
    <div class="tile-figure pos">{money(totals.value)}</div>
    <div class="tile-note">{holdings.length} holdings</div>
  </article>
  <article class="tile">
    <div class="tile-label">Cost</div>
    <div class="tile-figure">{money(totals.basis)}</div>
    <div class="tile-note">what was put in</div>
  </article>
  <article class="tile">
    <div class="tile-label">Gain</div>
    <div
      class="tile-figure"
      class:pos={Number(totals.gain) >= 0}
      class:neg={Number(totals.gain) < 0}
    >
      {money(totals.gain)}
    </div>
    <div class="tile-note">
      {totals.percent === null ? "against an unknown cost" : `${totals.percent.toFixed(1)}%`}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Accounts</div>
    <div class="tile-figure">{grouped.length}</div>
    <div class="tile-note">holding something</div>
  </article>
</section>

<div class="section-head">
  <h2>{holdings.length} holdings</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={groupBy === "account"} onclick={() => (groupBy = "account")}>
        By account
      </button>
      <button class:on={groupBy === "none"} onclick={() => (groupBy = "none")}>Flat</button>
    </div>
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    <button onclick={refreshPrices} disabled={pricing}>
      {pricing ? "Pricing…" : "Refresh prices"}
    </button>
    <button onclick={() => (managingTypes = !managingTypes)}>Types</button>
    {#if !open}
      <button onclick={() => { close(); adding = true; }} disabled={busy}>+ Holding</button>
    {/if}
  </div>
</div>

{#if priceError}<p class="error">{priceError}</p>{/if}
{#if priceSaid}<p class="ok">{priceSaid}</p>{/if}
{#if error}<p class="error">{error}</p>{/if}
{#if managingTypes}
  <TypesPanel list="investment" {types} fixed={fixedTypes} {onchanged} onclose={() => (managingTypes = false)} />
{/if}

{#if adding}
  <HoldingForm {accounts} {buckets} {types} {busy} onsave={(f) => save("", f)} oncancel={close} />
{:else if editing}
  <HoldingForm
    existing={editing}
    {accounts}
    {buckets}
    {types}
    {busy}
    onsave={(f) => save(editing!.id, f)}
    oncancel={close}
  />
{/if}

{#if trading}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); trade(); }}>
    <h3>{side === "buy" ? "Buy more" : "Sell some"} of {trading.name}</h3>
    <div class="toggle">
      <button type="button" class:on={side === "buy"} onclick={() => (side = "buy")}>Buy</button>
      <button type="button" class:on={side === "sell"} onclick={() => (side = "sell")}>
        Sell
      </button>
    </div>
    <label class="field">
      <span>Quantity</span>
      <input bind:value={quantity} inputmode="decimal" required />
    </label>
    <label class="field">
      <span>Price each</span>
      <input bind:value={tradePrice} inputmode="decimal" required />
    </label>
    <label class="field">
      <span>Note</span>
      <input bind:value={tradeNotes} placeholder="optional — why, or where it went" />
    </label>
    <p class="note">
      {units(trading.quantity)} held at an average of {money(trading.avgCost)}.
      {side === "buy"
        ? "A buy re-averages the cost."
        : "A sell leaves the average alone and lowers the cost basis."}
    </p>
    <div class="actions">
      <button type="submit" disabled={busy}>{busy ? "Saving…" : "Record trade"}</button>
      <button type="button" class="bare" onclick={close}>Cancel</button>
    </div>
  </form>
{/if}

{#if confirmDelete}
  <div class="warn-box">
    <p class="confirm-text">
      Delete <strong>{confirmDelete.name}</strong> and its
      {confirmDelete.trades.length} recorded trade{confirmDelete.trades.length === 1 ? "" : "s"}?
    </p>
    <div class="actions">
      <button
        disabled={busy}
        onclick={() =>
          run(() => ledger.apply({ op: "delete", kind: "holding", id: confirmDelete!.id }))}
      >
        Delete
      </button>
      <button class="bare" onclick={() => (confirmDelete = null)}>Keep it</button>
    </div>
  </div>
{/if}

{#each grouped as group (group.name)}
  {#if group.name}
    <div class="group-head">
      <span class="pill blue">{group.name}</span>
      <span class="group-count">
        {group.items.length} holding{group.items.length === 1 ? "" : "s"}
      </span>
      <span class="group-total">{money(group.value)}</span>
    </div>
  {/if}

  {#if view === "cards"}
    <div class="cards">
      {#each group.items as holding (holding.id)}
        <article class="card">
          <div class="card-top">
            {#if holding.ticker}<span class="pill violet">{holding.ticker}</span>{/if}
            <span class="card-where">{holding.kind}</span>
          </div>
          <button class="reveal card-name" onclick={() => (opened = holding.id)}>
            {holding.name}
          </button>
          <div class="card-figure">{money(holding.value)}</div>
          <div class="card-note">
            {units(holding.quantity)} × {priceNote(holding)}
          </div>
          <div class="card-note">
            {#if holding.gain === null}
              no cost recorded
            {:else}
              <span class:pos={Number(holding.gain) >= 0} class:neg={Number(holding.gain) < 0}>
                {money(holding.gain)}
                {#if holding.gainPercent !== null}{holding.gainPercent.toFixed(1)}%{/if}
              </span>
            {/if}
            {#if holding.bucketName} · {holding.bucketName}{/if}
          </div>
          <div class="card-actions">
            {@render actions(holding)}
          </div>
        </article>
      {/each}
    </div>
  {:else}
    <ul class="rows">
      {#each group.items as holding (holding.id)}
        <li class="row holding-row">
          <div class="row-main">
            <div class="head">
              {#if holding.ticker}<span class="pill violet">{holding.ticker}</span>{/if}
              <button class="reveal row-name" onclick={() => (opened = holding.id)}>
                {holding.name}
              </button>
              <span class="muted small">{holding.kind}</span>
            </div>
            <div class="row-meta">
              {units(holding.quantity)} × {priceNote(holding)}
              {#if holding.costBasis} · cost {money(holding.costBasis)}{/if}
              {#if holding.bucketName} · {holding.bucketName}{/if}
              {#if holding.trades.length > 0}
                · {holding.trades.length} trade{holding.trades.length === 1 ? "" : "s"}
              {/if}
            </div>
          </div>
          <span class="row-amount">{money(holding.value)}</span>
          <span class="gain">
            {#if holding.gain === null}
              <span class="muted small">—</span>
            {:else}
              <span class:pos={Number(holding.gain) >= 0} class:neg={Number(holding.gain) < 0}>
                {money(holding.gain)}
                {#if holding.gainPercent !== null}
                  <span class="small">{holding.gainPercent.toFixed(1)}%</span>
                {/if}
              </span>
            {/if}
          </span>
          <div class="row-actions">{@render actions(holding)}</div>
        </li>
      {/each}
    </ul>
  {/if}
{:else}
  <p class="empty">No holdings yet.</p>
{/each}

<p class="note">
  Click a name for its chart and what the ledger records against it. To price
  something by hand, edit it and tick "priced by hand".
</p>
{/if}

{#snippet actions(holding: HoldingView)}
  <button class="bare" onclick={() => startTrade(holding, "buy")} disabled={busy} title="Buy">
    +
  </button>
  <button
    class="bare"
    onclick={() => startTrade(holding, "sell")}
    disabled={busy || Number(holding.quantity) <= 0}
    title="Sell">−</button
  >
  <button class="bare" onclick={() => { close(); editing = holding; }} disabled={busy}>
    Edit
  </button>
  <button class="bare" onclick={() => { close(); confirmDelete = holding; }} disabled={busy}>
    Delete
  </button>
{/snippet}

<style>
  .reveal {
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .reveal:hover {
    text-decoration: underline;
    background: none;
  }
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
  .holding-row {
    grid-template-columns: 1fr auto auto auto;
  }
  .gain {
    font-variant-numeric: tabular-nums;
    min-width: 9rem;
    text-align: right;
  }
</style>
