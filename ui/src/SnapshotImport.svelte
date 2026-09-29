<script lang="ts">
  import { ledger, type SnapshotImport } from "./ledger";

  // Net worth history is the one thing that cannot be rebuilt from the ledger,
  // so the plugin's is brought across rather than started again.
  let path = $state("");
  let preview = $state<SnapshotImport | null>(null);
  let busy = $state(false);
  let error = $state("");
  let said = $state("");

  $effect(() => {
    ledger
      .pluginSnapshotsPath()
      .then((found) => {
        if (found && !path) path = found;
      })
      .catch(() => {});
  });

  async function look() {
    busy = true;
    error = "";
    said = "";
    preview = null;
    try {
      preview = await ledger.snapshotImportPreview(path);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function bring() {
    busy = true;
    error = "";
    try {
      const added = await ledger.snapshotImport(preview!.path);
      said =
        added === 0
          ? "Nothing new: every day in that file was already here."
          : `Added ${added} day${added === 1 ? "" : "s"} of history.`;
      preview = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="section-head">
  <h2>Net worth history</h2>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#if said}<p class="ok">{said}</p>{/if}

<div class="panel">
  <p class="note">
    The dashboard's 30-day change is built from one point a day. Bring in the plugin's
    <code>snapshots.json</code> to keep the history it already has. On the machine the plugin
    runs on it is in <code>~/.local/state/kairos.home-ledger/</code>. Importing the same file
    twice adds nothing, and for a day held in both the later point is kept.
  </p>
  <form class="import" onsubmit={(e) => { e.preventDefault(); look(); }}>
    <label class="field grow">
      <span>File</span>
      <input bind:value={path} placeholder="C:\path\to\snapshots.json" />
    </label>
    <button type="submit" disabled={busy || path.trim() === ""}>Look</button>
  </form>
  {#if preview}
    <p>
      {preview.points} day{preview.points === 1 ? "" : "s"}, {preview.first} to {preview.last}.
      {preview.new} new here{preview.overlap ? `, ${preview.overlap} already held` : ""}.
    </p>
    <div class="actions">
      <button onclick={bring} disabled={busy || preview.points === 0}>Import</button>
      <button class="bare" onclick={() => (preview = null)}>Cancel</button>
    </div>
  {/if}
</div>

<style>
  .import {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
    margin: 0;
  }
</style>
