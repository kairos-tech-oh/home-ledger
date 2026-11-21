<script lang="ts">
  // Making or changing a colour theme. It is worn as it is edited, so the
  // whole app behind the pop-up is the preview, and a sample inside it shows
  // the pieces a theme touches up close. Closing without saving puts the
  // chosen colours back.
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { ledger as ledgerApi, type ColorTheme, type Palette } from "./ledger";
  import {
    TOKENS,
    look,
    opaque,
    parseHex,
    previewMode,
    previewTheme,
    setCustomThemes,
    styleColors,
    toHex,
  } from "./theme.svelte";

  let {
    theme,
    onclose,
    onsaved,
  }: {
    /** The theme to change, or a new one (empty id) with its starting colours. */
    theme: ColorTheme;
    onclose: () => void;
    /** With the saved theme's id. */
    onsaved: (id: string) => void;
  } = $props();

  // What the style shows for a token the theme leaves out, in each mode.
  const own = { light: styleColors(false), dark: styleColors(true) };

  let name = $state(untrack(() => theme.name));
  let light = $state<Palette>(untrack(() => ({ ...theme.light })));
  let dark = $state<Palette>(untrack(() => ({ ...theme.dark })));
  let editingDark = $state(untrack(() => look.dark));
  let busy = $state(false);
  let error = $state("");

  const palette = $derived(editingDark ? dark : light);
  const base = $derived(editingDark ? own.dark : own.light);
  const groups = [...new Set(TOKENS.map((t) => t.group))];

  // Worn as it changes. Untracked inside, since wearing it reads the look.
  $effect(() => {
    const draft: ColorTheme = { id: theme.id, name, light: { ...light }, dark: { ...dark } };
    const showDark = editingDark;
    untrack(() => {
      previewTheme(draft);
      previewMode(showDark);
    });
  });

  function close() {
    previewTheme(null);
    onclose();
  }

  function set(key: string, value: string) {
    const v = value.trim().toLowerCase();
    if (!parseHex(v)) return;
    if (editingDark) dark[key] = v;
    else light[key] = v;
  }

  /** From the picker, which knows no alpha: keep whatever alpha the token had. */
  function pick(key: string, hex: string) {
    const was = parseHex(palette[key] ?? base[key] ?? "");
    const c = parseHex(hex);
    if (!c) return;
    set(key, toHex(c[0], c[1], c[2], was ? was[3] : 1));
  }

  function reset(key: string) {
    if (editingDark) delete dark[key];
    else delete light[key];
  }

  /** Copy every colour of one mode into the other, as a start on it. */
  function copyAcross() {
    if (editingDark) light = { ...dark };
    else dark = { ...light };
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim()) {
      error = "Give the theme a name.";
      return;
    }
    busy = true;
    error = "";
    try {
      const before = new Set(look.custom.map((t) => t.id));
      await ledgerApi.apply({
        op: "theme-set",
        theme: { id: theme.id, name: name.trim(), light: { ...light }, dark: { ...dark } },
      });
      const view = await ledgerApi.read();
      previewTheme(null);
      setCustomThemes(view.themes);
      const id = theme.id || view.themes.find((t) => !before.has(t.id))?.id || "";
      onsaved(id);
    } catch (err) {
      const text = String(err);
      // Saving what is already saved is not a failure.
      if (text.includes("already saved")) {
        previewTheme(null);
        onsaved(theme.id);
      } else {
        error = text;
      }
    } finally {
      busy = false;
    }
  }
</script>

