<script lang="ts">
  import WhoEditor from "./WhoEditor.svelte";
  import { money, type ChargeView } from "./ledger";

  // The charges behind a report, a page at a time, each with who spent it
  // open to change.
  let {
    charges,
    members,
    onchanged,
    empty = "No charges match these dates and status.",
  }: {
    charges: ChargeView[];
    members: string[];
    onchanged: () => void;
    empty?: string;
  } = $props();

  const PAGE = 25;
  // Kept through an edit, so changing who spent a charge on page 3 stays on
  // page 3. A new question starts over: the caller keys this list by it.
  let page = $state(0);
  const pages = $derived(Math.ceil(charges.length / PAGE));
  $effect(() => {
    if (page > 0 && page >= pages) page = Math.max(0, pages - 1);
  });

  const shown = $derived(charges.slice(page * PAGE, (page + 1) * PAGE));
</script>

{#if charges.length === 0}
  <p class="empty">{empty}</p>
{:else}
  <ul class="rows">
    {#each shown as charge (`${charge.reconciliationId}:${charge.lineId}`)}
      <li class="row">
        <div class="row-main">
          <span class="row-name">{charge.name}</span>
          <span class="row-meta">
            {charge.date || "Undated"}{charge.inferred && charge.date ? " (fallback)" : ""} ·
            {charge.member} · {charge.bucket} · {charge.card} · {charge.status}
          </span>
        </div>
        <span class="row-amount">{money(charge.amount)}</span>
        <WhoEditor
          reconciliationId={charge.reconciliationId}
          lineId={charge.lineId}
          member={charge.spentBy}
          {members}
          {onchanged}
        />
      </li>
    {/each}
  </ul>
  {#if pages > 1}
    <div class="pager">
      <button disabled={page === 0} onclick={() => (page -= 1)}>Previous</button>
      <span class="muted">{page + 1} / {pages}</span>
      <button disabled={page + 1 >= pages} onclick={() => (page += 1)}>Next</button>
    </div>
  {/if}
{/if}

<style>
  .pager {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin: 0.75rem 0;
  }
</style>
