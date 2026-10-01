<script lang="ts">
  import StatementForm from "./StatementForm.svelte";
  import ImportPanel from "./ImportPanel.svelte";
  import ShortfallChoice, { type Choice } from "./ShortfallChoice.svelte";
  import {
    ledger,
    money,
    when,
    type AccountView,
    type BucketView,
    type ReconciliationView,
  } from "./ledger";

  let {
    reconciliations,
    accounts,
    buckets,
    members,
    onchanged,
  }: {
    reconciliations: ReconciliationView[];
    accounts: AccountView[];
    buckets: BucketView[];
    members: string[];
    onchanged: () => void;
  } = $props();

  let openId = $state<string | null>(null);
  let adding = $state(false);
  let editing = $state<ReconciliationView | null>(null);
  let importing = $state<ReconciliationView | null>(null);
  // Settle, undo and delete all ask first: the first two move real money.
  let confirming = $state<{
    record: ReconciliationView;
    action: "settle" | "undo" | "delete";
  } | null>(null);
  let busy = $state(false);
  let error = $state("");

  const formOpen = $derived(adding || editing !== null);

  function close() {
    adding = false;
    editing = null;
    importing = null;
    confirming = null;
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
    run(() => ledger.apply({ op: "reconcile-set", id, record }));
  }

  // How each short bucket is covered, for the statement being settled.
  // Starts on each bucket's own default.
  let choices = $state<Record<string, Choice>>({});

  function askSettle(record: ReconciliationView) {
    close();
    choices = Object.fromEntries(
      record.shortfalls.map((s) => [
        s.bucketId,
        { how: s.whenShort, fromBucketId: s.coverBucketId, remember: false } satisfies Choice,
      ]),
    );
    confirming = { record, action: "settle" };
  }

  // Every short bucket needs a way to be covered before the settle can run.
  const coversChosen = $derived(
    !confirming ||
      confirming.action !== "settle" ||
      confirming.record.shortfalls.every((s) => {
        const c = choices[s.bucketId];
        return c && c.how !== "" && (c.how !== "bucket" || c.fromBucketId !== "");
      }),
  );

  function confirmed() {
    const { record, action } = confirming!;
    if (action !== "settle") {
      const op =
        action === "undo"
          ? ({ op: "reconcile-undo", id: record.id } as const)
          : ({ op: "reconcile-delete", id: record.id } as const);
      run(() => ledger.apply(op));
      return;
    }
    const cover = record.shortfalls.map((s) => {
      const c = choices[s.bucketId];
      return {
        bucketId: s.bucketId,
        how: c.how as "bucket" | "everyday" | "negative",
        fromBucketId: c.how === "bucket" ? c.fromBucketId : "",
      };
    });
    run(async () => {
      // Remembered first, so the bucket keeps its default even if the settle
      // is then refused for another reason.
      for (const c of cover) {
        if (!choices[c.bucketId].remember) continue;
        await ledger.apply({
          op: "set",
          kind: "bucket",
          id: c.bucketId,
          record: { whenShort: c.how, coverBucketId: c.fromBucketId },
        });
      }
      await ledger.apply({ op: "reconcile-settle", id: record.id, cover });
    });
  }

  /// Buckets the open statements together need more from than they hold.
  const overdrawn = $derived(buckets.filter((b) => b.committedShort !== null));

  const question = $derived.by(() => {
    if (!confirming) return "";
    const r = confirming.record;
    switch (confirming.action) {
      case "settle":
        return r.adjustAccounts
          ? `Settle ${r.card}? Each bucket line comes out of its bucket, the money comes out of the chosen accounts, and the card is paid down by ${money(r.balance)}.`
          : `Settle ${r.card}? Each bucket line comes out of its bucket and the card is paid down by ${money(r.balance)}. Account balances are left alone.`;
      case "undo":
        return `Undo settling ${r.card}? Every bucket and account gets back exactly what the settle took, and the statement reopens.`;
      case "delete":
        return `Delete the ${r.card} statement and its ${r.lines.length} charges? The card balance it set stays as it is.`;
    }
  });

  const open = $derived(reconciliations.filter((r) => r.status !== "settled"));
  const settled = $derived(reconciliations.filter((r) => r.status === "settled"));
  const openBalance = $derived(
    open.reduce((t, r) => t + Number(r.balance), 0).toFixed(2),
  );

  /// Zero unaccounted is what makes a statement ready to settle.
  function ready(record: ReconciliationView): boolean {
    return Math.abs(Number(record.unaccounted)) < 0.005;
  }
</script>

<section class="tiles">
  <article class="tile">
    <div class="tile-label">Open</div>
    <div class="tile-figure neg">{money(openBalance)}</div>
    <div class="tile-note">
      across {open.length} statement{open.length === 1 ? "" : "s"}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Ready</div>
    <div class="tile-figure">{open.filter(ready).length}</div>
    <div class="tile-note">fully accounted for</div>
  </article>
  <article class="tile">
    <div class="tile-label">Settled</div>
    <div class="tile-figure">{settled.length}</div>
    <div class="tile-note">already paid off</div>
  </article>
  <article class="tile">
    <div class="tile-label">Charges</div>
    <div class="tile-figure">
      {reconciliations.reduce((t, r) => t + r.lines.length, 0)}
    </div>
    <div class="tile-note">recorded against a statement</div>
  </article>
</section>

