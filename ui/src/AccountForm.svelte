<script lang="ts">
  import { untrack } from "svelte";
  import { accountKinds, money, type AccountView } from "./ledger";

  let {
    existing = null,
    accounts = [],
    onsave,
    oncancel,
    busy = false,
  }: {
    existing?: AccountView | null;
    /** Every account, for choosing the loan against a property or vehicle. */
    accounts?: AccountView[];
    onsave: (fields: Record<string, unknown>) => void;
    oncancel: () => void;
    busy?: boolean;
  } = $props();

  let name = $state(untrack(() => existing?.name ?? ""));
  let kind = $state(untrack(() => existing?.kind ?? "checking"));
  let institution = $state(untrack(() => existing?.institution ?? ""));
  let total = $state(untrack(() => existing?.total ?? ""));
  let availableCredit = $state(untrack(() => existing?.availableCredit ?? ""));
  let notes = $state(untrack(() => existing?.notes ?? ""));
  let loanAccountId = $state(untrack(() => existing?.loanAccountId ?? ""));

  // A home or a car: its total is what it is worth, and a loan can be secured
  // against it so the two read together as equity.
  const secured = $derived(kind === "property" || kind === "vehicle");
  const loans = $derived(
    accounts.filter(
      (a) =>
        (a.kind === "loan" || a.kind === "heloc") &&
        (!a.secures || a.id === existing?.loanAccountId),
    ),
  );

  const takesCredit = $derived(kind === "credit" || kind === "heloc");
  // Loans are owed too, but do not carry a credit limit.
  const owed = $derived(takesCredit || kind === "loan");

  function submit() {
    // Only what this form owns is sent. Anything it does not know about —
    // holdings, a retirement schedule — keeps whatever is stored, because a
    // set merges rather than replaces.
    const fields: Record<string, unknown> = {
      name: name.trim(),
      type: kind,
      institution: institution.trim(),
      notes: notes.trim(),
      total: total.trim() === "" ? null : total.trim(),
    };
    if (secured) fields.loanAccountId = loanAccountId;
    if (takesCredit) {
      fields.availableCredit = availableCredit.trim() === "" ? null : availableCredit.trim();
    }
    onsave(fields);
  }
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <h3>{existing ? `Edit ${existing.name}` : "Add an account"}</h3>

  <label class="field">
    <span>Name</span>
    <input bind:value={name} required />
  </label>
  <label class="field">
    <span>Kind</span>
    <select bind:value={kind}>
      {#each accountKinds as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span>Institution</span>
    <input bind:value={institution} placeholder="optional" />
  </label>
  <label class="field">
    <span>{owed ? "Balance owed" : secured ? "What it is worth" : "Cash balance"}</span>
    <input bind:value={total} inputmode="decimal" placeholder="leave blank for none" />
  </label>
  {#if secured}
    <label class="field">
      <span>Loan against it</span>
      <select bind:value={loanAccountId}>
        <option value="">None</option>
        {#each loans as loan (loan.id)}
          <option value={loan.id}>{loan.name} · {money(loan.total)} owed</option>
        {/each}
      </select>
    </label>
    <p class="note">
      Its value counts as an asset and the loan as a debt, so net worth includes the equity.
      Update the value now and then; nothing looks it up.
    </p>
  {/if}
  {#if takesCredit}
    <label class="field">
      <span>Available credit</span>
      <input bind:value={availableCredit} inputmode="decimal" placeholder="optional" />
    </label>
  {/if}
  <label class="field">
    <span>Notes</span>
    <input bind:value={notes} placeholder="optional" />
  </label>

  {#if existing && existing.holdingCount > 0}
    <p class="note">
      {existing.holdingCount} holding{existing.holdingCount === 1 ? "" : "s"} are assigned
      to this account and are priced separately. Editing here leaves them alone.
    </p>
  {/if}

  <div class="actions">
    <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save"}</button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>
</form>
