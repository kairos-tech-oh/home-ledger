<script lang="ts">
  import { look, setTheme, type Theme } from "./theme.svelte";
  import PriceKey from "./PriceKey.svelte";
  import BankPanel from "./BankPanel.svelte";
  import SnapshotImport from "./SnapshotImport.svelte";
  import About from "./About.svelte";
  import EncryptionPanel from "./EncryptionPanel.svelte";
  import StoreForm from "./StoreForm.svelte";
  import { describe, kinds, storage, type Setup, type StoreConfig } from "./storage";
  import type { StoreStatus } from "./lib";
  import { privacy, reshuffle, setBlur, setDemo } from "./privacy.svelte";

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

  async function demo(action: () => Promise<void>) {
    busy = "demo";
    error = "";
    note = "";
    try {
      await action();
      onchanged(setup);
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  // One section at a time, as settings pages are; General first.
  const SECTIONS = [
    ["general", "General"],
    ["appearance", "Appearance"],
    ["privacy", "Privacy"],
    ["storage", "Storage"],
    ["security", "Security"],
    ["connections", "Connections"],
    ["data", "Data"],
    ["about", "About"],
  ] as const;
  type Section = (typeof SECTIONS)[number][0];
  let section = $state<Section>("general");

  function show(next: Section) {
    section = next;
    error = "";
    note = "";
  }

  function refresh(next: Setup) {
    adding = false;
    editing = null;
    onchanged(next);
  }
</script>

{#snippet messages()}
  {#if note}<p class="ok">{note}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
{/snippet}

{#snippet storesBlock()}
  <div class="section-head">
    <h2>Where your ledger lives</h2>
  </div>
  <p class="note lead">
    The first store is the <strong>source of truth</strong>, the only one written to. The
    rest are <strong>backups</strong>: kept up to date, read from when the source of truth
    cannot be reached, and promoted by hand if you ever lose it.
  </p>

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
          <span class="role" class:primary={index === 0}>
            {index === 0 ? "Source of truth" : "Backup"}
          </span>
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

  {#if editing}
    <StoreForm existing={editing} onsaved={() => storage.setup().then(refresh)} oncancel={() => (editing = null)} />
  {:else if adding}
    <StoreForm onsaved={() => storage.setup().then(refresh)} oncancel={() => (adding = false)} />
  {:else if connectingDrive}
    <form class="panel" onsubmit={(e) => { e.preventDefault(); connectDrive(); }}>
      <h3>Connect Google Drive</h3>
      <p class="note">
        Google requires each app to identify itself, so this needs a client id from
        your own Google Cloud project: create one of type <strong>Desktop app</strong>
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
{/snippet}

{#snippet machineName()}
  <div class="setting">
    <div class="setting-text">
      <label class="setting-name" for="machine-name">Machine name</label>
      <p class="note">
        Recorded against every change you make, so a shared ledger shows which machine
        an edit came from. Never taken from the computer's own name.
      </p>
    </div>
    <input
      id="machine-name"
      class="setting-control"
      value={setup.device}
      maxlength="60"
      onchange={(e) => renameDevice(e.currentTarget)}
    />
  </div>
{/snippet}

{#if firstRun}
  <section class="first-run">
    {@render storesBlock()}
    <div class="panel settings-card">{@render machineName()}</div>
    {@render messages()}
    <div class="finish">
      <button onclick={() => run("finish", storage.finishSetup)} disabled={!!busy}>
        Done
      </button>
      <p class="note">You can change any of this later. Nothing here is set in stone.</p>
    </div>
  </section>
{:else}
  <div class="settings">
    <nav class="settings-nav" aria-label="Settings sections">
      {#each SECTIONS as [id, label] (id)}
        <button
          class="bare"
          class:on={section === id}
          aria-current={section === id ? "page" : undefined}
          onclick={() => show(id)}
        >
          {label}
        </button>
      {/each}
    </nav>

    <div class="settings-body">
      {#if section === "general"}
        <div class="section-head"><h2>General</h2></div>
        <div class="panel settings-card">
          {@render machineName()}
          <div class="setting">
            <div class="setting-text">
              <label class="setting-name" for="target-year">Retirement target year</label>
              <p class="note">The year retirement projections count towards.</p>
            </div>
            <select
              id="target-year"
              class="setting-control"
              value={setup.retirementTargetYear === null ? "" : String(setup.retirementTargetYear)}
              onchange={(e) => {
                const raw = e.currentTarget.value;
                run("target-year", () => storage.setRetirementTargetYear(raw ? Number(raw) : null));
              }}
            >
              <option value="">No projection</option>
              {#each targetYears as year (year)}<option value={String(year)}>{year}</option>{/each}
            </select>
          </div>
        </div>
      {:else if section === "appearance"}
        <div class="section-head"><h2>Appearance</h2></div>
        <div class="panel settings-card">
          <div class="setting">
            <div class="setting-text">
              <label class="setting-name" for="look">Look</label>
              <p class="note">Kept on this device only.</p>
            </div>
            <select
              id="look"
              class="setting-control"
              value={look.theme}
              onchange={(e) => setTheme(e.currentTarget.value as Theme)}
            >
              <option value="terminal">Terminal</option>
              <option value="finance-light">Finance, light</option>
              <option value="finance-dark">Finance, dark</option>
              <option value="finance-system">Finance, as the device is set</option>
              <option value="trading-dark">Trading, dark</option>
              <option value="trading-light">Trading, light</option>
              <option value="trading-system">Trading, as the device is set</option>
            </select>
          </div>
          {#if look.theme === "terminal"}
            <div class="setting">
              <div class="setting-text">
                <label class="setting-name" for="opacity">Window opacity</label>
                <p class="note">
                  How much of what is behind the window shows through, as the desktop
                  plugin does.
                </p>
              </div>
              <div class="setting-control range">
                <input
                  id="opacity"
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
              </div>
            </div>
          {/if}
        </div>
      {:else if section === "privacy"}
        <div class="section-head"><h2>Privacy</h2></div>
        <div class="panel settings-card">
          <div class="setting">
            <div class="setting-text">
              <label class="setting-name" for="demo">Demo mode</label>
              <p class="note">
                Every figure is made up from your ledger, so the screens look real and add
                up, but no amount is yours. Edits change only the made-up copy and are
                thrown away; bank connections and imports wait until it is off. Names stay
                as they are.
              </p>
              {#if privacy.demo}
                <button class="reshuffle" onclick={() => demo(reshuffle)} disabled={!!busy}>
                  New figures
                </button>
              {/if}
            </div>
            <input
              id="demo"
              type="checkbox"
              class="switch"
              checked={privacy.demo}
              disabled={!!busy}
              onchange={(e) => {
                const on = e.currentTarget.checked;
                demo(() => setDemo(on));
              }}
            />
          </div>
          <div class="setting">
            <div class="setting-text">
              <label class="setting-name" for="blur">Blur amounts</label>
              <p class="note">
                The real figures, blurred until the pointer is over them. For working where
                others can see the screen.
              </p>
            </div>
            <input
              id="blur"
              type="checkbox"
              class="switch"
              checked={privacy.blur}
              onchange={(e) => setBlur(e.currentTarget.checked)}
            />
          </div>
        </div>
      {:else if section === "storage"}
        {@render storesBlock()}
      {:else if section === "security"}
        <EncryptionPanel onchanged={() => onchanged(setup)} />
      {:else if section === "connections"}
        <BankPanel />
        <PriceKey />
      {:else if section === "data"}
        <SnapshotImport />
      {:else if section === "about"}
        <About />
      {/if}
      {@render messages()}
    </div>
  </div>
{/if}

<style>
  /* A list of sections beside the one that is open; above it on a narrow screen. */
  .settings {
    display: grid;
    grid-template-columns: 11rem minmax(0, 1fr);
    gap: 1.5rem;
    align-items: start;
    max-width: 60rem;
  }
  .settings-nav {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    position: sticky;
    top: 0.75rem;
  }
  .settings-nav button {
    text-align: left;
    padding: 0.45rem 0.7rem;
    color: var(--dim);
  }
  .settings-nav button.on {
    color: var(--text);
    background: var(--raised-strong);
  }
  .settings-body {
    min-width: 0;
  }
  .lead {
    margin-bottom: 0.75rem;
  }
  .settings-card {
    gap: 0;
    padding: 0 1rem;
  }
  .setting {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1.5rem;
    align-items: center;
    padding: 0.9rem 0;
    border-bottom: 1px solid var(--hairline);
  }
  .setting:last-child {
    border-bottom: none;
  }
  .setting-text {
    display: grid;
    gap: 0.2rem;
    min-width: 0;
  }
  .setting-name {
    font-weight: 600;
  }
  .setting-control {
    width: 15rem;
    max-width: 100%;
  }
  .range {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .range input {
    flex: 1;
    padding: 0;
    accent-color: var(--accent);
  }
  .reading {
    font-variant-numeric: tabular-nums;
    color: var(--dim);
    font-size: 0.85rem;
    min-width: 2.5rem;
    text-align: right;
  }
  .reshuffle {
    justify-self: start;
    margin-top: 0.3rem;
  }
  /* A checkbox drawn as a switch, the usual control for an on/off setting. */
  .switch {
    appearance: none;
    width: 2.4rem;
    height: 1.35rem;
    border-radius: 999px;
    border: 1px solid var(--hairline);
    background: var(--raised-strong);
    position: relative;
    cursor: pointer;
    padding: 0;
    transition: background 0.15s;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 0.12rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    background: var(--dim);
    transition: transform 0.15s;
  }
  .switch:checked {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch:checked::after {
    transform: translateX(1rem);
    background: #ffffff;
  }
  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .stores {
    list-style: none;
    padding: 0;
    margin: 0 0 1rem;
    display: grid;
    gap: 0.5rem;
  }
  .stores li {
    border: 1px solid var(--hairline);
    border-radius: 0.5rem;
    padding: 0.75rem 0.9rem;
    background: var(--raised);
  }
  .what {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 0.6rem;
  }
  .label {
    font-weight: 600;
  }
  .role {
    font-size: 0.75rem;
    color: var(--dim);
  }
  .role.primary {
    color: var(--accent);
  }
  .caveat {
    font-size: 0.75rem;
    color: var(--warn);
  }
  .where {
    font-size: 0.8rem;
    color: var(--dim);
    margin: 0.2rem 0 0.6rem;
    word-break: break-all;
  }
  .first-run {
    max-width: 48rem;
  }
  .finish {
    margin-top: 1.5rem;
    display: grid;
    gap: 0.4rem;
    justify-items: start;
  }
  @media (max-width: 40rem) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.75rem;
    }
    /* The sections become one row of chips that scrolls sideways. */
    .settings-nav {
      flex-direction: row;
      overflow-x: auto;
      position: static;
      padding-bottom: 0.25rem;
    }
    .settings-nav button {
      flex: none;
    }
    .setting {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.6rem;
    }
    .setting:has(.switch) {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .setting-control {
      width: 100%;
    }
  }
</style>
