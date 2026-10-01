<script lang="ts">
  import { money, type BucketView, type ShortView } from "./ledger";

  export interface Choice {
    how: "" | "bucket" | "everyday" | "negative";
    fromBucketId: string;
    /** Save this as the bucket's default, so the next settle comes pre-filled. */
    remember: boolean;
  }

  // Where the rest comes from when a statement needs more from a bucket than
  // it holds. Starts on the bucket's own default when it has one.
  let {
    short,
    buckets,
    choice = $bindable(),
  }: { short: ShortView; buckets: BucketView[]; choice: Choice } = $props();

  // Another bucket can cover only what it holds, and not if it is locked.
  const covers = $derived(
    buckets.filter((b) => b.id !== short.bucketId && !b.locked && Number(b.cash) > 0),
  );
</script>

<fieldset class="shortfall">
  <legend>
    <strong>{short.bucketName}</strong> holds {money(short.holds)} of the {money(short.needs)} this
    takes. Where should the other <strong>{money(short.short)}</strong> come from?
  </legend>
  <label class="field check">
    <input type="radio" bind:group={choice.how} value="bucket" disabled={covers.length === 0} />
    <span>Another bucket</span>
    <select
      bind:value={choice.fromBucketId}
      onchange={() => (choice.how = "bucket")}
      disabled={covers.length === 0}
    >
      <option value="" disabled>Choose…</option>
      {#each covers as b (b.id)}
        <option value={b.id} disabled={Number(b.cash) < Number(short.short)}>
          {b.name} · {money(b.cash)}{Number(b.cash) < Number(short.short) ? " (not enough)" : ""}
        </option>
      {/each}
    </select>
  </label>
  <label class="field check">
    <input type="radio" bind:group={choice.how} value="everyday" />
    <span>Everyday spending: pay it from the spending account, like a charge on no bucket</span>
  </label>
  <label class="field check">
    <input type="radio" bind:group={choice.how} value="negative" />
    <span>
      Let {short.bucketName} go to −{money(short.short)}, and refill it later (a payday fills it
      back first)
    </span>
  </label>
  <label class="field check remember">
    <input type="checkbox" bind:checked={choice.remember} disabled={choice.how === ""} />
    <span>Always do this when {short.bucketName} runs short</span>
  </label>
</fieldset>

<style>
  .shortfall {
    border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
    border-radius: 0.4rem;
    margin: 0.5rem 0;
    padding: 0.4rem 0.75rem 0.5rem;
  }
  legend {
    padding: 0 0.3rem;
  }
  .remember {
    margin-top: 0.3rem;
    color: var(--dim);
  }
  select {
    margin-left: 0.4rem;
  }
</style>
