<script lang="ts">
  import { storage, type Setup } from "./storage";

  // Asks once a computer is set up but still has its default name, which
  // would make it indistinguishable from any other "Windows PC" in history.
  let { setup, onrenamed }: { setup: Setup; onrenamed: (next: Setup) => void } = $props();

  let name = $state("");
  let busy = $state(false);
  let error = $state("");
  let later = $state(false);

  async function save() {
    busy = true;
    error = "";
    try {
      onrenamed(await storage.renameDevice(name.trim()));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if !later}
  <form class="name-it" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <span>
      Every change is labelled with the computer that made it, and this one is still called
      <strong>{setup.device}</strong>. Give it a name you will recognise:
    </span>
    <input bind:value={name} maxlength="60" placeholder="Office PC, Laptop…" />
    <button type="submit" disabled={busy || name.trim() === ""}>Save</button>
    <button type="button" class="bare" onclick={() => (later = true)}>Later</button>
    {#if error}<p class="error">{error}</p>{/if}
  </form>
{/if}

<style>
  .name-it {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin: 0.75rem 0 0;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--hairline);
    background: var(--raised);
    border-radius: 0.4rem;
  }
  .name-it span {
    flex-basis: 100%;
  }
</style>
