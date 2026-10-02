<script lang="ts">
  import { encryption, PASSPHRASE_MIN, type EncryptionStatus } from "./encryption";

  // Turning encryption on (with a passphrase, then showing the recovery code
  // once) and off (after checking the passphrase).
  let { onchanged }: { onchanged: () => void } = $props();

  let status = $state<EncryptionStatus | null>(null);
  let mode = $state<"" | "enable" | "disable">("");
  let passphrase = $state("");
  let repeat = $state("");
  let code = $state("");
  let saved = $state(false);
  let notes = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");

  async function refresh() {
    status = await encryption.status();
  }
  $effect(() => {
    refresh().catch((e) => (error = String(e)));
  });

  const strongEnough = $derived(passphrase.length >= PASSPHRASE_MIN);
  const matches = $derived(passphrase === repeat);

  function reset() {
    mode = "";
    passphrase = "";
    repeat = "";
    error = "";
  }

  async function enable() {
    busy = true;
    error = "";
    try {
      const on = await encryption.enable(passphrase);
      code = on.recoveryCode;
      saved = false;
      notes = [
        ...(on.kept ? [] : ["This computer has no keychain, so it will ask for the passphrase at every launch."]),
        ...on.skipped.map((s) => `Not reachable to encrypt now, encrypted when next written: ${s}`),
      ];
      reset();
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function disable() {
    busy = true;
    error = "";
    try {
      const skipped = await encryption.disable(passphrase);
      notes = skipped.map((s) => `Not reachable to decrypt now: ${s}`);
      reset();
      await refresh();
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function finish() {
    code = "";
    onchanged();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      // Clipboard refused: the code is on screen to copy by hand.
    }
  }
</script>

<div class="section-head">
  <h2>Encryption</h2>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#each notes as note (note)}<p class="warn-text">{note}</p>{/each}

<div class="panel">
  {#if code}
    <h3>Save your recovery code</h3>
    <p>
      This opens your ledger if the passphrase is ever forgotten. It is shown <strong>only now</strong>.
      Write it down or keep it in a password manager, somewhere other than this computer.
    </p>
    <p class="code">{code}</p>
    <div class="actions">
      <button class="bare" onclick={copy}>Copy</button>
    </div>
    <label class="field check">
      <input type="checkbox" bind:checked={saved} />
      <span>I have saved the recovery code somewhere safe</span>
    </label>
    <div class="actions">
      <button disabled={!saved} onclick={finish}>Done</button>
    </div>
  {:else if status}
    <p>
      {#if status.enabled}
        <strong class="pos">On.</strong> Your ledger, its history and snapshots are encrypted on every
        store and on this computer. Only someone with the passphrase or the recovery code can read
        them.
      {:else}
        <strong>Off.</strong> Your ledger is stored as readable text, on this computer and on every
        store. Turn encryption on so that only someone with your passphrase can read it.
      {/if}
    </p>

    {#if mode === "enable"}
      <form onsubmit={(e) => { e.preventDefault(); enable(); }}>
        <label class="field">
          <span>Passphrase (at least {PASSPHRASE_MIN} characters)</span>
          <input type="password" bind:value={passphrase} autocomplete="new-password" />
        </label>
        <label class="field">
          <span>The same again</span>
          <input type="password" bind:value={repeat} autocomplete="new-password" />
        </label>
        <p class="note">
          Each computer asks for it once, then keeps the key in its keychain. Next you get a
          recovery code. If both the passphrase and the code are lost, the data cannot be
          recovered by anyone.
        </p>
        {#if repeat && !matches}<p class="warn-text">The two do not match.</p>{/if}
        <div class="actions">
          <button type="submit" disabled={busy || !strongEnough || !matches}>
            {busy ? "Encrypting…" : "Turn on encryption"}
          </button>
          <button type="button" class="bare" onclick={reset}>Cancel</button>
        </div>
      </form>
    {:else if mode === "disable"}
      <form onsubmit={(e) => { e.preventDefault(); disable(); }}>
        <label class="field">
          <span>Passphrase</span>
          <input type="password" bind:value={passphrase} autocomplete="current-password" />
        </label>
        <p class="note">
          Everything is rewritten as readable text. Turn it off on your other computers too, or
          they will encrypt it again the next time they save.
        </p>
        <div class="actions">
          <button type="submit" disabled={busy || passphrase === ""}>
            {busy ? "Decrypting…" : "Turn off encryption"}
          </button>
          <button type="button" class="bare" onclick={reset}>Cancel</button>
        </div>
      </form>
    {:else}
      <div class="actions">
        {#if status.enabled}
          <button class="bare" onclick={() => (mode = "disable")}>Turn off…</button>
        {:else}
          <button onclick={() => (mode = "enable")}>Turn on encryption…</button>
        {/if}
      </div>
      {#if status.enabled}
        <p class="note">
          Your other computers ask for the passphrase or recovery code once. If your S3 bucket
          keeps old versions, copies saved before encryption are still readable there until those
          versions are removed.
        </p>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .code {
    font-size: 1.15rem;
    letter-spacing: 0.06em;
    padding: 0.6rem 0.8rem;
    background: var(--control);
    border: 1px solid var(--hairline);
    border-radius: 0.35rem;
    user-select: all;
    word-break: break-all;
  }
</style>
