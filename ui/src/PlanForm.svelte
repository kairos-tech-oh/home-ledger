<script lang="ts">
  import { untrack } from "svelte";
  import { assetClasses, money, type RetirementView } from "./ledger";

  let {
    account,
    onsave,
    oncancel,
    busy = false,
  }: {
    account: RetirementView;
    onsave: (fields: Record<string, unknown>) => void;
    oncancel: () => void;
    busy?: boolean;
  } = $props();

  const was = untrack(() => account);
  let contribution = $state(was.contribution ?? "");
  let autoContribute = $state(was.autoContribute);
  let sleeves = $state(
    was.sleeves.map((s) => ({ id: s.id, name: s.name, percent: s.percent, assetClass: s.assetClass })),
  );

  // Only a hint while typing; the writer is what refuses over 100%.
  const spread = $derived(sleeves.reduce((t, s) => t + (Number(s.percent) || 0), 0));

  function submit() {
    onsave({
      monthlyContribution: contribution.trim() === "" ? null : contribution.trim(),
      autoContribute,
      sleeves: sleeves.map((s) => ({ ...s, name: s.name.trim(), percent: s.percent.trim() })),
    });
  }
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  <h3>Plan for {account.name}</h3>

  <label class="field">
    <span>Contribution a month</span>
    <input bind:value={contribution} inputmode="decimal" placeholder="leave blank for none" />
  </label>
  <label class="field check">
    <input type="checkbox" bind:checked={autoContribute} />
    <span>Top up automatically — add the contribution for each month that goes by</span>
  </label>
  {#if Number(account.fromBudget) > 0}
    <p class="note">
      {account.name} also gets {money(account.fromBudget)} a month through the budget. Leave that out
      of the figure above, or it is counted twice.
    </p>
  {/if}

  <div class="weights-head">
    <span class="muted">Target weights</span>
    <span class:neg={spread > 100} class="muted">{spread.toFixed(2)}%</span>
    <button
      type="button"
      class="bare"
      onclick={() => sleeves.push({ id: "", name: "", percent: "", assetClass: "" })}
    >
      + Fund
    </button>
  </div>
  {#each sleeves as sleeve, i (i)}
    <div class="sleeve">
      <input bind:value={sleeve.name} placeholder="Fund name" />
      <input bind:value={sleeve.percent} inputmode="decimal" placeholder="%" />
      <select bind:value={sleeve.assetClass}>
        {#each assetClasses as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
      <button type="button" class="bare" onclick={() => sleeves.splice(i, 1)}>×</button>
    </div>
  {:else}
    <p class="note">Optional. How the account is split between funds, from your statement.</p>
  {/each}
  {#if spread > 100}<p class="error">The weights come to more than 100%.</p>{/if}

  <div class="actions">
    <button type="submit" disabled={busy || spread > 100}>{busy ? "Saving…" : "Save"}</button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  .weights-head {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-size: 0.75rem;
  }
  .sleeve {
    display: grid;
    grid-template-columns: 1fr 5rem 11rem auto;
    gap: 0.4rem;
  }
</style>
