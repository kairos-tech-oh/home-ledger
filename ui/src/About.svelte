<script lang="ts">
  import { updates, type UpdateInfo } from "./updates";

  let version = $state("");
  let found = $state<UpdateInfo | null | undefined>(undefined);
  let busy = $state(false);
  let error = $state("");

  $effect(() => {
    updates
      .version()
      .then((v) => (version = v))
      .catch(() => {});
  });

  async function check() {
    busy = true;
    error = "";
    try {
      found = await updates.check();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function install() {
    busy = true;
    error = "";
    try {
      await updates.install();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<div class="section-head">
  <h2>About</h2>
</div>

{#if error}<p class="error">{error}</p>{/if}

<div class="panel">
  <p>Home Ledger {version}</p>
  <p class="note">
    Updates come from the project's GitHub releases. Each one is signed, and this app checks
    the signature before installing anything.
  </p>
  <div class="actions">
    <button onclick={check} disabled={busy}>{busy && found === undefined ? "Checking…" : "Check for updates"}</button>
    {#if found}
      <span>{found.version} is available.</span>
      <button onclick={install} disabled={busy}>Install and restart</button>
    {:else if found === null}
      <span class="muted">This is the latest version.</span>
    {/if}
  </div>
</div>
