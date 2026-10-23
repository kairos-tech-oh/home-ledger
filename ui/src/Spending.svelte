<script lang="ts">
  import Breakdown from "./Breakdown.svelte";
  import ChargeList from "./ChargeList.svelte";
  import {
    ledger,
    localToday,
    money,
    spendingPeriods,
    spendingStatuses,
    type SpendingView,
  } from "./ledger";

  let { revision, onchanged }: { revision: number; onchanged: () => void } = $props();

  let period = $state("1m");
  let status = $state("all");
  let from = $state("");
  let to = $state(localToday());
  let report = $state<SpendingView | null>(null);
  let error = $state("");
  // The whole window, or one person's part of it.
  let view = $state<"all" | "person">("all");
  let personKey = $state<string | null>(null);

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

  // The one chosen, or whoever spent the most when they are not in this window.
  const person = $derived(
    report?.perPerson.find((p) => p.key === personKey) ?? report?.perPerson[0] ?? null,
  );
  const theirs = $derived(
    report && person
      ? report.transactions.filter((c) => c.spentBy.toLowerCase() === person.key)
      : [],
  );
  const question = $derived(`${period}|${status}|${from}|${to}`);

  /// Whole percent of the total, for a person's share.
  function share(part: string, whole: string): string {
    const w = Number(whole);
    return w > 0 ? ((Number(part) / w) * 100).toFixed(0) : "0";
  }

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
  <div class="switch" role="group" aria-label="Show">
    <button class:on={view === "all"} onclick={() => (view = "all")}>Everyone</button>
    <button class:on={view === "person"} onclick={() => (view = "person")}>By person</button>
  </div>
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

  {#if view === "person"}
    {@render byPerson(report)}
  {:else}
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
  {#key question}
    <ChargeList
      charges={report.transactions}
      members={report.members}
      {onchanged}
      empty="No charges match these dates and status. Try All time or add lines to a reconciliation."
    />
  {/key}
  {/if}
{/if}

{#snippet byPerson(report: SpendingView)}
  {#if !person}
    <p class="empty">No charges match these dates and status, so no one has spent anything.</p>
  {:else}
    <div class="people" role="tablist" aria-label="Person">
      {#each report.perPerson as p (p.key)}
        <button
          role="tab"
          aria-selected={p.key === person.key}
          class="person"
          class:on={p.key === person.key}
          onclick={() => (personKey = p.key)}
        >
          <span class="person-name">{p.name}</span>
          <span class="person-figures">{money(p.total)} · {plural(p.count, "charge", "charges")}</span>
        </button>
      {/each}
    </div>

    <section class="tiles">
      {#each [
        [`${person.name} spent`, money(person.total), plural(person.count, "charge", "charges")],
        ["Share of spending", `${share(person.total, report.total)}%`, `of ${money(report.total)}`],
        ["Everyday spending", money(person.everyday), "not assigned to a bucket"],
        ["Bucket-funded", money(person.fromBuckets), "assigned to savings buckets"],
        ["Open", money(person.open), "not settled yet"],
        ["Settled", money(person.settled), "already paid off"],
      ] as [label, figure, note] (label)}
        <article class="tile">
          <div class="tile-label">{label}</div>
          <div class="tile-figure">{figure}</div>
          <div class="tile-note">{note}</div>
        </article>
      {/each}
    </section>

    <div class="breakdowns">
      <Breakdown title="Where it came from" subtitle="Each bucket, and everyday spending" rows={person.buckets} />
      <Breakdown title="Most frequent items" subtitle="Most occurrences first" rows={person.items} />
      <Breakdown title="Monthly spending" subtitle="Oldest first · purchase or fallback date" rows={person.months} />
      <Breakdown title="By card" subtitle="Itemized charges within the selected dates" rows={person.cards} />
    </div>

    <div class="section-head">
      <h2>{person.key ? `${person.name}'s charges` : "Charges with no one named"}</h2>
    </div>
    {#key `${question}|${person.key}`}
      <ChargeList charges={theirs} members={report.members} {onchanged} />
    {/key}
  {/if}
{/snippet}

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
  .switch {
    display: inline-flex;
  }
  .switch button {
    border-radius: 0;
  }
  .switch button:first-child {
    border-radius: 0.3rem 0 0 0.3rem;
  }
  .switch button:last-child {
    border-radius: 0 0.3rem 0.3rem 0;
  }
  .switch button.on,
  .person.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .people {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin: 0.75rem 0;
  }
  .person {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.1rem;
    padding: 0.4rem 0.75rem;
  }
  .person-name {
    font-weight: 600;
  }
  .person-figures {
    font-size: 0.75rem;
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }
</style>
