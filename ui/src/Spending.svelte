<script lang="ts">
  import Breakdown from "./Breakdown.svelte";
  import {
    ledger,
    localToday,
    money,
    spendingPeriods,
    spendingStatuses,
    type SpendingView,
  } from "./ledger";

  let { revision, onchanged }: { revision: number; onchanged: () => void } = $props();

  const PAGE = 25;

  let period = $state("1m");
  let status = $state("all");
  let from = $state("");
  let to = $state(localToday());
  let report = $state<SpendingView | null>(null);
  let error = $state("");
  let page = $state(0);

  let managingFamily = $state(false);
  let familyText = $state("");
  let familyNote = $state("");

  $effect(() => {
    void revision;
    const asked = { period, status, from, to };
    ledger
      .spending(asked.period, asked.from, asked.to, asked.status)
      .then((r) => {
        // An older question answered late must not replace a newer answer.
        if (asked.period === period && asked.status === status && asked.from === from && asked.to === to) {
          report = r;
          page = 0;
          error = "";
        }
      })
      .catch((e) => (error = String(e)));
  });

  // Custom opens on the window the last period covered, so switching to it
  // changes nothing until a date is moved.
  function choosePeriod(next: string) {
    if (next === "custom" && report?.valid && report.start) {
      from = report.start;
      to = report.end;
    }
    period = next;
  }

  function openFamily() {
    if (!managingFamily) familyText = (report?.familyMembers ?? []).join(", ");
    managingFamily = !managingFamily;
    familyNote = "";
  }

  async function saveFamily() {
    try {
      const kept = await ledger.setFamilyMembers(familyText.split(","));
      familyText = kept.join(", ");
      familyNote = kept.length === 0 ? "No names kept." : `Kept ${kept.length} name${kept.length === 1 ? "" : "s"}.`;
      onchanged();
    } catch (e) {
      familyNote = String(e);
    }
  }

  const pages = $derived(report ? Math.ceil(report.transactions.length / PAGE) : 0);
  const shown = $derived(report ? report.transactions.slice(page * PAGE, (page + 1) * PAGE) : []);

  function plural(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
  }
</script>

