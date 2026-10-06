<script lang="ts">
  import {
    bank,
    ledger,
    money,
    type AccountView,
    type BalanceProposal,
    type BankItemView,
    type BankStatus,
  } from "./ledger";

  // Bank connections through Plaid, with the person's own keys. Rust does
  // every call to Plaid; this shows where things stand and asks what to do.
  // docs/BANK-CONNECTIONS.md has the design.

  let status = $state<BankStatus | null>(null);
  let accounts = $state<AccountView[]>([]);
  let busy = $state("");
  let error = $state("");
  let said = $state("");

  let environment = $state("sandbox");
  let clientId = $state("");
  let secret = $state("");

  /// A sign-in waiting in the browser: its token, its page, and the
  /// connection it signs in again, if any.
  let connecting = $state<{ token: string; url: string; item: string | null } | null>(null);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let disconnecting = $state<BankItemView | null>(null);
  let proposals = $state<BalanceProposal[] | null>(null);
  let accept = $state<string[]>([]);

  async function load() {
    try {
      status = await bank.status();
      accounts = (await ledger.read()).accounts;
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    load();
    return () => {
      if (timer) clearTimeout(timer);
    };
  });

  async function run(label: string, action: () => Promise<unknown>, done = "") {
    busy = label;
    error = "";
    said = "";
    try {
      await action();
      said = done;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  function saveKeys() {
    return run(
      "keys",
      async () => {
        status = await bank.saveKeys(clientId, secret, environment);
        clientId = "";
        secret = "";
      },
      "Plaid accepted the keys; they are in this computer's keychain.",
    );
  }

  // Thirty minutes is longer than anyone takes to sign in to a bank.
  const POLL_MS = 2000;
  const GIVE_UP_AFTER = (30 * 60 * 1000) / POLL_MS;

  function startConnect(item: string | null) {
    return run("connect", async () => {
      const link = await bank.connect(item);
      connecting = { ...link, item };
      poll(0);
    });
  }

  function stopConnect() {
    if (timer) clearTimeout(timer);
    timer = null;
    connecting = null;
  }

  function poll(tries: number) {
    timer = setTimeout(async () => {
      if (!connecting) return;
      try {
        const now = await bank.connectCheck(connecting.token, connecting.item);
        if (now.state === "waiting") {
          if (tries < GIVE_UP_AFTER) poll(tries + 1);
          else {
            stopConnect();
            error = "The sign-in page was left open too long; connect again.";
          }
          return;
        }
        const item = connecting.item ?? now.item?.id ?? null;
        stopConnect();
        if (now.state === "exited") {
          error = now.message || "The sign-in was closed before a bank was connected.";
          return;
        }
        said =
          now.state === "connected"
            ? `${now.item?.institution ?? "The bank"} is connected. Link its accounts below.`
            : "Signed in again.";
        if (item) await bank.fetch(item);
        await load();
      } catch (e) {
        stopConnect();
        error = String(e);
      }
    }, POLL_MS);
  }

  function link(item: BankItemView, account: string, ledgerAccount: string) {
    return run("link", async () => {
      status = await bank.link(item.id, account, ledgerAccount);
    });
  }

  function fetchNow(item: BankItemView | null) {
    return run("fetch", async () => {
      const got = await bank.fetch(item?.id ?? null);
      await load();
      const problems = got.items.filter((i) => i.problem);
      if (problems.length) {
        error = problems.map((i) => `${i.institution}: ${i.problem}`).join(" ");
        return;
      }
      const added = got.items.reduce((t, i) => t + i.added, 0);
      said = `Fetched: ${added} new transaction${added === 1 ? "" : "s"}.`;
    });
  }

  function disconnect(item: BankItemView) {
    return run(
      "disconnect",
      async () => {
        status = await bank.disconnect(item.id);
        disconnecting = null;
      },
      `${item.institution} is disconnected. Charges already added stay on their statements.`,
    );
  }

  function checkBalances() {
    return run("balances", async () => {
      await bank.fetch(null);
      proposals = await bank.balances();
      accept = proposals.map((p) => p.accountId);
      await load();
    });
  }

  function acceptBalances() {
    return run("balances", async () => {
      const n = await bank.applyBalances(accept);
      proposals = null;
      said = `${n} balance${n === 1 ? "" : "s"} updated from the bank.`;
    });
  }

  const linkable = $derived(accounts.slice().sort((a, b) => a.name.localeCompare(b.name)));

  // The ledger types each kind of bank account can be, as Rust checks it
  // (bank::fits): a savings account is never offered for a credit card.
  const FITS: Record<string, string[]> = {
    depository: ["checking", "savings", "other"],
    credit: ["credit", "heloc", "other"],
    loan: ["loan", "heloc", "other"],
    investment: ["investment", "retirement-roth", "retirement-traditional", "other"],
  };
  const fitting = (kind: string) =>
    linkable.filter((l) =>
      (FITS[kind] ?? Object.values(FITS).flat()).includes(l.kind),
    );

  function when(iso: string): string {
    if (!iso) return "never";
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  }
</script>

<div class="section-head">
  <h2>Bank connections</h2>
</div>

{#if error}<p class="error">{error}</p>{/if}
{#if said}<p class="ok">{said}</p>{/if}

<div class="panel">
  <p class="note">
    Fetches card charges and account balances from your bank through Plaid, using your own Plaid
    developer keys. Charges come into a statement's import to choose from, as a CSV does, and
    balances are offered for you to accept. Nothing changes without you.
  </p>

  {#if status && !status.keychain}
    <p class="warn-text">
      This computer has no keychain, so keys and connections would not survive a restart.
    </p>
  {/if}

  {#if status?.environment}
    <p class="note">
      Plaid keys for <strong>{status.environment}</strong> are in this computer's keychain.
    </p>
    <div class="actions">
      <button onclick={() => startConnect(null)} disabled={!!busy || !!connecting}>
        Connect a bank
      </button>
      {#if status.items.length}
        <button onclick={() => fetchNow(null)} disabled={!!busy}>
          {busy === "fetch" ? "Fetching…" : "Fetch now"}
        </button>
        <button onclick={checkBalances} disabled={!!busy}>
          {busy === "balances" ? "Checking…" : "Check balances"}
        </button>
      {/if}
      <button
        class="bare"
        onclick={() => run("keys", async () => (status = await bank.forgetKeys()), "Keys removed.")}
        disabled={!!busy}
      >
        Forget the keys
      </button>
    </div>
  {:else if status}
    <p class="note">
      Sign up at dashboard.plaid.com, then paste the client id and the secret for the environment
      you will use. Sandbox works straight away with Plaid's test banks; real banks need Production
      access from Plaid.
    </p>
  {/if}

  {#if status && !status.environment}
    <div class="keys">
      <label class="field">
        <span>Environment</span>
        <select bind:value={environment}>
          <option value="sandbox">Sandbox (test banks)</option>
          <option value="production">Production (your banks)</option>
        </select>
      </label>
      <label class="field">
        <span>Client id</span>
        <input bind:value={clientId} autocomplete="off" spellcheck="false" />
      </label>
      <label class="field">
        <span>Secret</span>
        <input type="password" bind:value={secret} autocomplete="off" />
      </label>
    </div>
    <div class="actions">
      <button
        onclick={saveKeys}
        disabled={!!busy || clientId.trim() === "" || secret.trim() === ""}
      >
        {busy === "keys" ? "Checking with Plaid…" : "Save"}
      </button>
    </div>
  {/if}

  {#if connecting}
    <div class="warn-box">
      <p>
        Plaid's sign-in page is open in your browser. Finish there and this updates by itself.
      </p>
      <p class="muted small">
        Not open? Paste this into your browser: <span class="url">{connecting.url}</span>
      </p>
      <div class="actions">
        <button class="bare" onclick={stopConnect}>Stop waiting</button>
      </div>
    </div>
  {/if}

  {#if proposals}
    <div class="proposals">
      {#if proposals.length === 0}
        <p class="note">Every linked account already matches the bank.</p>
      {:else}
        <p class="note">
          The bank's balances where they differ. A card with an open statement is left out: its
          statement sets what it owes.
        </p>
        {#each proposals as p (p.accountId)}
          <label class="field check">
            <input type="checkbox" bind:group={accept} value={p.accountId} />
            <span>
              <strong>{p.account}</strong>: {money(p.from)} → {money(p.to)}
              {#if p.availableCredit}<span class="muted">, {money(p.availableCredit)} available</span>{/if}
              <span class="muted small">({p.institution} {p.bankAccount})</span>
            </span>
          </label>
        {/each}
      {/if}
      <div class="actions">
        {#if proposals.length}
          <button onclick={acceptBalances} disabled={!!busy || accept.length === 0}>
            Accept {accept.length}
          </button>
        {/if}
        <button class="bare" onclick={() => (proposals = null)}>Close</button>
      </div>
    </div>
  {/if}

  {#each status?.items ?? [] as item (item.id)}
    <div class="item">
      <div class="item-head">
        <strong>{item.institution}</strong>
        <span class="muted small">
          fetched {when(item.fetchedAt)} · {item.transactions} transaction{item.transactions === 1
            ? ""
            : "s"} kept{#if item.gathering}
            · Plaid is still gathering history{/if}
        </span>
      </div>
      {#if item.needsSignIn}
        <p class="warn-text">
          The bank wants you to sign in again.
          <button class="bare" onclick={() => startConnect(item.id)} disabled={!!busy || !!connecting}>
            Sign in again
          </button>
        </p>
      {:else if item.problem}
        <p class="warn-text">{item.problem}</p>
      {/if}
      <table>
        <tbody>
          {#each item.accounts as a (a.id)}
            <tr>
              <td>
                {a.name}{#if a.mask}<span class="muted"> ··{a.mask}</span>{/if}
                <div class="muted small">{a.subtype || a.kind}</div>
              </td>
              <td class="num">{a.current === null ? "—" : money(a.current)}</td>
              <td>
                <select
                  value={a.linked}
                  disabled={!!busy}
                  onchange={(e) => link(item, a.id, e.currentTarget.value)}
                  aria-label="Ledger account for {a.name}"
                >
                  <option value="">Not used</option>
                  {#each fitting(a.kind) as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
                </select>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="actions">
        <button class="bare" onclick={() => (disconnecting = item)} disabled={!!busy}>
          Disconnect
        </button>
      </div>
      {#if disconnecting?.id === item.id}
        <div class="warn-box">
          <p class="confirm-text">
            Disconnect <strong>{item.institution}</strong>? Plaid stops reading it, and its fetched
            transactions are forgotten here. Charges already added to statements stay.
          </p>
          <div class="actions">
            <button onclick={() => disconnect(item)} disabled={!!busy}>Disconnect</button>
            <button class="bare" onclick={() => (disconnecting = null)}>Keep it</button>
          </div>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .keys {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
    gap: 0 0.75rem;
  }
  .item {
    border-top: 1px solid var(--hairline);
    margin-top: 0.75rem;
    padding-top: 0.75rem;
  }
  .item-head {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: baseline;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
    margin-top: 0.4rem;
  }
  td {
    padding: 0.3rem 0.4rem;
    border-bottom: 1px solid var(--hairline);
    vertical-align: middle;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .small {
    font-size: 0.75rem;
  }
  .url {
    user-select: all;
    word-break: break-all;
  }
  .proposals {
    margin: 0.5rem 0;
  }
</style>
