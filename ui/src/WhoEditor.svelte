<script lang="ts">
  import MemberPicker from "./MemberPicker.svelte";
  import { ledger } from "./ledger";

  // Who made one charge, changed on its own. The one edit a settled
  // statement takes: it moves no money, so the settle and its undo stand.
  let {
    reconciliationId,
    lineId,
    member,
    members,
    onchanged,
  }: {
    reconciliationId: string;
    lineId: string;
    /** As the charge says it, "" for no one. */
    member: string;
    members: string[];
    onchanged: () => void;
  } = $props();

  let editing = $state(false);
  let value = $state("");
  let busy = $state(false);
  let error = $state("");

  function start() {
    value = member;
    error = "";
    editing = true;
  }

  async function save() {
    if (value.trim() === member) {
      editing = false;
      return;
    }
    busy = true;
    error = "";
    try {
      await ledger.apply({ op: "reconcile-member", id: reconciliationId, lineId, member: value });
      editing = false;
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if editing}
  <form class="who" onsubmit={(e) => { e.preventDefault(); save(); }}>
    <MemberPicker bind:value {members} />
    <button type="submit" disabled={busy}>Save</button>
    <button type="button" class="bare" onclick={() => (editing = false)}>Cancel</button>
    {#if error}<span class="error">{error}</span>{/if}
  </form>
{:else}
  <button class="bare" title="Change who spent this" onclick={start}>Who?</button>
{/if}

<style>
  .who {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  .error {
    margin: 0;
  }
</style>