<Modal onclose={close} wide label="Close without saving">
  <form class="editor" onsubmit={save}>
    <h3>{theme.id ? "Edit theme" : "New theme"}</h3>

    <div class="top">
      <label class="field">
        <span>Name</span>
        <input bind:value={name} maxlength="40" placeholder="Seaside" required />
      </label>
      <div class="toggle" role="group" aria-label="Which colours to edit">
        <button type="button" class:on={!editingDark} onclick={() => (editingDark = false)}>Light</button>
        <button type="button" class:on={editingDark} onclick={() => (editingDark = true)}>Dark</button>
      </div>
    </div>

    <div class="body">
      <div class="tokens">
        {#each groups as group (group)}
          <fieldset>
            <legend>{group}</legend>
            {#each TOKENS.filter((t) => t.group === group) as t (t.key)}
              {@const value = palette[t.key] ?? base[t.key] ?? "#000000"}
              {@const mine = palette[t.key] !== undefined}
              <div class="token">
                <input
                  type="color"
                  value={opaque(value)}
                  aria-label={t.label}
                  oninput={(e) => pick(t.key, e.currentTarget.value)}
                />
                <span class="token-name" class:muted={!mine} title={mine ? "" : "The style's own colour"}>
                  {t.label}
                </span>
                <input
                  class="hex"
                  value={value}
                  spellcheck="false"
                  aria-label="{t.label}, as hex"
                  onchange={(e) => {
                    if (!parseHex(e.currentTarget.value.trim())) e.currentTarget.value = value;
                    else set(t.key, e.currentTarget.value);
                  }}
                />
                <button
                  type="button"
                  class="bare"
                  disabled={!mine}
                  title="Use the style's own colour"
                  onclick={() => reset(t.key)}>↺</button
                >
              </div>
            {/each}
          </fieldset>
        {/each}
      </div>

      <!-- The pieces a theme colours, up close. Real classes, so every style draws them its own way. -->
      <aside class="sample" aria-label="Sample">
        <div class="tiles">
          <div class="tile">
            <div class="tile-label">Net worth</div>
            <div class="tile-figure pos">$48,210</div>
            <div class="tile-note">up $1,204 this month</div>
          </div>
        </div>
        <div class="card">
          <div class="card-top">
            <span class="card-name">Everyday checking</span>
            <span class="pill blue">checking</span>
          </div>
          <div class="card-figure">$3,482.10</div>
          <div class="card-note">Synced today</div>
        </div>
        <ul class="rows">
          <li class="row">
            <div class="row-main">
              <div class="row-name">Groceries</div>
              <div class="row-meta">Budget · monthly</div>
            </div>
            <span class="pill green">food</span>
            <span class="row-amount neg">−$612.00</span>
          </li>
          <li class="row">
            <div class="row-main">
              <div class="row-name">Paycheck</div>
              <div class="row-meta">Income · every two weeks</div>
            </div>
            <span class="pill violet">salary</span>
            <span class="row-amount pos">$2,150.00</span>
          </li>
        </ul>
        <div class="pills">
          <span class="pill blue">blue</span>
          <span class="pill violet">violet</span>
          <span class="pill green">green</span>
          <span class="pill amber">amber</span>
          <span class="pill red">red</span>
        </div>
        <p class="warn-box">A warning reads like this.</p>
        <div class="actions">
          <input placeholder="A field" />
          <button type="button">A button</button>
          <button type="button" class="on">Chosen</button>
        </div>
      </aside>
    </div>

    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions foot">
      <button type="button" class="bare" onclick={copyAcross}>
        Copy {editingDark ? "dark" : "light"} colours to {editingDark ? "light" : "dark"}
      </button>
      <span class="spacer"></span>
      <button type="button" onclick={close} disabled={busy}>Cancel</button>
      <button type="submit" disabled={busy}>{busy ? "Saving…" : "Save theme"}</button>
    </div>
  </form>
</Modal>

<style>
  .editor {
    display: grid;
    gap: 0.75rem;
  }
  h3 {
    margin: 0;
    padding-right: 1.6rem;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .top .field {
    grid-template-columns: auto 16rem;
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 20rem);
    gap: 1rem;
    align-items: start;
  }
  .tokens {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: 0.6rem;
    max-height: min(60vh, 36rem);
    overflow: auto;
    padding-right: 0.25rem;
  }
  fieldset {
    border: 1px solid var(--hairline);
    border-radius: 0.4rem;
    margin: 0;
    padding: 0.4rem 0.6rem 0.5rem;
    display: grid;
    gap: 0.25rem;
    align-content: start;
  }
  legend {
    font-size: 0.72rem;
    color: var(--dim);
    padding: 0 0.3rem;
  }
  .token {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 6.2rem auto;
    align-items: center;
    gap: 0.4rem;
  }
  .token-name {
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  input[type="color"] {
    width: 1.7rem;
    height: 1.4rem;
    padding: 0;
    border: 1px solid var(--hairline);
    border-radius: 0.25rem;
    background: none;
    cursor: pointer;
  }
  .hex {
    font-family: ui-monospace, "Cascadia Mono", Menlo, Consolas, monospace;
    font-size: 0.75rem;
    padding: 0.15rem 0.3rem;
    width: 100%;
    box-sizing: border-box;
  }
  .sample {
    display: grid;
    gap: 0.6rem;
    padding: 0.75rem;
    border-radius: 0.5rem;
    background: rgb(var(--ground));
    border: 1px solid var(--hairline);
  }
  .sample .tiles {
    margin: 0;
    grid-template-columns: 1fr;
  }
  .sample .row {
    grid-template-columns: minmax(0, 1fr) auto auto;
  }
  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .sample .warn-box {
    margin: 0;
  }
  .sample .actions input {
    width: 6rem;
  }
  .foot {
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
  @media (max-width: 760px) {
    .body {
      grid-template-columns: 1fr;
    }
    .tokens {
      max-height: none;
    }
  }
</style>
