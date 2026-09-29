<script lang="ts">
  import { ledger, money, wholeMoney, when, type BudgetView, type TemplateView } from "./ledger";

  // Saved budgets. Putting one back replaces every budget line, so the budget
  // it replaces is saved first unless you say otherwise.
  let {
    templates,
    budget,
    monthly,
    onchanged,
    onclose,
  }: {
    templates: TemplateView[];
    budget: BudgetView[];
    /** The live budget's monthly total, for the placeholder. */
    monthly: string;
    onchanged: () => void;
    onclose: () => void;
  } = $props();

  let name = $state("");
  let keepCurrent = $state(true);
  let confirming = $state<{ template: TemplateView; action: "activate" | "delete" } | null>(null);
  let busy = $state(false);
  let error = $state("");

  const canSave = $derived(name.trim() !== "" && budget.length > 0);

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await action();
      confirming = null;
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function saveCurrent() {
    if (!canSave) return;
    // Names and amounts, not links to live records, so the template survives
    // the budget being rewritten.
    const items = budget.map((line) => ({
      name: line.name,
      type: line.kind,
      monthlyAmount: line.monthlyAmount,
      bucketId: line.bucketId,
    }));
    run(async () => {
      await ledger.apply({
        op: "set",
        kind: "template",
        id: "",
        record: { name: name.trim(), items },
      });
      name = "";
    });
  }

  function confirm() {
    const c = confirming!;
    if (c.action === "activate") {
      run(() => ledger.apply({ op: "template-activate", id: c.template.id, keepCurrent }));
    } else {
      run(() => ledger.apply({ op: "delete", kind: "template", id: c.template.id }));
    }
  }

  function difference(t: TemplateView): string {
    const n = Number(t.againstNow);
    if (budget.length === 0 || Math.abs(n) < 1) return "";
    return ` · ${n > 0 ? "+" : "−"}${wholeMoney(t.againstNow.replace(/^-/, ""))} on now`;
  }
</script>

<section class="panel">
  <h3>Budget templates</h3>
  <p class="note">
    A saved copy of a whole budget. Putting one back replaces every line you have now.
  </p>

  <form class="save-row" onsubmit={(e) => { e.preventDefault(); saveCurrent(); }}>
    <label class="field grow">
      <span>Save the budget you have now</span>
      <input
        bind:value={name}
        maxlength="120"
        disabled={budget.length === 0}
        placeholder={budget.length > 0
          ? `Name it: ${budget.length} lines, ${wholeMoney(monthly)} a month`
          : "There is no budget to save yet"}
      />
    </label>
    <button type="submit" disabled={busy || !canSave}>Save</button>
  </form>

  {#if error}<p class="error">{error}</p>{/if}

  {#if confirming}
    <div class="warn-box">
      {#if confirming.action === "activate"}
        <p>
          Replace all {budget.length} budget lines with the {confirming.template.lines} in
          <strong>{confirming.template.name}</strong>?
          {keepCurrent
            ? "The budget you have now is saved as a template first."
            : "The budget you have now is not saved."}
        </p>
      {:else}
        <p>Delete the template <strong>{confirming.template.name}</strong>?</p>
      {/if}
      <div class="actions">
        <button disabled={busy} onclick={confirm}>
          {confirming.action === "activate" ? "Put it back" : "Delete"}
        </button>
        <button class="bare" onclick={() => (confirming = null)}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if templates.length === 0}
    <p class="empty">
      Nothing saved yet. A template is useful for a budget you switch between: a lean month,
      a month with the holiday in it.
    </p>
  {:else}
    <ul class="rows">
      {#each templates as t (t.id)}
        <li class="row">
          <div class="row-main">
            <span class="row-name">{t.name}</span>
            <span class="row-meta">
              {t.lines} line{t.lines === 1 ? "" : "s"} · {money(t.monthly)} a month{difference(t)}
              {#if t.savedAt} · saved {when(t.savedAt)}{/if}
            </span>
            {#if t.notes}<span class="row-meta faint">{t.notes}</span>{/if}
          </div>
          <div class="row-actions">
            <button
              class="bare"
              disabled={busy}
              title="Replace every budget line with this template's"
              onclick={() => (confirming = { template: t, action: "activate" })}>Put this back</button
            >
            <button
              class="bare danger"
              disabled={busy}
              title="Delete this template"
              onclick={() => (confirming = { template: t, action: "delete" })}>×</button
            >
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="actions">
    <label class="field check">
      <input type="checkbox" bind:checked={keepCurrent} />
      <span>Save what I replace</span>
    </label>
    <button class="bare" onclick={onclose}>Done</button>
  </div>
</section>

<style>
  .save-row {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .grow {
    flex: 1;
    margin: 0;
  }
  .faint {
    color: var(--faint);
  }
  .actions {
    justify-content: space-between;
    align-items: center;
  }
</style>
