<script lang="ts">
  import MemberPicker from "./MemberPicker.svelte";
  import {
    ledger,
    money,
    type BankMapping,
    type BucketView,
    type ImportPreview,
    type ReconciliationView,
  } from "./ledger";

  // Charges from a bank export onto an open statement. The file is read in
  // Rust; this shows what it found, lets the columns be corrected when the
  // guess is wrong, and adds the rows that stay ticked.
  let {
    record,
    buckets,
    members,
    onchanged,
    onclose,
  }: {
    record: ReconciliationView;
    buckets: BucketView[];
    members: string[];
    onchanged: () => void;
    onclose: () => void;
  } = $props();

  let text = $state("");
  let fileName = $state("");
  let preview = $state<ImportPreview | null>(null);
  let mapping = $state<BankMapping | null>(null);
  let chosen = $state<boolean[]>([]);
  let member = $state("All");
  let bucketId = $state("");
  let categoryInNotes = $state(true);
  let adjusting = $state(false);
  let busy = $state(false);
  let error = $state("");

  async function readFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    fileName = file.name;
    text = await file.text();
    mapping = null;
    await look();
  }

  async function look() {
    if (!text.trim()) return;
    busy = true;
    error = "";
    try {
      preview = await ledger.transactionsPreview(record.id, text, mapping);
      mapping = { ...preview.mapping };
      chosen = preview.rows.map((r) => r.suggested);
    } catch (e) {
      error = String(e);
      preview = null;
    } finally {
      busy = false;
    }
  }

  // Changing a column or the sign reads the file again with that choice.
  function remap(change: Partial<BankMapping>) {
    mapping = { ...mapping!, ...change };
    look();
  }

  const column = (v: string) => (v === "" ? null : Number(v));

  const picked = $derived(preview ? preview.rows.filter((_, i) => chosen[i]) : []);
  // Whole cents, so adding many amounts never drifts; for this label only.
  const pickedTotal = $derived(
    (picked.reduce((t, r) => t + Math.round(Number(r.value) * 100), 0) / 100).toFixed(2),
  );

  async function add() {
    busy = true;
    error = "";
    try {
      await ledger.apply({
        op: "reconcile-import",
        id: record.id,
        lines: picked.map((r) => ({
          label: r.description,
          amount: r.value,
          spentOn: r.date,
          member: member.trim(),
          bucketId,
          notes: categoryInNotes ? r.category : "",
        })),
      });
      onchanged();
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function status(r: ImportPreview["rows"][number]): string {
    if (r.problem) return r.problem;
    if (r.duplicate) return "already on this statement";
    if (r.afterStatement) return "after the statement date";
    return "";
  }
</script>

<section class="panel">
  <h3>Import charges into {record.card}</h3>
  <p class="note">
    Export the card's transactions from your bank as CSV and choose the file. Purchases are
    picked out; payments, refunds and anything already on this statement are left unticked.
  </p>

  <label class="field">
    <span>Export file</span>
    <input type="file" accept=".csv,.txt,text/csv" onchange={readFile} disabled={busy} />
  </label>
  {#if fileName && busy && !preview}<p class="muted">Reading {fileName}…</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}

  {#if preview && mapping}
    {#each preview.notes as note (note)}<p class="warn-text">{note}</p>{/each}

    <p class="summary">
      {preview.rows.length} rows: {preview.charges} to add{#if preview.duplicates}, {preview.duplicates}
        already here{/if}{#if preview.setAside}, {preview.setAside} payments or credits set aside{/if}.
      <button class="bare" onclick={() => (adjusting = !adjusting)}>
        {adjusting ? "Hide columns" : "Wrong columns?"}
      </button>
    </p>

    {#if adjusting}
      <div class="mapping">
        {#each [["date", "Date"], ["description", "Description"], ["amount", "Amount"], ["debit", "Debit (money out)"], ["credit", "Credit (money in)"], ["category", "Category"]] as [key, label] (key)}
          <label class="field">
            <span>{label}</span>
            <select
              value={mapping[key as keyof BankMapping] === null ? "" : String(mapping[key as keyof BankMapping])}
              onchange={(e) => remap({ [key]: column(e.currentTarget.value) })}
            >
              <option value="">—</option>
              {#each preview.headers as h, i (i)}<option value={String(i)}>{h}</option>{/each}
            </select>
          </label>
        {/each}
        <label class="field check">
          <input
            type="checkbox"
            checked={mapping.chargesNegative}
            onchange={(e) => remap({ chargesNegative: e.currentTarget.checked })}
          />
          <span>Purchases are negative amounts</span>
        </label>
        <label class="field check">
          <input
            type="checkbox"
            checked={mapping.dayFirst}
            onchange={(e) => remap({ dayFirst: e.currentTarget.checked })}
          />
          <span>Dates are day first (31/12/2026)</span>
        </label>
      </div>
    {/if}

    <div class="defaults">
      <label class="field">
        <span>Spent by</span>
        <MemberPicker bind:value={member} {members} />
      </label>
      <label class="field">
        <span>Paid from</span>
        <select bind:value={bucketId}>
          <option value="">Everyday spending</option>
          {#each buckets as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
        </select>
      </label>
      <label class="field check">
        <input type="checkbox" bind:checked={categoryInNotes} />
        <span>Keep the bank's category as a note</span>
      </label>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>
              <input
                type="checkbox"
                aria-label="All"
                checked={picked.length > 0 && picked.length === preview.rows.filter((r) => !r.problem).length}
                onchange={(e) => {
                  const on = e.currentTarget.checked;
                  chosen = preview!.rows.map((r) => on && !r.problem);
                }}
              />
            </th>
            <th>Date</th>
            <th>Description</th>
            <th>Category</th>
            <th class="num">Amount</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each preview.rows as r, i (r.line)}
            <tr class:off={!chosen[i]}>
              <td><input type="checkbox" bind:checked={chosen[i]} disabled={!!r.problem} /></td>
              <td>{r.date || "—"}</td>
              <td>{r.description}</td>
              <td class="muted">{r.category}</td>
              <td class="num" class:pos={!r.charge}>{r.charge ? "" : "+"}{money(r.value)}</td>
              <td class="status" class:warn-text={r.duplicate || r.afterStatement}>{status(r)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <div class="actions">
    {#if preview}
      <button onclick={add} disabled={busy || picked.length === 0}>
        Add {picked.length} charge{picked.length === 1 ? "" : "s"} · {money(pickedTotal)}
      </button>
    {/if}
    <button class="bare" onclick={onclose} disabled={busy}>Cancel</button>
  </div>
  {#if preview}
    <p class="note">
      The statement balance stays {money(record.balance)}; the charges then show what is still
      unaccounted. Buckets and who spent what can be changed per charge with Edit.
    </p>
  {/if}
</section>

<style>
  .summary {
    margin: 0.5rem 0;
  }
  .mapping,
  .defaults {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
    gap: 0 0.75rem;
    align-items: end;
    margin-bottom: 0.5rem;
  }
  .table-wrap {
    max-height: 22rem;
    overflow: auto;
    border: 1px solid var(--hairline);
    border-radius: 0.35rem;
    margin: 0.5rem 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.82rem;
  }
  th,
  td {
    padding: 0.3rem 0.5rem;
    text-align: left;
    border-bottom: 1px solid var(--hairline);
    white-space: nowrap;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--menu);
    color: var(--dim);
    font-weight: 600;
  }
  td:nth-child(3) {
    white-space: normal;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .status {
    font-size: 0.75rem;
    color: var(--faint);
  }
  tr.off td {
    opacity: 0.55;
  }
</style>
