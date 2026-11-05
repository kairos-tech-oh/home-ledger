<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    onclose,
    children,
    wide = false,
    lightDismiss = false,
    label = "Close",
  }: {
    onclose: () => void;
    children: Snippet;
    /** Room for a page of detail rather than a form. */
    wide?: boolean;
    /**
     * A click outside closes it. Only for something to read: a form loses
     * what was typed to one stray click.
     */
    lightDismiss?: boolean;
    /** What the corner button says to a screen reader. */
    label?: string;
  } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  // Escape closes a dialog natively and fires `close`; this is the backdrop.
  // A click on the backdrop lands on the dialog itself, outside its box.
  function outside(e: MouseEvent) {
    if (!lightDismiss || e.target !== dialog) return;
    const box = dialog.getBoundingClientRect();
    const inside =
      e.clientX >= box.left &&
      e.clientX <= box.right &&
      e.clientY >= box.top &&
      e.clientY <= box.bottom;
    if (!inside) dialog.close();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} class="modal" class:wide {onclose} onclick={outside}>
  {@render children()}
  <!-- After the content, so opening focuses the first field, not this. -->
  <button type="button" class="bare modal-close" aria-label={label} title={label} onclick={() => dialog.close()}>
    ×
  </button>
</dialog>
