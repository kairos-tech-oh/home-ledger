<script lang="ts">
  import { ledger } from "./ledger";

  let {
    list,
    types,
    fixed,
    onchanged,
    onclose,
  }: {
    list: "budget" | "investment";
    types: string[];
    fixed: string[];
    onchanged: () => void;
    onclose: () => void;
  } = $props();

  let name = $state("");
  let busy = $state(false);
  let error = $state("");

  async function run(op: Parameters<typeof ledger.apply>[0]) {
    busy = true;
    error = "";
    try {
      await ledger.apply(op);
      name = "";
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="panel">
  <h3>{list === "budget" ? "Budget types" : "Holding types"}</h3>
  <div class="types">
    {#each types as type (type)}
      <span class="pill muted">
        {type}
        {#if !fixed.includes(type)}
          <button
            class="bare"
            disabled={busy}
            title="Delete — anything filed here moves to {fixed[0]}"
            onclick={() => run({ op: "type-delete", list, name: type })}>×</button
          >
        {/if}
      </span>
    {/each}
  </div>
  <form
    class="add"
    onsubmit={(e) => {
      e.preventDefault();
      run({ op: "type-add", list, name: name.trim() });
    }}
  >
    <input bind:value={name} placeholder="New type" required />
    <button type="submit" disabled={busy}>Add</button>
    <button type="button" class="bare" onclick={onclose}>Done</button>
  </form>
  {#if error}<p class="error">{error}</p>{/if}
  <p class="note">The built-in types cannot be removed. Deleting one of yours refiles its records under {fixed[0]}.</p>
</div>

<style>
  .types {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .types .bare {
    padding: 0 0.2rem;
  }
  .add {
    display: flex;
    gap: 0.35rem;
  }
</style>
