<script lang="ts">
  import type { Snippet } from "svelte";

  let { onclose, children }: { onclose: () => void; children: Snippet } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });
</script>

<dialog bind:this={dialog} class="modal" {onclose}>
  {@render children()}
</dialog>
