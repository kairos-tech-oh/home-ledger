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

  // A page at a time, of a size kept on this machine.
  const SIZES = [10, 25, 50, 100];
  const SIZE_KEY = "home-ledger.history-page-size";
  function savedSize(): number {
    try {
      const n = Number(localStorage.getItem(SIZE_KEY));
      return SIZES.includes(n) ? n : 25;
    } catch {
      return 25;
    }
  }
  let pageSize = $state(savedSize());
  let page = $state(0);
  const pages = $derived(Math.max(1, Math.ceil(shown.length / pageSize)));
  const onPage = $derived(shown.slice(page * pageSize, (page + 1) * pageSize));

  // Back to the first page whenever what is shown changes.
  $effect(() => {
    void byMachine;
    void byClient;
    void pageSize;
    page = 0;
  });

  function setSize(n: number) {
    pageSize = n;
    try {
      localStorage.setItem(SIZE_KEY, String(n));
    } catch {
      // Not kept past this session; the size still applies.
    }
  }

  // Entries opened to show everything, which the one-line row cuts off.
  let open = $state<Set<string>>(new Set());
  function toggle(id: string) {
    const next = new Set(open);
    if (!next.delete(id)) next.add(id);
    open = next;
  }

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
  {#snippet pager()}
    <div class="pager">
      <label class="size">
        Show
        <select value={pageSize} onchange={(e) => setSize(Number(e.currentTarget.value))}>
          {#each SIZES as n (n)}<option value={n}>{n}</option>{/each}
        </select>
      </label>
      <button class="bare" disabled={page === 0} onclick={() => (page -= 1)}>‹ Newer</button>
      <span class="muted">Page {page + 1} of {pages}</span>
      <button class="bare" disabled={page >= pages - 1} onclick={() => (page += 1)}>Older ›</button>
    </div>
  {/snippet}

  {#if shown.length > 0}{@render pager()}{/if}

  <ul class="rows">
    {#each onPage as entry (entry.id)}
      {@const expanded = open.has(entry.id)}
      <li>
        <div
          class="row entry"
          class:expanded
          role="button"
          tabindex="0"
          aria-expanded={expanded}
          title={expanded ? "Show less" : "Show everything"}
          onclick={() => toggle(entry.id)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              toggle(entry.id);
            }
          }}
        >
          <div class="row-main">
            <div class="head">
              <span class="pill {actionTone(entry.action)}">{entry.action}</span>
              <span class="row-name">{entry.name}</span>
              <span class="muted subject">{entry.subject}</span>
            </div>
            {#if expanded}
              <dl class="detail">
                <dt>When</dt>
                <dd>{when(entry.at)}</dd>
                {#if who(entry)}
                  <dt>By</dt>
                  <dd>{who(entry)}{#if entry.version} · version {entry.version}{/if}</dd>
                {/if}
                {#if entry.changes.length > 0}
                  <dt>Changed</dt>
                  <dd>
                    <ul class="changes">
                      {#each entry.changes as change, i (i)}<li>{change}</li>{/each}
                    </ul>
                  </dd>
                {/if}
              </dl>
            {:else}
              <div class="row-meta">
                {when(entry.at)}{#if who(entry)} · <span title={entry.version ? `version ${entry.version}` : ""}>{who(entry)}</span>{/if}
                {#if entry.changes.length > 0} · {entry.changes.join(" · ")}{/if}
              </div>
            {/if}
          </div>
          <span class="row-amount">{entry.amount ? money(entry.amount) : ""}</span>
          <span class="chevron muted" aria-hidden="true">{expanded ? "▾" : "▸"}</span>
        </div>
      </li>
    {:else}
      <p class="empty">Nothing has been changed yet.</p>
    {/each}
  </ul>

  {#if pages > 1}{@render pager()}{/if}
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
  .pager {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.75rem;
    margin: 0.4rem 0;
    font-size: 0.75rem;
  }
  .size {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-right: auto;
  }
  .entry {
    cursor: pointer;
  }
  .entry.expanded {
    align-items: start;
    border-color: var(--hairline);
  }
  .detail {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.2rem 0.75rem;
    margin: 0.4rem 0 0.1rem;
    font-size: 0.75rem;
  }
  .detail dt {
    color: var(--dim);
  }
  .detail dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .changes {
    margin: 0;
    padding-left: 1rem;
  }
  .chevron {
    font-size: 0.7rem;
  }
</style>
