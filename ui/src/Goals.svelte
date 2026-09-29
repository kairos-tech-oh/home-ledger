<script lang="ts">
  import { ledger, money, wholeMoney, type BucketView, type GoalTotalsView, type GoalView } from "./ledger";
  import { largestFirst } from "./order";

  let {
    goals,
    totals,
    buckets,
    onchanged,
  }: {
    goals: GoalView[];
    totals: GoalTotalsView;
    buckets: BucketView[];
    onchanged: () => void;
  } = $props();

  let adding = $state(false);
  let editing = $state<GoalView | null>(null);
  let confirmDelete = $state<GoalView | null>(null);
  let busy = $state(false);
  let error = $state("");

  let name = $state("");
  let target = $state("");
  let bucketId = $state("");
  let notes = $state("");

  const open = $derived(adding || editing !== null);
  const bucketChoices = $derived(largestFirst(buckets));
  const chosen = $derived(buckets.find((b) => b.id === bucketId) ?? null);

  function edit(goal: GoalView | null) {
    editing = goal;
    adding = goal === null;
    confirmDelete = null;
    name = goal?.name ?? "";
    target = goal?.target ?? "";
    // A goal on a deleted bucket shows as unlinked rather than as a blank choice.
    bucketId = goal && goal.bucketName ? goal.bucketId : "";
    notes = goal?.notes ?? "";
    error = "";
  }

  function close() {
    editing = null;
    adding = false;
  }

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      close();
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
        kind: "goal",
        id: editing?.id ?? "",
        record: {
          name: name.trim(),
          targetAmount: target.trim() === "" ? null : target.trim(),
          bucketId,
          notes: notes.trim(),
        },
      }),
    );
  }

  function plural(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
  }
</script>

<section class="tiles">
  <article class="tile">
    <div class="tile-label">Goals</div>
    <div class="tile-figure">{goals.length}</div>
    <div class="tile-note">
      {goals.length === 0 ? "nothing set yet" : `${totals.withTarget} of them have a target`}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Saved towards them</div>
    <div class="tile-figure pos">{money(totals.saved)}</div>
    <div class="tile-note">
      {totals.withTarget > 0 ? `of ${wholeMoney(totals.target)} wanted` : "no targets to measure against"}
    </div>
  </article>
  <article class="tile">
    <div class="tile-label">Progress</div>
    <div class="tile-figure">{totals.percent === null ? "—" : `${totals.percent.toFixed(1)}%`}</div>
    <div class="tile-note">
      {totals.percent === null ? "give a goal a target" : `${wholeMoney(totals.remaining)} still to find`}
    </div>
  </article>
</section>

<div class="section-head">
  <h2>{plural(goals.length, "goal", "goals")}</h2>
  {#if !open}
    <button onclick={() => edit(null)} disabled={busy}>+ Goal</button>
  {/if}
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if open}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <h3>{editing ? `Edit ${editing.name}` : "Add a goal"}</h3>
    <label class="field">
      <span>Name</span>
      <input bind:value={name} required placeholder="New roof, wedding, sabbatical" />
    </label>
    <label class="field">
      <span>Target</span>
      <input bind:value={target} inputmode="decimal" placeholder="optional" />
    </label>
    <label class="field">
      <span>Saved in</span>
      <select bind:value={bucketId}>
        <option value="">Not linked yet</option>
        {#each bucketChoices as bucket (bucket.id)}
          <option value={bucket.id}>{bucket.name}</option>
        {/each}
      </select>
    </label>
    <p class="note">
      {#if chosen}
        {chosen.name} holds <strong class="pos">{money(chosen.total)}</strong> now, including
        anything invested in its name. The goal follows it.
      {:else}
        Link a bucket and the goal tracks whatever that bucket holds, including anything
        invested in its name.
      {/if}
    </p>
    <label class="field">
      <span>Notes</span><input bind:value={notes} placeholder="optional" />
    </label>
    <div class="actions">
      <button type="submit" disabled={busy || name.trim() === ""}>
        {busy ? "Saving…" : "Save"}
      </button>
      <button type="button" class="bare" onclick={close}>Cancel</button>
    </div>
  </form>
{/if}

{#if confirmDelete}
  <div class="warn-box">
    <p>
      Delete <strong>{confirmDelete.name}</strong>? The bucket behind it keeps its money;
      only the goal goes.
    </p>
    <div class="actions">
      <button
        disabled={busy}
        onclick={() =>
          run(() => ledger.apply({ op: "delete", kind: "goal", id: confirmDelete!.id }))}
      >
        Delete
      </button>
      <button class="bare" onclick={() => (confirmDelete = null)}>Keep it</button>
    </div>
  </div>
{/if}

{#if goals.length === 0 && !open}
  <div class="empty-state">
    <p><strong>No goals yet</strong></p>
    <p class="muted">
      A goal is something you are saving for, with an amount and the bucket the money is going
      into. Its progress follows that bucket.
    </p>
    <button onclick={() => edit(null)}>Add your first goal</button>
  </div>
{:else}
  <div class="cards">
    {#each goals as goal (goal.id)}
      <article class="card">
        <div class="card-name">{goal.name}</div>
        <div class="card-figure pos">{money(goal.saved)}</div>
        <div class="card-note" class:faint={!goal.bucketName}>
          {#if goal.bucketName}
            from {goal.bucketName}{#if goal.target} · {wholeMoney(goal.target)} wanted{/if}
          {:else}
            No bucket behind this goal yet
          {/if}
        </div>
        {#if goal.progress !== null}
          <span class="bar" title="{goal.progress.toFixed(1)}%">
            <span class="fill" style:width="{goal.progress}%"></span>
          </span>
          <div class="card-note" class:pos={goal.remaining === "0.00"}>
            {goal.progress.toFixed(1)}% · {goal.remaining === "0.00"
              ? "reached"
              : `${wholeMoney(goal.remaining ?? "0")} to go`}
          </div>
        {/if}
        {#if goal.notes}<div class="card-note clamp">{goal.notes}</div>{/if}
        <div class="card-actions">
          <button class="bare" onclick={() => edit(goal)} disabled={busy}>Edit</button>
          <button
            class="bare danger"
            onclick={() => { close(); confirmDelete = goal; }}
            disabled={busy}
            title="Delete">×</button
          >
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  .bar {
    display: block;
    height: 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--positive) 15%, transparent);
    overflow: hidden;
    margin: 0.35rem 0 0.15rem;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--positive);
  }
  .faint {
    color: var(--faint);
  }
  .clamp {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .empty-state {
    max-width: 26rem;
    margin: 3rem auto;
    text-align: center;
  }
  .empty-state p {
    margin: 0 0 0.5rem;
  }
</style>
