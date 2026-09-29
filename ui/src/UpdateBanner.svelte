<script lang="ts">
  import { updates, type UpdateInfo } from "./updates";

  // Offered once a launch, and only in a release build: a development build is
  // always "older" than the latest release and would ask every time.
  let found = $state<UpdateInfo | null>(null);
  let dismissed = $state(false);
  let installing = $state(false);
  let error = $state("");
  let showNotes = $state(false);

  $effect(() => {
    if (!import.meta.env.PROD) return;
    updates
      .check()
      .then((u) => (found = u))
      .catch((e) => console.warn("update check failed", e));
  });

  async function install() {
    installing = true;
    error = "";
    try {
      await updates.install();
    } catch (e) {
      error = String(e);
      installing = false;
    }
  }
</script>

{#if found && !dismissed}
  <div class="update" role="status">
    <span>
      Home Ledger <strong>{found.version}</strong> is available (this is {found.current}).
      {#if found.notes}
        <button class="bare" onclick={() => (showNotes = !showNotes)}>
          {showNotes ? "Hide" : "What's new"}
        </button>
      {/if}
    </span>
    <span class="actions">
      <button onclick={install} disabled={installing}>
        {installing ? "Downloading…" : "Install and restart"}
      </button>
      <button class="bare" onclick={() => (dismissed = true)} disabled={installing}>Later</button>
    </span>
    {#if showNotes}<pre class="notes">{found.notes}</pre>{/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>
{/if}

<style>
  .update {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem 1rem;
    margin: 0.75rem 0 0;
    padding: 0.55rem 0.8rem;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border-radius: 0.4rem;
  }
  .actions {
    display: flex;
    gap: 0.4rem;
  }
  .notes {
    flex-basis: 100%;
    margin: 0;
    white-space: pre-wrap;
    font: inherit;
    font-size: 0.8rem;
    color: var(--dim);
    max-height: 12rem;
    overflow: auto;
  }
</style>
