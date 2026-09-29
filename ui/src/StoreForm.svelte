<script lang="ts">
  import { untrack } from "svelte";
  import {
    describe,
    kinds,
    newStoreId,
    storage,
    type Secret,
    type Settings,
    type StoreConfig,
    manualKinds,
    type ManualKind,
  } from "./storage";

  let {
    existing = null,
    onsaved,
    oncancel,
  }: {
    existing?: StoreConfig | null;
    onsaved: () => void;
    oncancel: () => void;
  } = $props();

  // Drive is absent on purpose: it is connected by signing in, not by
  // filling in a form.
  let kind = $state<ManualKind>(
    untrack(() => {
      const current = existing?.settings.kind;
      return current && current !== "google-drive" ? current : "s3";
    }),
  );
  let label = $state(untrack(() => existing?.label ?? ""));
  let path = $state(untrack(() => existing && (existing.settings.kind === "local" || existing.settings.kind === "nas")
      ? existing.settings.path
      : "",));
  let bucket = $state(untrack(() => existing?.settings.kind === "s3" ? existing.settings.bucket : ""));
  let objectKey = $state(untrack(() => existing?.settings.kind === "s3" ? existing.settings.key : "ledger/ledger.json",));
  let region = $state(untrack(() => existing?.settings.kind === "s3" ? existing.settings.region : "us-east-1"));
  let endpoint = $state(untrack(() => existing?.settings.kind === "s3" ? (existing.settings.endpoint ?? "") : "",));
  let awsProfile = $state(
    untrack(() =>
      existing?.settings.kind === "s3" ? (existing.settings.aws_profile ?? "") : "",
    ),
  );
  let signWith = $state<"keys" | "profile">(
    untrack(() =>
      existing?.settings.kind === "s3" && existing.settings.aws_profile ? "profile" : "keys",
    ),
  );
  let accessKeyId = $state("");
  let secretAccessKey = $state("");
  let acceptRisk = $state(untrack(() => existing?.acceptRisk ?? false));

  let testing = $state(false);
  let saving = $state(false);
  let tested = $state("");
  let error = $state("");

  function settings(): Settings {
    if (kind === "s3") {
      return {
        kind: "s3",
        bucket: bucket.trim(),
        key: objectKey.trim(),
        region: region.trim(),
        endpoint: endpoint.trim() || undefined,
        aws_profile: signWith === "profile" ? awsProfile.trim() || undefined : undefined,
      };
    }
    return { kind, path: path.trim() };
  }

  function secret(): Secret | undefined {
    if (kind !== "s3" || signWith === "profile") return undefined;
    // Left blank when editing means "keep the key already stored".
    if (!accessKeyId.trim() && !secretAccessKey.trim()) return undefined;
    return {
      kind: "access-key",
      access_key_id: accessKeyId.trim(),
      secret_access_key: secretAccessKey.trim(),
    };
  }

  function draft(): StoreConfig {
    return {
      id: existing?.id ?? newStoreId(),
      label: label.trim() || kinds[kind].name,
      settings: settings(),
      acceptRisk,
    };
  }

  async function test() {
    testing = true;
    error = "";
    tested = "";
    try {
      tested = await storage.test(draft(), secret());
    } catch (e) {
      error = String(e);
    } finally {
      testing = false;
    }
  }

  async function save() {
    saving = true;
    error = "";
    try {
      await storage.save(draft(), secret());
      onsaved();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  const editingS3WithStoredKey = $derived(existing?.settings.kind === "s3");
</script>

<form class="panel" onsubmit={(e) => { e.preventDefault(); save(); }}>
  <h3>{existing ? "Edit this store" : "Add a store"}</h3>

  {#if !existing}
    <label class="field">
      <span>Kind</span>
      <select bind:value={kind}>
        {#each manualKinds as value (value)}
          <option {value}>{kinds[value].name}</option>
        {/each}
      </select>
    </label>
    <p class="note">{kinds[kind].blurb}</p>
  {/if}

  <label class="field">
    <span>Name</span>
    <input bind:value={label} placeholder={kinds[kind].name} />
  </label>

  {#if kind === "s3"}
    <label class="field">
      <span>Bucket</span>
      <input bind:value={bucket} placeholder="my-ledger-bucket" required />
    </label>
    <label class="field">
      <span>Object key</span>
      <input bind:value={objectKey} required />
    </label>
    <label class="field">
      <span>Region</span>
      <input bind:value={region} placeholder="us-east-1" required />
    </label>
    <label class="field">
      <span>Endpoint</span>
      <input bind:value={endpoint} placeholder="leave blank for Amazon S3" />
    </label>
    <p class="note">
      An endpoint points this at something other than Amazon — MinIO, Backblaze B2,
      Cloudflare R2 or Wasabi all speak the same API.
    </p>

    <label class="field">
      <span>Sign with</span>
      <select bind:value={signWith}>
        <option value="keys">A key pair kept in this computer's keychain</option>
        <option value="profile">A profile in ~/.aws/credentials</option>
      </select>
    </label>

    {#if signWith === "profile"}
      <label class="field">
        <span>Profile</span>
        <input bind:value={awsProfile} placeholder="default" required />
      </label>
      <p class="note">
        Read from ~/.aws/credentials when a request is signed, so this app keeps
        no second copy of the key. The file must not be readable by other users.
      </p>
    {:else}
    <label class="field">
      <span>Access key ID</span>
      <input
        bind:value={accessKeyId}
        autocomplete="off"
        placeholder={editingS3WithStoredKey ? "leave blank to keep the stored key" : ""}
      />
    </label>
    <label class="field">
      <span>Secret access key</span>
      <input
        type="password"
        bind:value={secretAccessKey}
        autocomplete="off"
        placeholder={editingS3WithStoredKey ? "leave blank to keep the stored key" : ""}
      />
    </label>
    <p class="note">
      Kept in this computer's keychain, never in a settings file. The app needs
      permission to read and write this one object, and nothing else.
    </p>
    {/if}
  {:else}
    <label class="field">
      <span>File</span>
      <input bind:value={path} placeholder="/path/to/ledger.json" required />
    </label>
  {/if}

  {#if !kinds[kind].safeAsSourceOfTruth}
    <label class="field check">
      <input type="checkbox" bind:checked={acceptRisk} />
      <span>
        Allow this to be the source of truth anyway. Network shares cannot lock
        reliably, so two machines writing at once can lose an edit.
      </span>
    </label>
  {/if}

  {#if tested}
    <p class="ok">Reachable.</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="actions">
    <button type="button" onclick={test} disabled={testing || saving}>
      {testing ? "Checking…" : "Test connection"}
    </button>
    <button type="submit" disabled={saving || testing}>
      {saving ? "Saving…" : "Save"}
    </button>
    <button type="button" class="bare" onclick={oncancel}>Cancel</button>
  </div>

  {#if existing}
    <p class="note">Currently {describe(existing.settings)}</p>
  {/if}
</form>
