<script lang="ts">
  import DashWidget from "./DashWidget.svelte";
  import WidgetEditor from "./WidgetEditor.svelte";
  import { ledger, type DashboardView, type Widget, type WidgetView } from "./ledger";

  // The landing page. Widgets sit in rows: a full-width widget has a row to
  // itself and half-width widgets pair up, each pair sharing one height. On a
  // narrow window every widget takes a row.
  let {
    revision,
    onchanged,
    onopen,
  }: { revision: number; onchanged: () => void; onopen: (page: string) => void } = $props();

  let dash = $state<DashboardView | null>(null);
  let error = $state("");
  let editing = $state(false);
  let editor = $state<{ widget: Widget | null } | null>(null);
  let busy = $state(false);
  let width = $state(1000);

  $effect(() => {
    void revision;
    ledger
      .dashboard()
      .then((d) => {
        dash = d;
        error = "";
      })
      .catch((e) => (error = String(e)));
  });

  const columns = $derived(width >= 760 ? 2 : 1);

  /** Rows of widget indexes, as the plugin lays them out. */
  const rows = $derived.by(() => {
    const out: number[][] = [];
    let pending: number[] = [];
    (dash?.widgets ?? []).forEach((w, i) => {
      if (columns === 1 || w.span === 2) {
        if (pending.length) out.push(pending);
        pending = [];
        out.push([i]);
      } else {
        pending.push(i);
        if (pending.length === 2) {
          out.push(pending);
          pending = [];
        }
      }
    });
    if (pending.length) out.push(pending);
    return out;
  });

  /** The stored shape of a widget, without what the screen was given to draw it. */
  function stored(w: WidgetView | Widget): Widget {
    const out: Widget = { id: w.id, kind: w.kind, title: w.title, span: w.span, refs: [...w.refs] };
    if (w.options) out.options = $state.snapshot(w.options);
    return out;
  }

  async function save(widgets: Widget[]) {
    busy = true;
    error = "";
    try {
      await ledger.apply({ op: "dashboard-set", dashboard: { v: 1, widgets } });
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const current = () => (dash?.widgets ?? []).map(stored);

  function move(index: number, delta: -1 | 1) {
    const list = current();
    const to = index + delta;
    if (to < 0 || to >= list.length) return;
    const [moved] = list.splice(index, 1);
    list.splice(to, 0, moved);
    save(list);
  }

  function toggleSpan(index: number) {
    const list = current();
    list[index].span = list[index].span === 2 ? 1 : 2;
    save(list);
  }

  function remove(index: number) {
    save(current().filter((_, i) => i !== index));
  }

  function commit(widget: Widget, adding: boolean) {
    const list = current();
    if (adding) list.push(widget);
    else list.splice(list.findIndex((w) => w.id === widget.id), 1, widget);
    editor = null;
    save(list);
  }

  function useDefault() {
    if (dash) save(dash.defaultLayout.map(stored));
  }
</script>

<div class="section-head">
  <h2>Dashboard</h2>
  <div class="controls">
    {#if editing}
      <button onclick={() => (editor = { widget: null })} disabled={busy || editor !== null}>
        + Widget
      </button>
      {#if dash && !dash.isDefault}
        <button class="bare" onclick={useDefault} disabled={busy} title="Replace this layout with the one built from your ledger">
          Start again from the default
        </button>
      {/if}
      <button onclick={() => { editing = false; editor = null; }}>Done</button>
    {:else}
      <button onclick={() => (editing = true)} disabled={!dash}>Customize</button>
    {/if}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if editor && dash}
  <WidgetEditor
    existing={editor.widget}
    {dash}
    onsave={commit}
    oncancel={() => (editor = null)}
  />
{/if}

{#if editing}
  <p class="note accent">
    Customizing. Move, resize, edit or remove any widget, or add one. Changes save as you make them.
  </p>
{/if}

{#if dash && dash.widgets.length === 0}
  <div class="empty-state">
    <p><strong>Your dashboard is empty</strong></p>
    <p class="muted">
      Add the figures you want to see first: savings buckets, accounts, goals, retirement,
      reconciliation, spending and more.
    </p>
    <div class="actions">
      <button onclick={() => { editing = true; editor = { widget: null }; }}>Add a widget</button>
      <button class="bare" onclick={useDefault} disabled={busy}>Use the default</button>
    </div>
  </div>
{:else if dash}
  <div class="grid" bind:clientWidth={width}>
    {#each rows as row, r (r)}
      <div class="row-of" class:pair={row.length === 2}>
        {#each row as index (dash.widgets[index].id)}
          <DashWidget
            widget={dash.widgets[index]}
            {editing}
            first={index === 0}
            last={index === dash.widgets.length - 1}
            onmove={(delta) => move(index, delta)}
            onspan={() => toggleSpan(index)}
            onedit={() => (editor = { widget: stored(dash!.widgets[index]) })}
            onremove={() => remove(index)}
            {onopen}
          />
        {/each}
      </div>
    {/each}
  </div>
  {#if dash.historyDays < 2}
    <p class="note">
      Net worth history builds a point a day. Points from the plugin can be brought in under
      Storage.
    </p>
  {/if}
{/if}

<style>
  .controls {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .accent {
    color: var(--accent);
  }
  .grid {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .row-of {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.75rem;
  }
  .row-of.pair {
    grid-template-columns: 1fr 1fr;
  }
  .empty-state {
    max-width: 28rem;
    margin: 3rem auto;
    text-align: center;
  }
  .empty-state p {
    margin: 0 0 0.5rem;
  }
  .empty-state .actions {
    justify-content: center;
  }
</style>
