<script lang="ts">
  import { untrack } from "svelte";
  import {
    assetClasses,
    type AccountView,
    type BucketView,
    type HoldingView,
  } from "./ledger";

  let {
    existing = null,
    accounts,
    buckets,
    types,
    onsave,
    oncancel,
    busy = false,
  }: {
    existing?: HoldingView | null;
    accounts: AccountView[];
    buckets: BucketView[];
    types: string[];
    onsave: (fields: Record<string, unknown>) => void;
    oncancel: () => void;
    busy?: boolean;
  } = $props();

  const was = untrack(() => existing);
  const firstType = untrack(() => types[0] ?? "");
  let name = $state(was?.name ?? "");
  let ticker = $state(was?.ticker ?? "");
  let kind = $state(was?.kind ?? firstType);
  let assetClass = $state(was?.assetClass ?? "");
  let quantity = $state(was?.quantity ?? "");
  let costBasis = $state(was?.costBasis ?? "");
  let price = $state(was?.price ?? "");
  let fixedPrice = $state(was?.fixedPrice ?? false);
  let accountId = $state(was?.accountId ?? "");
  let bucketId = $state(was?.bucketId ?? "");
  let purchaseDate = $state(was?.purchaseDate ?? "");
  let notes = $state(was?.notes ?? "");

  // Trades are left out, so an edit here never touches the trade history.
  function submit() {
    onsave({
      name: name.trim(),
      ticker: ticker.trim(),
      type: kind,
      assetClass,
      quantity: quantity.trim() === "" ? "0" : quantity.trim(),
      costBasis: costBasis.trim() === "" ? null : costBasis.trim(),
      price: price.trim() === "" ? null : price.trim(),
      fixedPrice,
      accountId,
      bucketId,
      purchaseDate,
      notes: notes.trim(),
    });
  }
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <h3>{existing ? `Edit ${existing.name}` : "Add a holding"}</h3>

  <label class="field"><span>Name</span><input bind:value={name} required /></label>
  <label class="field">
    <span>Ticker</span><input bind:value={ticker} placeholder="optional" />
  </label>
  <label class="field">
    <span>Type</span>
    <select bind:value={kind}>
      {#each types as option (option)}<option value={option}>{option}</option>{/each}
    </select>
  </label>
  <label class="field">
    <span>Asset class</span>
    <select bind:value={assetClass}>
      {#each assetClasses as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span>Quantity</span><input bind:value={quantity} inputmode="decimal" />
  </label>
  <label class="field">
    <span>Cost basis</span>
    <input bind:value={costBasis} inputmode="decimal" placeholder="what it cost in total" />
  </label>
  <label class="field">
    <span>Price each</span>
    <input bind:value={price} inputmode="decimal" placeholder="blank until quoted" />
  </label>
  <label class="field check">
    <input type="checkbox" bind:checked={fixedPrice} />
    <span>Priced by hand — never quoted, whatever ticker it carries</span>
  </label>
  <label class="field">
    <span>Held in</span>
    <select bind:value={accountId}>
      <option value="">No account</option>
      {#each accounts as account (account.id)}
        <option value={account.id}>{account.name}</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span>Counts towards</span>
    <select bind:value={bucketId}>
      <option value="">No bucket</option>
      {#each buckets as bucket (bucket.id)}
        <option value={bucket.id}>{bucket.name}</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span>Bought on</span><input type="date" bind:value={purchaseDate} />
  </label>
  <label class="field">
    <span>Notes</span><input bind:value={notes} placeholder="optional" />
  </label>

  {#if existing && existing.trades.length > 0}
    <p class="note">
      Average cost is worked out from cost basis over quantity. To record a buy or
      sell, use + or − on the row instead, so it lands in the trade history.
    </p>
  {/if}

  <div class="actions">
    <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save"}</button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>
</form>
