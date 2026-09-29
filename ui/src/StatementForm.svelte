<script lang="ts">
  import { untrack } from "svelte";
  import MemberPicker from "./MemberPicker.svelte";
  import {
    money,
    type AccountView,
    type BucketView,
    type ReconciliationView,
  } from "./ledger";

  let {
    existing = null,
    accounts,
    buckets,
    members = [],
    onsave,
    oncancel,
    busy = false,
  }: {
    existing?: ReconciliationView | null;
    accounts: AccountView[];
    buckets: BucketView[];
    /** Family names to suggest for who spent it. */
    members?: string[];
    onsave: (fields: Record<string, unknown>) => void;
    oncancel: () => void;
    busy?: boolean;
  } = $props();

  const cards = $derived(accounts.filter((a) => a.kind === "credit" || a.kind === "heloc"));

  const was = untrack(() => existing);
  let cardAccountId = $state(was?.cardAccountId ?? "");
  let card = $state(was?.card ?? "");
  let statementDate = $state(was?.statementDate ?? "");
  let balance = $state(was?.balance ?? "");
  let adjustAccounts = $state(was?.adjustAccounts ?? true);
  let bucketSourceId = $state(was?.bucketSourceId ?? "");
  let spendSourceId = $state(was?.spendSourceId ?? "");
  let notes = $state(was?.notes ?? "");
  let lines = $state(
    (was?.lines ?? []).map((l) => ({
      id: l.id,
      label: l.label,
      member: l.member,
      spentOn: l.spentOn,
      amount: l.amount,
      bucketId: l.bucketId,
      notes: l.notes,
    })),
  );

  // In whole cents, so the hint never shows a float crumb. The writer decides.
  const cents = (v: string) => Math.round((Number(v) || 0) * 100);
  const left = $derived(cents(balance) - lines.reduce((t, l) => t + cents(l.amount), 0));

  function pickCard() {
    const chosen = cards.find((a) => a.id === cardAccountId);
    if (chosen && card.trim() === "") card = chosen.name;
  }

  function addLine() {
    lines.push({ id: "", label: "", member: "", spentOn: "", amount: "", bucketId: "", notes: "" });
  }

  function submit() {
    onsave({
      card: card.trim(),
      cardAccountId,
      statementDate,
      balance: balance.trim() || "0",
      adjustAccounts,
      bucketSourceId,
      spendSourceId,
      notes: notes.trim(),
      lines: lines.map((l) => ({ ...l, label: l.label.trim(), amount: l.amount.trim() })),
    });
  }
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <h3>{existing ? `Edit ${existing.card}` : "Start a statement"}</h3>
  <p class="note">
    Say which bucket each charge came out of; the rest is everyday spending. Saving
    sets what the card owes. Settle it from the list once the lines add up.
  </p>

  <label class="field">
    <span>Card account</span>
    <select bind:value={cardAccountId} onchange={pickCard}>
      <option value="">Match the card name</option>
      {#each cards as account (account.id)}
        <option value={account.id}>{account.name}</option>
      {/each}
    </select>
  </label>
  <label class="field"><span>Card</span><input bind:value={card} required /></label>
  <label class="field">
    <span>Statement date</span><input type="date" bind:value={statementDate} />
  </label>
  <label class="field">
    <span>Balance to pay</span>
    <input bind:value={balance} inputmode="decimal" required />
  </label>

  <div class="lines-head">
    <span class="muted">Charges</span>
    <span class="muted" class:neg={left !== 0} class:pos={left === 0 && lines.length > 0}>
      {left === 0 ? "adds up" : `${money((left / 100).toFixed(2))} unaccounted`}
    </span>
    <button type="button" class="bare" onclick={addLine}>+ Charge</button>
  </div>
  {#each lines as line, i (i)}
    <div class="line">
      <input bind:value={line.label} placeholder="Kroger, gas, car service…" />
      <input bind:value={line.amount} inputmode="decimal" placeholder="0.00" />
      <select bind:value={line.bucketId}>
        <option value="">Everyday spending</option>
        {#each buckets as bucket (bucket.id)}
          <option value={bucket.id}>{bucket.name} · {money(bucket.cash)}</option>
        {/each}
      </select>
      <MemberPicker bind:value={line.member} {members} />
      <input type="date" bind:value={line.spentOn} />
      <button type="button" class="bare" onclick={() => lines.splice(i, 1)}>×</button>
    </div>
  {/each}


  <label class="field check">
    <input type="checkbox" bind:checked={adjustAccounts} />
    <span>Take the money out of the accounts too when it is settled</span>
  </label>
  {#if adjustAccounts}
    <label class="field">
      <span>Bucket money from</span>
      <select bind:value={bucketSourceId}>
        <option value="">Choose an account</option>
        {#each accounts as account (account.id)}
          <option value={account.id}>{account.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>Spending from</span>
      <select bind:value={spendSourceId}>
        <option value="">Choose an account</option>
        {#each accounts as account (account.id)}
          <option value={account.id}>{account.name}</option>
        {/each}
      </select>
    </label>
  {/if}
  <label class="field">
    <span>Notes</span><input bind:value={notes} placeholder="optional" />
  </label>

  <div class="actions">
    <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save"}</button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  .lines-head {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-size: 0.75rem;
  }
  .line {
    display: grid;
    grid-template-columns: 1fr 6rem 12rem 7rem 9rem auto;
    gap: 0.4rem;
  }
</style>