<div class="filters">
  <label class="field">
    <span>Period</span>
    <select value={period} onchange={(e) => choosePeriod(e.currentTarget.value)}>
      {#each spendingPeriods as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
    </select>
  </label>
  <label class="field">
    <span>Reconciliation status</span>
    <select bind:value={status}>
      {#each spendingStatuses as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
    </select>
  </label>
  {#if period === "custom"}
    <label class="field">
      <span>From (inclusive)</span>
      <input type="date" bind:value={from} />
    </label>
    <label class="field">
      <span>To (inclusive)</span>
      <input type="date" bind:value={to} />
    </label>
  {/if}
  <button onclick={openFamily}>{managingFamily ? "Close family names" : "Family names"}</button>
</div>

{#if managingFamily}
  <form class="panel" onsubmit={(e) => { e.preventDefault(); saveFamily(); }}>
    <label class="field">
      <span>Family members (comma separated)</span>
      <input bind:value={familyText} maxlength="2000" />
    </label>
    <p class="note">
      Kept on this machine. Income owners and names already used on charges are offered as well,
      and names used in history are kept on the charges that use them.
    </p>
    <div class="actions">
      <button type="submit">Save family names</button>
      {#if familyNote}<span class="muted">{familyNote}</span>{/if}
    </div>
  </form>
{/if}

{#if error}<p class="error">{error}</p>{/if}

{#if report}
  <p class="note" class:neg={!report.valid}>
    {#if !report.valid}
      Choose dates with From on or before To.
    {:else}
      {period === "all" ? "All recorded dates" : `${report.start} through ${report.end}`} ·
      spending uses purchase dates, then statement dates, then settlement dates. Withdrawals
      use settlement dates. Reconciliation charges only; holdings, transfers and budget plans
      are not spending.
    {/if}
  </p>

  <section class="tiles">
    {#each [
      ["Itemized spending", money(report.total), plural(report.count, "charge", "charges")],
      ["Open charges", money(report.open), "not settled yet"],
      ["Settled charges", money(report.settled), "by spending date"],
      ["Buckets withdrawn", money(report.withdrawn), "actual settlement debits"],
      ["Bucket-funded charges", money(report.fromBuckets), "assigned to savings buckets"],
      ["Everyday spending", money(report.everyday), "not assigned to a bucket"],
      ["Average charge", money(report.average), "per recorded line"],
      ["Unattributed", money(report.unattributed), "no family member selected"],
    ] as [label, figure, note] (label)}
      <article class="tile">
        <div class="tile-label">{label}</div>
        <div class="tile-figure">{figure}</div>
        <div class="tile-note">{note}</div>
      </article>
    {/each}
  </section>

  <p class="note">
    {plural(report.inferredDates, "charge uses", "charges use")} a fallback date.
    {plural(report.undated, "undated charge", "undated charges")}
    {period === "all" ? "included" : "excluded"}.
    {money(report.unitemized)} in statement balances is not itemized and is excluded from
    spending totals.
    {#if report.missingSettlementHistory}
      {plural(report.missingSettlementHistory, "settlement has", "settlements have")} no debit
      history and {report.missingSettlementHistory === 1 ? "is" : "are"} excluded from withdrawals.
    {/if}
    {#if report.undatedWithdrawals}
      {plural(report.undatedWithdrawals, "settlement has", "settlements have")} no settlement
      date; {report.undatedWithdrawals === 1 ? "its" : "their"} recorded debits appear only in
      All time.
    {/if}
    Repeated item names are grouped ignoring case and extra spaces; frequency counts lines, not
    units purchased.
  </p>

  <div class="breakdowns">
    <Breakdown title="Who spent the most" subtitle="Highest total first · × is charge count" rows={report.people} />
    <Breakdown
      title="Most frequent items"
      subtitle="Most occurrences first · bar represents frequency"
      rows={report.items}
    />
    <Breakdown
      title="Monthly spending"
      subtitle="Recorded months, oldest first · purchase or fallback date"
      rows={report.months}
    />
    <Breakdown title="Spending by card" subtitle="Itemized charges within the selected dates" rows={report.cards} />
    <Breakdown
      title="Charges assigned to buckets"
      subtitle="Includes pending charges when Open is selected"
      rows={report.bucketSpending}
    />
    <Breakdown
      title="Actual bucket withdrawals"
      subtitle="Settled debits only · × is settlement count · undo removes the debit"
      rows={report.withdrawals}
    />
  </div>

  <div class="section-head"><h2>Matching charges</h2></div>
  {#if report.transactions.length === 0}
    <p class="empty">
      No charges match these dates and status. Try All time or add lines to a reconciliation.
    </p>
  {:else}
    <ul class="rows">
      {#each shown as charge, i (page * PAGE + i)}
        <li class="row">
          <div class="row-main">
            <span class="row-name">{charge.name}</span>
            <span class="row-meta">
              {charge.date || "Undated"}{charge.inferred && charge.date ? " (fallback)" : ""} ·
              {charge.member} · {charge.bucket} · {charge.card} · {charge.status}
            </span>
          </div>
          <span class="row-amount">{money(charge.amount)}</span>
        </li>
      {/each}
    </ul>
    {#if pages > 1}
      <div class="pager">
        <button disabled={page === 0} onclick={() => (page -= 1)}>Previous</button>
        <span class="muted">{page + 1} / {pages}</span>
        <button disabled={page + 1 >= pages} onclick={() => (page += 1)}>Next</button>
      </div>
    {/if}
  {/if}
{/if}

<style>
  .filters {
    display: flex;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 0.6rem;
    margin: 0.75rem 0 0.5rem;
  }
  .filters .field {
    margin: 0;
  }
  .breakdowns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr));
    gap: 0.75rem;
    margin: 0.75rem 0;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin: 0.75rem 0;
  }
</style>
