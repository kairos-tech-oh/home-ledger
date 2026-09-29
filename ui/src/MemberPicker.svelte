<script lang="ts">
  // Who spent it: a dropdown of the family's names, with room for someone who
  // is not on the list. A browser suggestion list was used before, and it hid
  // every name that did not match what the box already said.
  let {
    value = $bindable(""),
    members,
    label = "Spent by",
  }: { value?: string; members: string[]; label?: string } = $props();

  const OTHER = "\u0000other";

  // A name not on the list (typed before, or imported) is still offered, so
  // opening a charge never changes who it is attributed to.
  const names = $derived(
    value && !members.some((m) => m.toLowerCase() === value.toLowerCase())
      ? [...members, value]
      : members,
  );

  let typing = $state(false);

  function choose(next: string) {
    if (next === OTHER) {
      typing = true;
      value = "";
    } else {
      typing = false;
      value = next;
    }
  }
</script>

{#if typing}
  <span class="typed">
    <input
      bind:value
      placeholder="Name"
      aria-label={label}
      maxlength="120"
    />
    <button type="button" class="bare" title="Back to the list" onclick={() => (typing = false)}>↩</button>
  </span>
{:else}
  <select
    aria-label={label}
    value={names.find((n) => n.toLowerCase() === value.toLowerCase()) ?? ""}
    onchange={(e) => choose(e.currentTarget.value)}
  >
    <option value="">No one chosen</option>
    {#each names as name (name)}<option value={name}>{name}</option>{/each}
    <option value={OTHER}>Someone else…</option>
  </select>
{/if}

<style>
  .typed {
    display: flex;
    gap: 0.2rem;
    min-width: 0;
  }
  .typed input {
    flex: 1;
    min-width: 0;
  }
</style>
