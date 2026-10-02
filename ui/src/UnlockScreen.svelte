<script lang="ts">
  import { encryption } from "./encryption";

  // Shown instead of the app when the ledger is encrypted and this machine
  // does not hold the key yet: a new machine, or one without a keychain.
  let { onunlocked }: { onunlocked: () => void } = $props();

  let secret = $state("");
  let busy = $state(false);
  let error = $state("");
  let notKept = $state(false);

  async function unlock() {
    busy = true;
    error = "";
    try {
      const kept = await encryption.unlock(secret);
      secret = "";
      if (kept) onunlocked();
      else notKept = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="unlock">
  <h2>Your ledger is encrypted</h2>
  {#if notKept}
    <p>
      Unlocked. This computer has no keychain to keep the key in, so it will ask again the next
      time Home Ledger opens.
    </p>
    <button onclick={onunlocked}>Continue</button>
  {:else}
    <p class="muted">
      Enter the passphrase, or the recovery code, to open it on this computer. It is asked for
      once; after that this computer keeps the key in its keychain.
    </p>
    <form onsubmit={(e) => { e.preventDefault(); unlock(); }}>
      <label class="field">
        <span>Passphrase or recovery code</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input type="password" bind:value={secret} autocomplete="current-password" autofocus />
      </label>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="actions">
        <button type="submit" disabled={busy || secret.trim() === ""}>
          {busy ? "Unlocking…" : "Unlock"}
        </button>
      </div>
    </form>
  {/if}
</section>

<style>
  .unlock {
    max-width: 26rem;
    margin: 4rem auto;
  }
  h2 {
    font-size: 1.1rem;
    margin: 0 0 0.5rem;
  }
</style>
