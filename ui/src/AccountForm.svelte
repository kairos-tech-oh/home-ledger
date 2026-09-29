<script lang="ts">
  import { untrack } from "svelte";
  import { accountKinds, type AccountView } from "./ledger";

  let {
    existing = null,
    onsave,
    oncancel,
    busy = false,
  }: {
    existing?: AccountView | null;
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
    <span>{owed ? "Balance owed" : "Cash balance"}</span>
    <input bind:value={total} inputmode="decimal" placeholder="leave blank for none" />
  </label>
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
