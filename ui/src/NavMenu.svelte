<script lang="ts" generics="T extends string">
  // The tabs, grouped. A group of one is a plain button; a larger group opens
  // its pages on hover, on click, and from the keyboard, and names the page
  // that is open when it holds it.
  let {
    groups,
    current,
    onchoose,
  }: {
    groups: readonly { label: string; pages: readonly (readonly [T, string])[] }[];
    current: T;
    onchoose: (page: T) => void;
  } = $props();

  let open = $state<string | null>(null);
  let closing: ReturnType<typeof setTimeout> | undefined;

  function show(label: string) {
    clearTimeout(closing);
    open = label;
  }

  // A moment's grace, so crossing the gap from the button to the menu does
  // not close it.
  function hide() {
    clearTimeout(closing);
    closing = setTimeout(() => (open = null), 150);
  }

  function choose(page: T) {
    clearTimeout(closing);
    open = null;
    onchoose(page);
  }

  function leaveFocus(e: FocusEvent, label: string) {
    const next = e.relatedTarget as Node | null;
    if (!(e.currentTarget as HTMLElement).contains(next) && open === label) open = null;
  }

  function keys(e: KeyboardEvent, label: string) {
    if (e.key === "Escape" && open === label) {
      open = null;
      ((e.currentTarget as HTMLElement).querySelector("button") as HTMLButtonElement)?.focus();
    } else if (e.key === "ArrowDown" && open !== label) {
      e.preventDefault();
      show(label);
    }
  }
</script>

<nav aria-label="Pages">
  {#each groups as group (group.label)}
    {@const holds = group.pages.find(([page]) => page === current)}
    {#if group.pages.length === 1}
      <button class="group" class:on={holds} onclick={() => choose(group.pages[0][0])}>
        {group.pages[0][1]}
      </button>
    {:else}
      <div
        class="menu-wrap"
        role="presentation"
        onmouseenter={() => show(group.label)}
        onmouseleave={hide}
        onfocusout={(e) => leaveFocus(e, group.label)}
        onkeydown={(e) => keys(e, group.label)}
      >
        <button
          class="group"
          class:on={holds}
          aria-haspopup="menu"
          aria-expanded={open === group.label}
          onclick={() => (open === group.label ? (open = null) : show(group.label))}
        >
          {group.label}{#if holds}<span class="here"> · {holds[1]}</span>{/if}
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        {#if open === group.label}
          <div class="menu" role="menu">
            {#each group.pages as [page, label] (page)}
              <button role="menuitem" class:on={page === current} onclick={() => choose(page)}>
                {label}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/each}
</nav>

<style>
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .group.on {
    background: var(--raised-strong);
    border-color: var(--faint);
  }
  .here {
    color: var(--accent);
  }
  .caret {
    font-size: 0.7em;
    color: var(--dim);
    margin-left: 0.2rem;
  }
  .menu-wrap {
    position: relative;
  }
  /* Padding rather than a margin above the list, so the pointer never leaves
     the wrapper on its way down. */
  .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    min-width: 11rem;
    padding: 0.3rem;
    margin-top: 0.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    background: var(--menu);
    border: 1px solid var(--hairline);
    border-radius: 0.4rem;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }
  .menu::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -0.3rem;
    height: 0.3rem;
  }
  .menu button {
    text-align: left;
    border-color: transparent;
  }
  .menu button.on {
    color: var(--accent);
    background: var(--raised);
  }
</style>