<div class="section-head">
  <h2>{reconciliations.length} reconciliations</h2>
  {#if !formOpen}
    <button onclick={() => { close(); adding = true; }} disabled={busy}>+ Statement</button>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if overdrawn.length > 0}
  <div class="warn-box">
    <p>The open statements together need more than these buckets hold:</p>
    <ul class="short-list">
      {#each overdrawn as b (b.id)}
        <li>
          <strong>{b.name}</strong>: {money(b.committed)} across open statements · holds
          {money(b.cash)} · <span class="neg">{money(b.committedShort)} short</span>
          {#if b.whenShort === "bucket"} · the rest will come from {b.coverBucketName}
          {:else if b.whenShort === "everyday"} · the rest will be everyday spending
          {:else if b.whenShort === "negative"} · it will go below zero
          {:else} · you will be asked when settling{/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}

{#if importing}
  <ImportPanel record={importing} {buckets} {members} {onchanged} onclose={close} />
{/if}

{#if adding}
  <StatementForm {accounts} {buckets} {members} {busy} onsave={(f) => save("", f)} oncancel={close} />
{:else if editing}
  <StatementForm
    existing={editing}
    {accounts}
    {buckets}
    {members}
    {busy}
    onsave={(f) => save(editing!.id, f)}
    oncancel={close}
  />
{/if}

{#if confirming}
  <div class="warn-box">
    <p class="confirm-text">{question}</p>
    {#if confirming.action === "settle"}
      {#each confirming.record.shortfalls as s (s.bucketId)}
        <ShortfallChoice short={s} {buckets} bind:choice={choices[s.bucketId]} />
      {/each}
    {/if}
    <div class="actions">
      <button disabled={busy || !coversChosen} onclick={confirmed}>
        {confirming.action === "settle"
          ? "Settle"
          : confirming.action === "undo"
            ? "Undo settle"
            : "Delete"}
      </button>
      <button class="bare" onclick={() => (confirming = null)}>Not now</button>
    </div>
  </div>
{/if}

<ul class="rows">
  {#each reconciliations as record (record.id)}
    <li class="row">
      <div class="row-main">
        <div class="head">
          <span class="pill" class:red={record.status !== "settled"} class:green={record.status === "settled"}>
            {record.status}
          </span>
          <span class="row-name">{record.card}</span>
          {#if record.statementDate}
            <span class="muted small">{record.statementDate}</span>
          {/if}
          {#if record.status !== "settled" && ready(record)}
            <span class="pill green">ready</span>
          {/if}
        </div>
        <div class="row-meta">
          {record.lines.length} charge{record.lines.length === 1 ? "" : "s"}
          {money(record.linesTotal)}
          {#if !ready(record)} · {money(record.unaccounted)} unaccounted{/if}
          {#if record.cardAccountName} · {record.cardAccountName}{/if}
          {#if record.settledAt} · settled {when(record.settledAt)}{/if}
        </div>
        {#each record.shortfalls as s (s.bucketId)}
          <div class="row-meta warn-text">
            {s.bucketName} holds {money(s.holds)} of the {money(s.needs)} this takes
          </div>
        {/each}
      </div>
      <span class="row-amount" class:neg={record.status !== "settled"}>
        {money(record.balance)}
      </span>
      <div class="row-actions">
        <button
          class="bare"
          onclick={() => (openId = openId === record.id ? null : record.id)}
          disabled={record.lines.length === 0}
        >
          {openId === record.id ? "Hide" : "Charges"}
        </button>
        {#if record.status === "settled"}
          <button
            class="bare"
            onclick={() => { close(); confirming = { record, action: "undo" }; }}
            disabled={busy}>Undo</button
          >
        {:else}
          <button
            class="bare"
            onclick={() => askSettle(record)}
            disabled={busy || !ready(record)}
            title={ready(record) ? "Settle" : "The charges must add up to the balance first"}
            >Settle</button
          >
          <button class="bare" onclick={() => { close(); importing = record; }} disabled={busy}>
            Import
          </button>
          <button class="bare" onclick={() => { close(); editing = record; }} disabled={busy}>
            Edit
          </button>
          <button
            class="bare"
            onclick={() => { close(); confirming = { record, action: "delete" }; }}
            disabled={busy}>Delete</button
          >
        {/if}
      </div>
    </li>

    {#if openId === record.id}
      <li class="lines">
        <ul class="rows">
          {#each record.lines as line (line.id)}
            <li class="row">
              <div class="row-main">
                <div class="row-name">{line.label || "Unnamed charge"}</div>
                <div class="row-meta">
                  {#if line.member}{line.member}{/if}
                  {#if line.spentOn} · {line.spentOn}{/if}
                  {#if line.bucketName} · {line.bucketName}{:else} · everyday spending{/if}
                </div>
              </div>
              <span class="row-amount">{money(line.amount)}</span>
              <span></span>
            </li>
          {/each}
        </ul>
      </li>
    {/if}
  {:else}
    <p class="empty">No reconciliations yet.</p>
  {/each}
</ul>

<p class="note">
  A settled statement is history: undo it to change it. Undo returns exactly what
  the settle moved, even if the charges or balances have changed since.
</p>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }
  .small {
    font-size: 0.7rem;
  }
  .lines {
    margin: 0 0 0.4rem 1.5rem;
    padding-left: 0.6rem;
    border-left: 1px solid var(--hairline);
  }
  .short-list {
    margin: 0.2rem 0 0;
    padding-left: 1.2rem;
  }
</style>
