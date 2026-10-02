<script lang="ts">
  import PriceKey from "./PriceKey.svelte";
  import SnapshotImport from "./SnapshotImport.svelte";
  import About from "./About.svelte";
  import EncryptionPanel from "./EncryptionPanel.svelte";
  import StoreForm from "./StoreForm.svelte";
  import { describe, kinds, storage, type Setup, type StoreConfig } from "./storage";
  import type { StoreStatus } from "./lib";

  let {
    setup,
    stores = [],
    onchanged,
    firstRun = false,
  }: {
    setup: Setup;
    /// Live health, which the configuration alone cannot tell you.
    stores?: StoreStatus[];
    onchanged: (next: Setup) => void;
    firstRun?: boolean;
  } = $props();

  function healthOf(id: string): StoreStatus["health"] | null {
    return stores.find((s) => s.id === id)?.health ?? null;
  }

  const thisYear = new Date().getFullYear();
  const targetYears = Array.from({ length: 71 }, (_, i) => thisYear + i);

  let adding = $state(false);
  let connectingDrive = $state(false);
  let driveClientId = $state("");
  let driveFileName = $state("home-ledger.json");
  let driveLabel = $state("Google Drive");
  let editing = $state<StoreConfig | null>(null);
  let busy = $state("");
  let error = $state("");
  let note = $state("");

  /// A refusal that can be overridden asks once, then again with force.
  let pendingPromotion = $state<{ id: string; why: string } | null>(null);

  async function run(what: string, action: () => Promise<Setup>) {
    busy = what;
    error = "";
    note = "";
    try {
      onchanged(await action());
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  function renameDevice(input: HTMLInputElement) {
    const name = input.value.trim();
    if (!name) {
      input.value = setup.device;
      error = "This machine needs a name.";
      note = "";
      return;
    }
    run("device", () => storage.renameDevice(name));
  }

  async function promote(id: string, force: boolean) {
    busy = "promote";
    error = "";
    note = "";
    try {
      const report = await storage.promote(id, force);
      onchanged(report.setup);
      pendingPromotion = null;
      note = report.fastForwarded
        ? "Brought up to date first, so nothing was lost."
        : `Promoted; the copies were ${report.was}.`;
    } catch (e) {
      const why = String(e);
      // The backend says "promote again" for the cases a person may override.
      if (!force && why.includes("promote again")) {
        pendingPromotion = { id, why };
      } else {
        error = why;
      }
    } finally {
      busy = "";
    }
  }

  async function connectDrive() {
    busy = "drive";
    error = "";
    note = "";
    try {
      const result = await storage.connectDrive(driveClientId, driveFileName, driveLabel);
      onchanged(result.setup);
      connectingDrive = false;
      note = `Connected ${result.label}. It joined as a backup.`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  function refresh(next: Setup) {
    adding = false;
    editing = null;
    onchanged(next);
  }
</script>

<section>
  <header>
    <div>
      <h2>Where your ledger lives</h2>
      <p class="note">
        The first store is the <strong>source of truth</strong> — the only one written
        to. The rest are <strong>backups</strong>: kept up to date, read from when the
        source of truth cannot be reached, and promoted by hand if you ever lose it.
      </p>
    </div>
  </header>

  {#if !setup.keychainAvailable}
    <p class="warn-box">
      This system has no keychain available, so credentials cannot be kept. Stores
      needing a key will work until the app closes, and then need entering again.
    </p>
  {/if}

  {#each setup.problems as problem (problem)}
    <p class="warn-box">{problem}</p>
  {/each}

  <ol class="stores">
    {#each setup.stores as store, index (store.id)}
      <li>
        <div class="what">
          <span class="label">{store.label}</span>
          <span class="role">{index === 0 ? "source of truth" : "backup"}</span>
          {#if healthOf(store.label)}
            {@const health = healthOf(store.label)!}
            <span
              class="pill"
              class:green={health.state === "reachable"}
              class:red={health.state !== "reachable"}
              title={health.state === "reachable" ? "Reachable" : health.detail}
            >
              {health.state}
            </span>
          {/if}
          {#if !kinds[store.settings.kind].safeAsSourceOfTruth}
            <span class="caveat" title="Network shares cannot lock reliably">
              no safe locking
            </span>
          {/if}
        </div>
        <div class="where">{describe(store.settings)}</div>
        <div class="actions">
          {#if index !== 0}
            <button onclick={() => promote(store.id, false)} disabled={!!busy}>
              Make source of truth
            </button>
          {/if}
          <button onclick={() => (editing = store)} disabled={!!busy}>Edit</button>
          {#if setup.stores.length > 1 && index !== 0}
            <button
              class="bare"
              disabled={!!busy}
              onclick={() => run("remove", () => storage.remove(store.id))}
            >
              Remove
            </button>
          {/if}
        </div>
      </li>
    {/each}
  </ol>

  {#if pendingPromotion}
    <div class="warn-box">
      <p>{pendingPromotion.why}</p>
      <div class="actions">
        <button onclick={() => promote(pendingPromotion!.id, true)}>
          Promote anyway
        </button>
        <button class="bare" onclick={() => (pendingPromotion = null)}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if note}<p class="ok">{note}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}

  {#if editing}
    <StoreForm existing={editing} onsaved={() => storage.setup().then(refresh)} oncancel={() => (editing = null)} />
  {:else if adding}
    <StoreForm onsaved={() => storage.setup().then(refresh)} oncancel={() => (adding = false)} />
  {:else if connectingDrive}
    <form class="panel" onsubmit={(e) => { e.preventDefault(); connectDrive(); }}>
      <h3>Connect Google Drive</h3>
      <p class="note">
        Google requires each app to identify itself, so this needs a client id from
        your own Google Cloud project — create one of type <strong>Desktop app</strong>
        and paste it below. The app asks only for files it creates in your Drive, so
        it can never see anything else there.
      </p>
      <label class="field">
        <span>Client id</span>
        <input bind:value={driveClientId} placeholder="….apps.googleusercontent.com" required />
      </label>
      <label class="field">
        <span>File name</span>
        <input bind:value={driveFileName} required />
      </label>
      <label class="field">
        <span>Name</span>
        <input bind:value={driveLabel} />
      </label>
      <div class="actions">
        <button type="submit" disabled={!!busy}>
          {busy === "drive" ? "Waiting for sign-in…" : "Sign in with Google"}
        </button>
        <button type="button" class="bare" onclick={() => (connectingDrive = false)}>
          Cancel
        </button>
      </div>
      {#if busy === "drive"}
        <p class="note">A browser tab has opened. Finish signing in there.</p>
      {/if}
    </form>
  {:else}
    <div class="actions">
      <button onclick={() => (adding = true)} disabled={!!busy}>Add a store</button>
      <button onclick={() => (connectingDrive = true)} disabled={!!busy}>
        Connect Google Drive
      </button>
    </div>
  {/if}

  <div class="appearance">
    <label class="field">
      <span>Window opacity</span>
      <input
        type="range"
        min="0.3"
        max="1"
        step="0.01"
        value={setup.opacity}
        oninput={(e) => {
          // Previewed as the slider moves; only the release is stored.
          document.documentElement.style.setProperty(
            "--surface-alpha",
            e.currentTarget.value,
          );
        }}
        onchange={(e) =>
          run("opacity", () => storage.setOpacity(Number(e.currentTarget.value)))}
      />
      <span class="reading">{Math.round(setup.opacity * 100)}%</span>
    </label>
    <p class="note">
      How much of what is behind the window shows through. The same ground and
      palette as the desktop plugin.
    </p>
  </div>

  <div class="device">
    <label class="field">
      <span>Machine name</span>
      <input
        value={setup.device}
        maxlength="60"
        onchange={(e) => renameDevice(e.currentTarget)}
      />
    </label>
    <p class="note">
      Recorded against every change you make, so a shared ledger shows which machine
      an edit came from. Never taken from the computer's own name.
    </p>
  </div>

  <div class="device">
    <h3 class="group">Retirement</h3>
    <label class="field">
      <span>Target year</span>
      <select
        value={setup.retirementTargetYear === null ? "" : String(setup.retirementTargetYear)}
        onchange={(e) => {
          const raw = e.currentTarget.value;
          run("target-year", () => storage.setRetirementTargetYear(raw ? Number(raw) : null));
        }}
      >
        <option value="">No projection</option>
        {#each targetYears as year (year)}<option value={String(year)}>{year}</option>{/each}
      </select>
    </label>
  </div>

  {#if !firstRun}
    <EncryptionPanel onchanged={() => onchanged(setup)} />
    <PriceKey />
    <SnapshotImport />
    <About />
  {/if}

  {#if firstRun}
    <div class="finish">
      <button onclick={() => run("finish", storage.finishSetup)} disabled={!!busy}>
        Done
      </button>
      <p class="note">
        You can change any of this later. Nothing here is set in stone.
      </p>
    </div>
  {/if}
</section>

<style>
  h2 {
    font-size: 1rem;
    margin: 0 0 0.25rem;
  }
  .stores {
    list-style: none;
    padding: 0;
    margin: 1rem 0;
    display: grid;
    gap: 0.5rem;
  }
  .stores li {
    border: 1px solid var(--hairline);
    border-radius: 0.5rem;
    padding: 0.6rem 0.8rem;
  }
  .what {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
  }
  .label {
    font-weight: 600;
  }
  .role {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.6;
  }
  .caveat {
    font-size: 0.75rem;
    color: var(--warn);
  }
  .where {
    font-size: 0.8rem;
    opacity: 0.7;
    margin: 0.15rem 0 0.5rem;
    word-break: break-all;
  }
  .ok {
    color: var(--positive);
    font-size: 0.9rem;
  }
  .error {
    color: var(--negative);
    font-size: 0.9rem;
  }
  .appearance {
    margin-top: 1.5rem;
    display: grid;
    gap: 0.3rem;
  }
  .appearance label {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-size: 0.9rem;
  }
  .appearance input[type="range"] {
    flex: 1;
    max-width: 18rem;
    padding: 0;
    accent-color: var(--accent);
  }
  .reading {
    font-variant-numeric: tabular-nums;
    opacity: 0.7;
    font-size: 0.85rem;
  }
  .device {
    margin-top: 1.5rem;
    display: grid;
    gap: 0.3rem;
  }
  .group {
    font-size: 0.85rem;
    font-weight: 600;
    margin: 0;
  }
  select {
    font: inherit;
    padding: 0.35rem 0.5rem;
    border-radius: 0.3rem;
    border: 1px solid var(--faint);
    background: transparent;
    color: inherit;
  }
  .device label {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-size: 0.9rem;
  }
  input {
    font: inherit;
    padding: 0.35rem 0.5rem;
    border-radius: 0.3rem;
    border: 1px solid var(--faint);
    background: transparent;
    color: inherit;
  }
  .finish {
    margin-top: 1.5rem;
    display: grid;
    gap: 0.4rem;
    justify-items: start;
  }
</style>
