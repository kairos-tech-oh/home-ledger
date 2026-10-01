<script lang="ts">
  import { ledger, money, type BucketView, type PaydayView } from "./ledger";
  import { largestFirst } from "./order";

  let {
    buckets,
    paydays = [],
    onchanged,
  }: { buckets: BucketView[]; paydays?: PaydayView[]; onchanged: () => void } = $props();

  // A payday adds one paycheck's worth to every bucket that earner funds, as
  // one edit; its undo takes the same amounts back out.
  function payday(day: PaydayView, undo: boolean) {
    run(() =>
      ledger.apply({
        op: "bucket-adjust",
        adjustments: day.adjustments.map((a) => ({ id: a.id, delta: undo ? `-${a.delta}` : a.delta })),
        label: `${undo ? "Undo " : ""}${day.owner}'s payday`,
      }),
    );
  }

  let adding = $state(false);
  let editing = $state<BucketView | null>(null);
  let moving = $state<{ bucket: BucketView; mode: "add" | "spend" | "set" | "move" } | null>(
    null,
  );
  let amount = $state("");
  let label = $state("");
  let toBucket = $state("");
  let busy = $state(false);
  let error = $state("");
  let view = $state<"cards" | "table">("cards");

  let name = $state("");
  let target = $state("");
  let notes = $state("");
  let locked = $state(false);
  let whenShort = $state<BucketView["whenShort"]>("");
  let coverBucketId = $state("");

  const open = $derived(adding || editing !== null || moving !== null);
  const ordered = $derived(largestFirst(buckets));

  const totals = $derived.by(() => {
    const sum = (pick: (b: BucketView) => string) =>
      buckets.reduce((t, b) => t + Number(pick(b)), 0).toFixed(2);
    return {
      total: sum((b) => b.total),
      cash: sum((b) => b.cash),
      invested: sum((b) => b.invested),
      funded: sum((b) => b.fundedMonthly),
    };
  });

  function edit(bucket: BucketView | null) {
    editing = bucket;
    adding = bucket === null;
    moving = null;
    name = bucket?.name ?? "";
    target = bucket?.target ?? "";
    notes = bucket?.notes ?? "";
    locked = bucket?.locked ?? false;
    whenShort = bucket?.whenShort ?? "";
    coverBucketId = bucket?.coverBucketId ?? "";
    error = "";
  }

  function start(bucket: BucketView, mode: "add" | "spend" | "set" | "move") {
    moving = { bucket, mode };
    editing = null;
    adding = false;
    amount = mode === "set" ? bucket.cash : "";
    label = "";
    toBucket = "";
    error = "";
  }

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      moving = null;
      editing = null;
      adding = false;
      amount = "";
      label = "";
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function saveBucket() {
    run(() =>
      ledger.apply({
        op: "set",
        kind: "bucket",
        id: editing?.id ?? "",
        record: {
          name: name.trim(),
          targetAmount: target.trim() === "" ? null : target.trim(),
          notes: notes.trim(),
          locked,
          whenShort: whenShort === "bucket" && !coverBucketId ? "" : whenShort,
          coverBucketId: whenShort === "bucket" ? coverBucketId : "",
        },
      }),
    );
  }

  function applyMoney() {
    const bucket = moving!.bucket;
    const value = amount.trim();
    if (!value) return;

    switch (moving!.mode) {
      case "add":
        return run(() =>
          ledger.apply({
            op: "bucket-adjust",
            adjustments: [{ id: bucket.id, delta: value }],
            label: label.trim() || "Added",
          }),
        );
      case "spend":
        return run(() =>
          ledger.apply({
            op: "bucket-adjust",
            adjustments: [{ id: bucket.id, delta: `-${value.replace(/^-/, "")}` }],
            label: label.trim() || "Spent",
          }),
        );
      case "set":
        return run(() => ledger.apply({ op: "bucket-total", id: bucket.id, amount: value }));
      case "move":
        return run(() =>
          ledger.apply({ op: "bucket-move", fromId: bucket.id, toId: toBucket, amount: value }),
        );
    }
  }

  const verb = $derived(
    moving?.mode === "add"
      ? "Add to"
      : moving?.mode === "spend"
        ? "Spend from"
        : moving?.mode === "set"
          ? "Set the balance of"
          : "Move out of",
  );
</script>

