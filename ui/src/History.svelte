<script lang="ts">
  import { actionTone } from "./actions";
  import { ledger, money, when, type HistoryEntry } from "./ledger";

  let entries = $state<HistoryEntry[]>([]);
  let problems = $state<string[]>([]);
  let shared = $state(true);
  let loading = $state(true);
  let error = $state("");
  let byMachine = $state("");
  let byClient = $state("");

  // An install is told apart by its id, so two machines given the same name
  // are two machines; the name is what is shown.
  const machineKey = (e: HistoryEntry) => e.install || e.actor;
  const machines = $derived(
    [...new Map(entries.map((e) => [machineKey(e), e.actor] as const)).entries()].filter(([k]) => k),
  );
  const clients = $derived([...new Set(entries.map((e) => e.client).filter(Boolean))].sort());
  const shown = $derived(
    entries.filter(
      (e) => (!byMachine || machineKey(e) === byMachine) && (!byClient || e.client === byClient),
    ),
  );

  /** Who made it: the machine, then which program, then a script's own label. */
  function who(e: HistoryEntry): string {
    return [e.actor, e.client, e.via].filter(Boolean).join(" · ");
  }

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
  <span class="muted count">{shown.length} of {entries.length} recorded</span>
</div>

{#if machines.length > 1 || clients.length > 1}
  <div class="filters">
    <label class="field">
      <span>Machine</span>
      <select bind:value={byMachine}>
        <option value="">Every machine</option>
        {#each machines as [key, name] (key)}<option value={key}>{name}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Made with</span>
      <select bind:value={byClient}>
        <option value="">Anything</option>
        {#each clients as c (c)}<option value={c}>{c}</option>{/each}
      </select>
    </label>
  </div>
{/if}

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
    {#each shown as entry (entry.id)}
      <li class="row">
        <div class="row-main">
          <div class="head">
            <span class="pill {actionTone(entry.action)}">{entry.action}</span>
            <span class="row-name">{entry.name}</span>
            <span class="muted subject">{entry.subject}</span>
          </div>
          <div class="row-meta">
            {when(entry.at)}{#if who(entry)} · <span title={entry.version ? `version ${entry.version}` : ""}>{who(entry)}</span>{/if}
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
  .filters {
    display: flex;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
  }
  .filters .field {
    margin: 0;
  }
  .subject,
  .count {
    font-size: 0.7rem;
  }
</style>
