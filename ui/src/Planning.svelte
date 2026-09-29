<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { ledger, money, wholeMoney, type PlanningView } from "./ledger";

  // Re-read whenever the ledger changes underneath, so an edit elsewhere shows.
  let { revision }: { revision: number } = $props();

  /** A local calendar date, `months` from today, the way the plugin steps it. */
  function isoIn(months: number): string {
    const d = new Date();
    d.setMonth(d.getMonth() + months);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  const presets = [
    { label: "3 months", months: 3 },
    { label: "6 months", months: 6 },
    { label: "1 year", months: 12 },
    { label: "2 years", months: 24 },
  ];

  let target = $state(isoIn(12));
  let plan = $state<PlanningView | null>(null);
  let error = $state("");
  const open = new SvelteSet<string>();

  const valid = $derived(/^\d{4}-\d{2}-\d{2}$/.test(target) && target > isoIn(0));

  $effect(() => {
    void revision;
    const to = target;
    if (!valid) {
      plan = null;
      return;
    }
    ledger
      .planning(isoIn(0), to)
      .then((p) => {
        // A slower answer for an older date must not overwrite a newer one.
        if (to === target) {
          plan = p;
          error = "";
        }
      })
      .catch((e) => (error = String(e)));
  });

  function toggle(id: string) {
    if (open.has(id)) open.delete(id);
    else open.add(id);
  }

  const negative = (value: string) => value.startsWith("-");

  function plural(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
  }
</script>

<div class="bar-controls">
  <label class="field">
    <span>Project to</span>
    <input type="date" bind:value={target} min={isoIn(0)} />
  </label>
  <div class="presets">
    {#each presets as preset (preset.months)}
      <button
        class:on={target === isoIn(preset.months)}
        onclick={() => (target = isoIn(preset.months))}>{preset.label}</button
      >
    {/each}
  </div>
</div>

{#if error}<p class="error">{error}</p>{/if}

{#if !valid}
  <p class="empty">Pick a date in the future to project to.</p>
{:else if plan}
  <section class="tiles">
    <article class="tile">
      <div class="tile-label">Saved now</div>
      <div class="tile-figure">{money(plan.current)}</div>
      <div class="tile-note">across {plural(plan.buckets.length, "bucket", "buckets")}</div>
    </article>
    <article class="tile">
      <div class="tile-label">Goes in</div>
      <div class="tile-figure pos">{money(plan.contributions)}</div>
      <div class="tile-note">over {plan.months} months of budget</div>
    </article>
    <article class="tile">
      <div class="tile-label">Comes out</div>
      <div class="tile-figure" class:neg={plan.deductions !== "0.00"}>{money(plan.deductions)}</div>
      <div class="tile-note">
        {plan.deductions !== "0.00"
          ? "planned draws falling in that window"
          : "nothing planned in that window"}
      </div>
    </article>
    <article class="tile">
      <div class="tile-label">Ends up</div>
      <div class="tile-figure" class:neg={negative(plan.change)} class:pos={!negative(plan.change)}>
        {money(plan.projected)}
      </div>
      <div class="tile-note">
        {negative(plan.change) ? "down" : "up"}
        {wholeMoney(plan.change.replace(/^-/, ""))} on today
      </div>
    </article>
  </section>

  {#if plan.buckets.length === 0}
    <p class="empty">
      No bucket has a balance or a budget line feeding it, so there is nothing to project.
    </p>
  {:else}
    <div class="plan-head">
      <span>Bucket</span>
      <span class="num">Now</span>
      <span class="num">{plan.to}</span>
      <span class="num">Of target</span>
    </div>
    <ul class="rows">
      {#each plan.buckets as bucket (bucket.id)}
        {@const expandable = bucket.draws.length > 0}
        <li class="plan-row">
          <button
            class="plan-line"
            class:expandable
            disabled={!expandable}
            aria-expanded={expandable ? open.has(bucket.id) : undefined}
            onclick={() => toggle(bucket.id)}
          >
            <span class="row-main">
              <span class="row-name">{bucket.name}</span>
              <span class="row-meta">
                {bucket.velocity !== null
                  ? `${wholeMoney(bucket.velocity)} a month in`
                  : "nothing feeds it"}
                {#if bucket.deductions !== "0.00"}
                  · {plural(bucket.draws.length, "draw", "draws")} taking {wholeMoney(bucket.deductions)}
                  {open.has(bucket.id) ? "▾" : "▸"}
                {/if}
                {#if bucket.monthsToGoal !== null && bucket.monthsToGoal > 0}
                  · {bucket.monthsToGoal} months to target
                {:else if bucket.monthsToGoal === 0}
                  · target reached
                {/if}
              </span>
            </span>
            <span class="num muted">{money(bucket.current)}</span>
            <span class="num strong" class:neg={negative(bucket.change)} class:pos={!negative(bucket.change)}>
              {money(bucket.projected)}
            </span>
            <span
              class="num"
              class:faint={bucket.percent === null}
              class:pos={bucket.percent !== null && bucket.percent >= 100}
            >
              {bucket.percent === null ? "—" : `${bucket.percent.toFixed(1)}%`}
            </span>
          </button>
          {#if open.has(bucket.id)}
            <ul class="draws">
              {#each bucket.draws as draw, i (i)}
                <li>
                  <span>
                    {draw.name} · {draw.schedule} · {plural(draw.occurrences, "time", "times")}
                  </span>
                  <span class="neg">−{wholeMoney(draw.impact)}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .bar-controls {
    display: flex;
    align-items: flex-end;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin: 0.75rem 0;
  }
  .bar-controls .field {
    margin: 0;
  }
  .presets {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
  }
  .presets button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .plan-head,
  .plan-line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 8rem 8rem 6rem;
    gap: 0.6rem;
    align-items: center;
  }
  .plan-head {
    padding: 0 0.5rem 0.3rem;
    font-size: 0.72rem;
    color: var(--dim);
    border-bottom: 1px solid var(--hairline);
  }
  .plan-row {
    list-style: none;
    border-bottom: 1px solid var(--hairline);
  }
  .plan-line {
    width: 100%;
    text-align: left;
    border: 0;
    border-radius: 0.25rem;
    padding: 0.45rem 0.5rem;
    white-space: normal;
  }
  .plan-line:disabled {
    opacity: 1;
    cursor: default;
  }
  .plan-line.expandable:hover {
    background: var(--raised);
  }
  .row-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .strong {
    font-weight: 600;
  }
  .faint {
    color: var(--faint);
  }
  .draws {
    margin: 0 0 0.4rem;
    padding: 0.35rem 0.75rem 0.35rem 1.5rem;
    background: var(--raised);
    border-radius: 0.25rem;
    font-size: 0.8rem;
    color: var(--dim);
  }
  .draws li {
    list-style: none;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
</style>