<section class="tiles">
  <article class="tile">
    <div class="tile-label">Saved</div>
    <div class="tile-figure pos">{money(totals.total)}</div>
    <div class="tile-note">across {buckets.length} buckets</div>
  </article>
  <article class="tile">
    <div class="tile-label">Cash</div>
    <div class="tile-figure">{money(totals.cash)}</div>
    <div class="tile-note">sitting in the buckets</div>
  </article>
  <article class="tile">
    <div class="tile-label">Invested</div>
    <div class="tile-figure">{money(totals.invested)}</div>
    <div class="tile-note">held in a bucket's name</div>
  </article>
  <article class="tile">
    <div class="tile-label">Funded</div>
    <div class="tile-figure">{money(totals.funded)}</div>
    <div class="tile-note">a month from the budget</div>
  </article>
</section>

{#if paydays.length > 0}
  <div class="paydays">
    <span class="muted">Payday, one paycheck each:</span>
    {#each paydays as day (day.owner)}
      <span class="payday">
        <button
          onclick={() => payday(day, false)}
          disabled={busy}
          title="Add {money(day.total)} across the {day.adjustments.length} buckets {day.owner} funds"
        >
          Add all · {day.owner} {money(day.total)}
        </button>
        <button
          class="bare"
          onclick={() => payday(day, true)}
          disabled={busy}
          title="Take {day.owner}'s payday back out of those buckets">Undo</button
        >
      </span>
    {/each}
  </div>
{/if}

<div class="section-head">
  <h2>{buckets.length} buckets</h2>
  <div class="controls">
    <div class="toggle">
      <button class:on={view === "cards"} onclick={() => (view = "cards")}>Cards</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    {#if !open}
      <button onclick={() => edit(null)} disabled={busy}>+ Bucket</button>
    {/if}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if adding || editing}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); saveBucket(); }}>
    <h3>{editing ? `Edit ${editing.name}` : "Add a bucket"}</h3>
    <label class="field"><span>Name</span><input bind:value={name} required /></label>
    <label class="field">
      <span>Target</span>
      <input bind:value={target} inputmode="decimal" placeholder="optional" />
    </label>
    <label class="field">
      <span>Notes</span><input bind:value={notes} placeholder="optional" />
    </label>
    <label class="field check">
      <input type="checkbox" bind:checked={locked} />
      <span>Locked — funds cannot be moved out of this bucket</span>
    </label>
    <label class="field">
      <span>When a statement needs more than it holds</span>
      <select bind:value={whenShort}>
        <option value="">Ask when settling</option>
        <option value="bucket">Take the rest from another bucket</option>
        <option value="everyday">Charge the rest to everyday spending</option>
        <option value="negative">Let it go below zero</option>
      </select>
    </label>
    {#if whenShort === "bucket"}
      <label class="field">
        <span>The rest comes from</span>
        <select bind:value={coverBucketId}>
          <option value="" disabled>Choose a bucket</option>
          {#each ordered.filter((b) => b.id !== editing?.id && !b.locked) as other (other.id)}
            <option value={other.id}>{other.name}</option>
          {/each}
        </select>
      </label>
    {/if}
    <div class="actions">
      <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save"}</button>
      <button type="button" class="bare" onclick={() => { editing = null; adding = false; }}>
        Cancel
      </button>
    </div>
  </form>
{/if}

{#if moving}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); applyMoney(); }}>
    <h3>{verb} {moving.bucket.name}</h3>
    <label class="field">
      <span>Amount</span>
      <input bind:value={amount} inputmode="decimal" required />
    </label>
    {#if moving.mode === "move"}
      <label class="field">
        <span>Into</span>
        <select bind:value={toBucket} required>
          <option value="" disabled>Choose a bucket</option>
          {#each ordered.filter((b) => b.id !== moving!.bucket.id) as other (other.id)}
            <option value={other.id}>{other.name}</option>
          {/each}
        </select>
      </label>
    {:else if moving.mode !== "set"}
      <label class="field">
        <span>What for</span>
        <input bind:value={label} placeholder="optional" />
      </label>
    {/if}
    <div class="actions">
      <button type="submit" disabled={busy}>{busy ? "Working…" : "Confirm"}</button>
      <button type="button" class="bare" onclick={() => (moving = null)}>Cancel</button>
    </div>
  </form>
{/if}

{#if view === "cards"}
  <div class="cards">
    {#each ordered as bucket (bucket.id)}
      <article class="card">
        <div class="card-top">
          {#if bucket.locked}<span class="pill muted">locked</span>{/if}
          {#if bucket.target}<span class="card-where">of {money(bucket.target)}</span>{/if}
        </div>
        <div class="card-name">{bucket.name}</div>
        <div class="card-figure" class:pos={!bucket.total.startsWith("-")} class:neg={bucket.total.startsWith("-")}>{money(bucket.total)}</div>
        {#if bucket.progress !== null && bucket.target}
          <span class="bar" title="{bucket.progress.toFixed(0)}% of {money(bucket.target)}">
            <span class="fill" style:width="{Math.min(bucket.progress, 100)}%"></span>
          </span>
        {/if}
        {#if bucket.committed !== "0.00"}
          <div class="card-note" class:warn-text={bucket.committedShort !== null}>
            {money(bucket.committed)} on open statements{#if bucket.committedShort !== null}
              · {money(bucket.committedShort)} short{/if}
          </div>
        {/if}
        <div class="card-note">
          cash {money(bucket.cash)}
          {#if bucket.invested !== "0.00"} · invested {money(bucket.invested)}{/if}
          {#if bucket.fundedBy.length > 0} · {money(bucket.fundedMonthly)}/mo{/if}
        </div>
        <div class="card-actions">
          <button class="bare" onclick={() => start(bucket, "add")} disabled={busy} title="Add">
            +
          </button>
          <button
            class="bare"
            onclick={() => start(bucket, "spend")}
            disabled={busy}
            title="Spend">−</button
          >
          <button
            class="bare"
            onclick={() => start(bucket, "move")}
            disabled={busy || bucket.locked || buckets.length < 2}
            title={bucket.locked ? "This bucket is locked" : "Move"}>→</button
          >
          <button
            class="bare"
            onclick={() => start(bucket, "set")}
            disabled={busy}
            title="Set balance">=</button
          >
          <button class="bare" onclick={() => edit(bucket)} disabled={busy}>Edit</button>
        </div>
      </article>
    {:else}
      <p class="empty">No savings buckets yet.</p>
    {/each}
  </div>
{:else}
<ul class="rows">
  {#each ordered as bucket (bucket.id)}
    <li class="row">
      <div class="row-main">
        <div class="head">
          <span class="row-name">{bucket.name}</span>
          {#if bucket.locked}<span class="pill muted">locked</span>{/if}
          {#if bucket.progress !== null && bucket.target}
            <span class="bar" title="{bucket.progress.toFixed(0)}% of {money(bucket.target)}">
              <span class="fill" style:width="{Math.min(bucket.progress, 100)}%"></span>
            </span>
          {/if}
        </div>
        <div class="row-meta">
          cash {money(bucket.cash)}
          {#if bucket.invested !== "0.00"} · invested {money(bucket.invested)}{/if}
          {#if bucket.contributions !== "0.00"} · contributions {money(bucket.contributions)}{/if}
          {#if bucket.target} · of {money(bucket.target)}{/if}
          {#if bucket.fundedBy.length > 0}
            · {money(bucket.fundedMonthly)}/mo from {bucket.fundedBy.length} line{bucket
              .fundedBy.length === 1
              ? ""
              : "s"}
          {/if}
        </div>
      </div>

      <span class="row-amount" class:pos={!bucket.total.startsWith("-")} class:neg={bucket.total.startsWith("-")}>{money(bucket.total)}</span>

      <div class="row-actions">
        <button class="bare" onclick={() => start(bucket, "add")} disabled={busy} title="Add">
          +
        </button>
        <button class="bare" onclick={() => start(bucket, "spend")} disabled={busy} title="Spend">
          −
        </button>
        <button
          class="bare"
          onclick={() => start(bucket, "move")}
          disabled={busy || bucket.locked || buckets.length < 2}
          title={bucket.locked ? "This bucket is locked" : "Move"}>→</button
        >
        <button
          class="bare"
          onclick={() => start(bucket, "set")}
          disabled={busy}
          title="Set balance">=</button
        >
        <button class="bare" onclick={() => edit(bucket)} disabled={busy}>Edit</button>
      </div>
    </li>
  {:else}
    <p class="empty">No savings buckets yet.</p>
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
  .bar {
    display: inline-block;
    width: 6rem;
    height: 3px;
    border-radius: 2px;
    background: var(--hairline);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--positive);
  }
  .paydays {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.9rem;
    margin: 0.75rem 0 0.25rem;
  }
  .payday {
    display: inline-flex;
    gap: 0.15rem;
  }
</style>
