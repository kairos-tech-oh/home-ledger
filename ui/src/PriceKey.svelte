<script lang="ts">
  import { quotes } from "./ledger";

  let stored = $state<boolean | null>(null);
  let key = $state("");
  let busy = $state(false);
  let error = $state("");
  let said = $state("");

  $effect(() => {
    quotes
      .hasKey()
      .then((has) => (stored = has))
      .catch(() => (stored = false));
  });

  async function run(action: () => Promise<boolean>, done: string) {
    busy = true;
    error = "";
    said = "";
    try {
      stored = await action();
      key = "";
      said = done;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="section-head">
  <h2>Share prices</h2>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#if said}<p class="ok">{said}</p>{/if}

<div class="panel">
  <p class="note">
    Prices come from Finnhub, using your own account. The free tier is enough
    for a household ledger. Without a key nothing is looked up and holdings keep
    whatever price they were last given.
  </p>

  {#if stored}
    <p class="note">A key is stored in this computer's keychain.</p>
    <div class="actions">
      <button onclick={() => run(quotes.forgetKey, "Key removed.")} disabled={busy}>
        Forget the key
      </button>
    </div>
  {/if}

  <label class="field">
    <span>{stored ? "Replace key" : "API key"}</span>
    <input type="password" bind:value={key} autocomplete="off" placeholder="from finnhub.io" />
  </label>
  <div class="actions">
    <button
      onclick={() => run(() => quotes.saveKey(key), "Key saved.")}
      disabled={busy || key.trim() === ""}
    >
      {busy ? "Saving…" : "Save"}
    </button>
  </div>
</div>
