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
  // An account holding buckets holds only them; spending comes from elsewhere.
  const spendable = $derived(accounts.filter((a) => a.bucketCount === 0));

  const was = untrack(() => existing);
  let cardAccountId = $state(was?.cardAccountId ?? "");
  let card = $state(was?.card ?? "");
  let statementDate = $state(was?.statementDate ?? "");
  let balance = $state(was?.balance ?? "");
  let adjustAccounts = $state(was?.adjustAccounts ?? true);
  let spendSourceId = $state(was?.spendSourceId ?? "");
  let notes = $state(was?.notes ?? "");
  type Part = { id: string; bucketId: string; amount: string; rest: boolean };
  type Line = {
    id: string;
    label: string;
    member: string;
    spentOn: string;
    amount: string;
    bucketId: string;
    notes: string;
    splitGroup: string;
    parts: Part[];
  };

  function load(): Line[] {
    const out: Line[] = [];
    for (const l of was?.lines ?? []) {
      const part = { id: l.id, bucketId: l.bucketId, amount: l.amount, rest: false };
      const group = l.splitGroup ? out.find((o) => o.splitGroup === l.splitGroup) : undefined;
      if (group) {
        group.parts.push(part);
        continue;
      }
      out.push({
        id: l.id,
        label: l.label,
        member: l.member,
        spentOn: l.spentOn,
        amount: l.splitGroup ? l.splitTotal : l.amount,
        bucketId: l.bucketId,
        notes: l.notes,
        splitGroup: l.splitGroup,
        parts: l.splitGroup ? [part] : [],
      });
    }
    for (const line of out) {
      if (line.parts.length > 1) line.parts[line.parts.length - 1].rest = true;
      else if (line.parts.length === 1) {
        line.parts = [];
        line.splitGroup = "";
      }
    }
    return out;
  }

  let lines = $state(load());

  // In whole cents, so the hint never shows a float crumb. The writer decides.
  const cents = (v: string) => Math.round((Number(v) || 0) * 100);
  const left = $derived(cents(balance) - lines.reduce((t, l) => t + cents(l.amount), 0));

  function pickCard() {
    const chosen = cards.find((a) => a.id === cardAccountId);
    if (chosen && card.trim() === "") card = chosen.name;
  }

  function addLine() {
    lines.push({
      id: "",
      label: "",
      member: "",
      spentOn: "",
      amount: "",
      bucketId: "",
      notes: "",
      splitGroup: "",
      parts: [],
    });
  }

  const newId = () => crypto.randomUUID().replaceAll("-", "");

  function split(line: Line) {
    line.splitGroup = line.id || newId();
    line.parts = [
      { id: line.id, bucketId: line.bucketId, amount: "", rest: false },
      { id: "", bucketId: "", amount: "", rest: true },
    ];
  }

  function addPart(line: Line) {
    line.parts.splice(line.parts.length - 1, 0, { id: "", bucketId: "", amount: "", rest: false });
  }

  function removePart(line: Line, j: number) {
    const wasRest = line.parts[j].rest;
    line.parts.splice(j, 1);
    if (line.parts.length === 1) {
      line.bucketId = line.parts[0].bucketId;
      line.id = line.id || line.parts[0].id;
      line.parts = [];
      line.splitGroup = "";
    } else if (wasRest) {
      line.parts[line.parts.length - 1].rest = true;
    }
  }

  const rest = (line: Line) =>
    cents(line.amount) - line.parts.reduce((t, p) => t + (p.rest ? 0 : cents(p.amount)), 0);

  function flat(line: Line) {
    const shared = {
      label: line.label.trim(),
      member: line.member,
      spentOn: line.spentOn,
      notes: line.notes,
    };
    if (line.parts.length < 2) {
      return [{ ...shared, id: line.id, amount: line.amount.trim(), bucketId: line.bucketId }];
    }
    return line.parts.map((p) => ({
      ...shared,
      id: p.id,
      bucketId: p.bucketId,
      amount: p.rest ? "0" : p.amount.trim(),
      splitGroup: line.splitGroup,
      splitTotal: line.amount.trim(),
      splitRest: p.rest,
    }));
  }

  function submit() {
    onsave({
      card: card.trim(),
      cardAccountId,
      statementDate,
      balance: balance.trim() || "0",
      adjustAccounts,
      spendSourceId,
      notes: notes.trim(),
      lines: lines.flatMap(flat),
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
  {#if lines.length > 0}
    <div class="line line-labels muted" aria-hidden="true">
      <span>Charge</span>
      <span>Amount</span>
      <span>Came out of</span>
      <span>Who spent it</span>
      <span>Date</span>
      <span></span>
      <span></span>
    </div>
  {/if}
  {#each lines as line, i (i)}
    <div class="line">
      <input
        bind:value={line.label}
        placeholder="Kroger, gas, car service…"
        aria-label="Charge"
        title={line.label}
      />
      <input bind:value={line.amount} inputmode="decimal" placeholder="0.00" aria-label="Amount" />
      {#if line.parts.length > 1}
        <span class="split-note muted">Split across {line.parts.length} buckets</span>
      {:else}
        <select bind:value={line.bucketId} aria-label="Came out of">
          <option value="">Everyday spending</option>
          {#each buckets as bucket (bucket.id)}
            <option value={bucket.id}>{bucket.name} · {money(bucket.cash)}</option>
          {/each}
        </select>
      {/if}
      <MemberPicker bind:value={line.member} {members} />
      <input type="date" bind:value={line.spentOn} aria-label="Date" />
      {#if line.parts.length > 1}
        <button type="button" class="bare" title="Add a bucket" aria-label="Add a bucket"
          onclick={() => addPart(line)}>+</button>
      {:else}
        <button type="button" class="bare" title="Split across buckets"
          aria-label="Split across buckets" onclick={() => split(line)}>⑂</button>
      {/if}
      <button
        type="button"
        class="bare"
        title="Remove this charge"
        aria-label="Remove this charge"
        onclick={() => lines.splice(i, 1)}>×</button
      >
    </div>
    {#each line.parts as part, j (j)}
      <div class="line part">
        <span class="part-mark muted" aria-hidden="true">↳</span>
        {#if part.rest}
          <span class="rest muted" class:neg={rest(line) <= 0} title="Worked out when saved">
            rest · {money((rest(line) / 100).toFixed(2))}
          </span>
        {:else}
          <input bind:value={part.amount} inputmode="decimal" placeholder="0.00"
            aria-label="Amount from this bucket" />
        {/if}
        <select bind:value={part.bucketId} aria-label="Came out of">
          <option value="">Everyday spending</option>
          {#each buckets as bucket (bucket.id)}
            <option value={bucket.id}>{bucket.name} · {money(bucket.cash)}</option>
          {/each}
        </select>
        <span></span>
        <span></span>
        <span></span>
        <button type="button" class="bare" title="Remove this part" aria-label="Remove this part"
          onclick={() => removePart(line, j)}>×</button>
      </div>
    {/each}
  {/each}


  <label class="field check">
    <input type="checkbox" bind:checked={adjustAccounts} />
    <span>Take everyday spending out of an account too when it is settled</span>
  </label>
  {#if adjustAccounts}
    <label class="field">
      <span>Spending from</span>
      <select bind:value={spendSourceId}>
        <option value="">Choose an account</option>
        {#each spendable as account (account.id)}
          <option value={account.id}>{account.name}</option>
        {/each}
      </select>
    </label>
  {/if}
  <p class="note">
    Bucket lines come out of their buckets, and each bucket's account follows it.
  </p>
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
  /* Opened in a wide pop-up, so every field of a charge shows in full on one
     row: the name gets what is left after the fixed columns. */
  .line {
    display: grid;
    grid-template-columns: minmax(10rem, 1fr) 7rem minmax(10rem, 14rem) 9rem 9.5rem 1.6rem 1.6rem;
    gap: 0.4rem;
    align-items: center;
  }
  .line > :global(*) {
    min-width: 0;
  }
  .line-labels {
    font-size: 0.68rem;
  }
  .part .part-mark {
    text-align: right;
    padding-right: 0.3rem;
  }
  .split-note,
  .rest {
    font-size: 0.75rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* A narrow window: the name takes a row of its own, the rest share one. */
  @media (max-width: 60rem) {
    .line {
      grid-template-columns: 7rem minmax(9rem, 1fr) 8rem 9rem 1.6rem 1.6rem;
      padding-bottom: 0.4rem;
      border-bottom: 1px solid var(--hairline);
    }
    .line > :first-child {
      grid-column: 1 / -1;
    }
    .line.part > :first-child {
      display: none;
    }
    .line-labels {
      display: none;
    }
  }
</style>
