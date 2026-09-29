<script lang="ts">
  import { money, type GroupView } from "./ledger";

  // One ranked list with a bar per row. The bar is already measured in Rust
  // against the list's largest row, by count or by amount.
  let {
    title,
    subtitle = "",
    rows,
    step = 10,
  }: { title: string; subtitle?: string; rows: GroupView[]; step?: number } = $props();

  let shown = $state(10);
  // A new report starts short again.
  $effect(() => {
    void rows;
    shown = 10;
  });
</script>

<article class="breakdown">
  <h3>{title}</h3>
  {#if subtitle}<p class="sub">{subtitle}</p>{/if}
  {#if rows.length === 0}
    <p class="faint">No matching records</p>
  {:else}
    <ul>
      {#each rows.slice(0, shown) as row (row.key)}
        <li>
          <div class="line">
            <span class="name">{row.name}</span>
            <span class="figures">{row.count}× · {money(row.amount)}</span>
          </div>
          <span class="track"><span class="fill" style:width="{row.share * 100}%"></span></span>
        </li>
      {/each}
    </ul>
    {#if rows.length > shown}
      <button class="bare" onclick={() => (shown += step * 2)}>
        Show more ({rows.length - shown})
      </button>
    {/if}
  {/if}
</article>

<style>
  .breakdown {
    background: var(--raised);
    border-radius: 0.5rem;
    padding: 0.75rem 0.9rem;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 0.9rem;
  }
  .sub {
    margin: 0.1rem 0 0.5rem;
    font-size: 0.72rem;
    color: var(--dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .figures {
    color: var(--dim);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .track {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--hairline);
    overflow: hidden;
    margin-top: 0.15rem;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .faint {
    color: var(--faint);
    margin: 0;
  }
</style>
