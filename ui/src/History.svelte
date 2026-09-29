<script lang="ts">
  import { actionTone } from "./actions";
  import { ledger, money, when, type HistoryEntry } from "./ledger";

  let entries = $state<HistoryEntry[]>([]);
  let problems = $state<string[]>([]);
  let shared = $state(true);
  let loading = $state(true);
  let error = $state("");

  $effect(() => {
    ledger
      .history()
      .then((view) => {
        entries = view.entries;
        problems = view.problems;
        shared = view.shared;
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  });
</script>

<div class="section-head">
  <h2>History</h2>
  <span class="muted count">{entries.length} recorded</span>
</div>

<p class="note">
  {#if shared}
    What changed on every machine that uses this ledger, newest first, each
    labelled with the machine that made it.
  {:else}
    What changed on this machine, newest first. This source of truth cannot hold
    shared history, so other machines' changes are not shown.
  {/if}
</p>

{#each problems as problem}<p class="error">{problem}</p>{/each}
{#if error}<p class="error">{error}</p>{/if}

{#if loading}
  <p class="empty">Reading…</p>
{:else}
  <ul class="rows">
    {#each entries as entry (entry.id)}
      <li class="row">
        <div class="row-main">
          <div class="head">
            <span class="pill {actionTone(entry.action)}">{entry.action}</span>
            <span class="row-name">{entry.name}</span>
            <span class="muted subject">{entry.subject}</span>
          </div>
          <div class="row-meta">
            {when(entry.at)}{#if entry.actor} · {entry.actor}{/if}
            {#if entry.changes.length > 0} · {entry.changes.join(" · ")}{/if}
          </div>
        </div>
        <span class="row-amount">{entry.amount ? money(entry.amount) : ""}</span>
        <span></span>
      </li>
    {:else}
      <p class="empty">Nothing has been changed yet.</p>
    {/each}
  </ul>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }
  .subject,
  .count {
    font-size: 0.7rem;
  }
</style>
