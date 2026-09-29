# Where your ledger lives

Every copy of this app stores its data in accounts **you** own. There is no
service behind it, no bucket belonging to whoever wrote it, and nothing to sign
up for. You bring an S3 bucket, a Google Drive, a NAS share, or nothing at all,
and the app uses it.

That has a consequence worth stating plainly: nobody else can recover your data
for you, so the app's job is to be careful with it.

## What you choose

1. **One store, or several.** One is the simple answer and the right default.
   Several gives you somewhere to fall back to when one is unreachable, and a
   continuously updated copy if you ever lose an account.
2. **Which kinds.** Amazon S3 and S3-compatible services, Google Drive, Google
   Cloud Storage, Azure Blob Storage, a NAS or network share, or **fully
   local** — a file on this computer and nothing leaves it.
3. **The order.** With several stores, which one is the **source of truth** and
   which are **backups** behind it.

## Where the choosing happens

**Not in the installer.** Three reasons, and the first two are hard:

- On Windows the installer runs elevated, often unattended, sometimes through a
  management tool. Prompting there for cloud credentials is the shape of a
  phishing dialog, and security software treats it that way.
- Google Drive and GCS need an OAuth round-trip through a browser. An MSI is
  the wrong place to open one, and a `.deb` postinstall script is worse.
- An installer that can fail on a network step is an installer that fails.

**First run, inside the app.** The first launch opens a setup flow that asks
the three questions above, and it is skippable: choosing nothing gives you a
local file and a working app, which is the fastest path to seeing whether you
want this at all. Nothing is mandatory at first launch except a choice to defer.

**Settings is the real home.** The same screens live in Settings permanently,
because a store gets added, re-keyed, re-ordered or removed long after the
first run. The setup flow is a friendly wrapper around the Settings screens —
one implementation, two entry points. Whatever we build for Settings, the
wizard gets for free.

## What each kind can actually promise

The one property that matters is **compare-and-swap**: can the service refuse a
write that is based on a stale read? Without it, two devices writing at once
silently lose one of the edits, and no amount of care in the app fixes it.

| Kind | Compare-and-swap | How |
|---|---|---|
| Amazon S3, and S3-compatible | **native** | `If-Match` on the ETag |
| Google Cloud Storage | **native** | `x-goog-if-generation-match` |
| Azure Blob Storage | **native** | `If-Match` on the ETag |
| Google Drive | **checked afterwards** | no `If-Match` in the v3 API; a version counter plus revision history |
| This computer | strong, locally | atomic rename, plus a content hash |
| NAS / network share | **best effort** | SMB and NFS do not lock dependably |

S3-compatible is worth calling out because it is nearly free once S3 works:
MinIO, Backblaze B2, Cloudflare R2, Wasabi and a dozen others speak the same
API. Supporting a custom endpoint turns one provider into many.

The Drive row is a correction to an earlier draft of this document, which
claimed `If-Match`. **Drive v3 has no such thing on `files.update`** — there is
no way to say "only if it is still the version I read". What it has is a
`version` that increases on every change, and revision history.

So the Drive store checks the version before writing and again afterwards, and
reports a change that appeared in between as a conflict. That catches a
clobber rather than preventing it, and what was overwritten is still in
Drive's own revisions. Weaker than S3; far stronger than a share that notices
nothing. The difference is visible in the app rather than hidden, because
pretending otherwise is how people lose data they were told was safe.

The NAS row is the awkward one. A network share cannot promise that two
machines writing at the same moment will not interleave, so the app **refuses
to make a NAS the authoritative store** unless you say, explicitly, that you
accept the risk. It is an excellent fallback and a poor source of truth.

## How several stores stay in sync

This is the part that has to be right, so it is stated as rules.

**One store is authoritative. Only that store is ever written.**

The others are mirrors. Every time a change lands on the authoritative store,
it is copied to each mirror. They are behind by at most one write, and they are
never *ahead*, so the question "which of these is correct?" always has the same
answer.

**Edits made while it is unreachable wait on this computer.**

They go into a durable outbox — on disk before you are told the edit worked, so
closing the laptop mid-edit loses nothing. When the store comes back, they are
replayed onto whatever it holds by then and written under a conditional write.

**Edits are never diverted to a fallback.**

This is the rule that makes the guarantee possible, and it is the one that
might not match what you pictured, so here is the reasoning.

If the app wrote to the fallback whenever the primary was down, then the
primary and the fallback would both hold edits the other does not. Reconciling
them means merging two whole documents, and there is no general answer to
"these two versions of the same account both changed — which is right?" Any
app that claims otherwise is either guessing or quietly dropping one side.

So the fallback is what you **read** from when the primary is unreachable, and
what you **promote** if the primary is gone for good. It is not a second place
to write. What you get in exchange is the thing you actually asked for: several
stores that are always in step, and conflicts that cannot happen rather than
conflicts that are usually handled.

**Two devices on one store is the normal case, and it is safe.**

Both read, both write conditionally. If the second one's write is based on a
stale read the service refuses it, the app re-reads, replays that edit onto the
newer document, and writes again. Nothing is lost and nobody is asked anything.

**What is left is rare and is surfaced, not guessed.**

Two devices, both offline, both editing the same record, both reconnecting —
the later edit wins the field and the earlier one is in the shared history and
in the store's own version history. The app says so rather than pretending it did
not happen.

## The change history, shared between machines

Every edit is recorded in a history entry naming the machine that made it (the
"Machine name" in settings, never the computer's own name). Each install keeps
its entries in `audit.json` in its data directory, and also publishes them to
the source of truth so every other machine's History tab can show them.

This used to be local on purpose, on the grounds that syncing it meant merging
two append-only logs. It no longer has to, because the log is split so that
**every object has exactly one writer**:

- Each install gets a random **install id** once, kept in its config. It is
  not the machine name: names are chosen by people and need not be unique.
- An install publishes only its own object, beside the ledger:
  `<ledger folder>/history/<install id>.json`. On the shared S3 bucket that is
  `ledger/history/…`, which a sync policy scoped to the ledger already
  covers (Get/Put under `ledger/*`, List scoped to `ledger/`). No new
  credentials or policy were needed.
- The object is `{"v":1,"install":"…","entries":[AuditEntry…]}`, oldest first.
  Entries carry `id`, `at`, `actor` (the machine name) and the diff.
- Readers list the folder, read every object, union by entry `id`, sort by
  time and show the result. There is nothing to merge, because nobody else
  writes your object.

**The rules that still hold:**

- **Only the primary.** History is published to the source of truth and
  nowhere else. Mirrors copy the ledger document only; they never receive
  history, because a mirror is not a second writer.
- **History never blocks an edit.** The edit is queued and flushed first; the
  history line is appended locally afterwards, and publishing it is a separate
  best-effort step whose failure is logged, not raised.
- **Writes stay conditional.** An install's object is written with `If-Match`
  (or create-only when absent). If two windows of the same install publish at
  once, the loser re-reads, unions and writes again, so neither entry is lost.
  On a file store the compare-and-write happens under an operating-system lock
  on `<file>.lock`, so this holds across separate processes too, not just
  within one. A NAS share whose locks fail falls back to its usual best effort.

**When the source of truth is unreachable**, the local `audit.json` is the
queue: anything in it that the published object lacks is pushed on the next
sync that reaches the primary (every Sync, every app load, and after each
edit). Entries dedupe by id, so publishing again never duplicates anything,
and history recorded before this feature existed is published on the first
sync.

**A damaged or missing object** from another machine is skipped and named in
the History tab; the rest still shows. A damaged object of this install's own
is replaced from the local log, since this install is its only writer.

**Retention** is the same as the local log, applied to each published object
and to the merged view: sixty days, and at most five hundred entries.

**Stores without a shelf.** Local, NAS and S3 sources of truth support shared
history. Google Drive does not yet; with Drive as the source of truth, history
stays on each machine and the History tab says so.

**Importing the plugin's history.** A one-time command brings the retired
plugin's log (`~/.local/state/kairos.home-ledger/audit.json`, read only) into
this install's history, then publishes it like any other entry:

```sh
home-ledger import-plugin-history --machine "Laptop" --dry-run   # count and date range
home-ledger import-plugin-history --machine "Laptop"
```

`--dry-run` only reads: it writes no config, history or data directory, even
on a machine the app has never run on. Plugin entries name no machine, so the
name is typed at import. Ids and times
are kept, so running it again adds nothing. It refuses, rather than trims,
when an entry is already outside the sixty-day window or the import would pass
five hundred entries. Close the app first: the command writes this install's
history file.

## Promotion, and how the app knows which copy is newer

Every write stamps the document with a **lineage**: a generation number and the
id of the store that wrote it. A mirror is a fast-forward of the primary, so
its lineage is an ancestor.

That makes promotion safe. When you promote a mirror, the app can tell whether
it is genuinely behind (fast-forward, no loss), identical, or — if something
has gone wrong — genuinely forked, in which case it stops and shows you both
rather than picking one.

Without lineage, promotion is "hope this one was current". With it, promotion
is a decision the app can check.

## What is built

| | |
|---|---|
| This computer | done |
| Amazon S3, and S3-compatible | done, and exercised against a real bucket |
| Lineage, so promotion can be checked | done |
| Config file and OS keychain | done |
| Google Drive, including sign-in | built, **not yet exercised against a real account** |
| Google Cloud Storage, Azure | not yet |
| The setup and Settings screens | done |

The S3 store is verified the only way that counts: against a live bucket it
reads the object and its version, and a write carrying a deliberately stale
version is refused. That second check exercises the whole conditional-write
path and provably stores nothing, because the precondition fails.

`cargo run -p ledger-store --example s3-check -- <bucket> <key> <region>`
runs it, with `AWS_PROFILE` naming a profile in `~/.aws/credentials`. It
works against any S3-compatible service by passing an endpoint.

## Credentials

Connection details are yours, and they never go near the ledger document.

- **The OS keychain holds secrets.** Windows Credential Manager, and the Secret
  Service (GNOME Keyring, KWallet) on Linux. Access keys, refresh tokens and
  connection strings live there, not in a config file.
- **Non-secret settings live in a config file**: which stores, in what order,
  bucket names, regions, endpoints, paths.
- **Google Drive and GCS use OAuth**, so the app never sees a password and you
  can revoke it from your Google account. That means a browser round-trip and a
  stored refresh token.
- **S3, GCS-with-a-key, and Azure use keys you paste in.** The app should say
  what permissions it needs, and ask for no more: read and write one object.
- **S3 can sign with an AWS profile instead.** Name a profile in
  `~/.aws/credentials` and the key is read from there when a request is signed,
  so a machine that already has one keeps a single copy of the secret rather
  than two. The file is refused unless it is private to its owner. The profile
  name is not a secret and is written into the config file with the bucket.
- **Nothing is sent anywhere else.** The app talks to the stores you configure
  and to nothing besides.

Where no keychain is available — some headless Linux setups — the app says so
and asks whether to use an encrypted file instead, rather than silently writing
a key in the clear.

## Changing your mind

Nothing chosen at first run is permanent. Stores get added, removed, re-keyed
and re-ordered, and that has to be an ordinary thing to do rather than a
migration you dread — so each change is a named operation the app knows how to
do safely, not an edit to a config file that the sync layer then has to cope
with.

Two rules cover all of them:

- **No reconfiguration can lose data.** Anything that moves or replaces a
  document is previewed first, the same way import is, and says what will
  happen before it happens.
- **The outbox must be empty first.** Queued edits were made against a
  particular store; changing which store is authoritative while they wait would
  strand them. The app syncs first, or tells you what is waiting.

| What you do | What happens to the data |
|---|---|
| **Add a mirror** | The current document is copied to it. If it already holds a ledger, you are asked before it is overwritten. |
| **Add your first remote**, coming from local-only | Your local ledger is uploaded and becomes the source of truth. If the remote already holds one, you choose which survives — neither is merged silently. |
| **Promote a mirror** to authoritative | Lineage is checked first: behind means fast-forward, identical means just a role swap, forked means stop and show you both. Roles swap; no data moves. |
| **Re-order the fallbacks** | Nothing moves. Order only decides who is consulted first when the primary is unreachable. |
| **Re-key a store** | Nothing moves. Credentials are replaced in the keychain and the store is re-checked. |
| **Remove a mirror** | It is forgotten. The data is left where it is — deleting from your account is not the app's decision to make. |
| **Remove the authoritative store** | Refused until another is promoted. There is always exactly one source of truth. |
| **Go back to local-only** | The current document is pulled down and kept locally; the remotes are forgotten, not emptied. |

The pattern is that the app never silently merges and never silently deletes.
Where two documents genuinely disagree it stops and shows you, and where data
lives in an account of yours it stays there until you remove it yourself.

## Decided

**No client-side encryption.** The store holds the document as written. S3,
GCS and Azure encrypt at rest by default, the transport is TLS, and your
account is already the security boundary. The alternative costs a passphrase
that cannot be lost without losing the data, and takes away the ability to read
or repair your own ledger by hand — which for a file you own outright is a real
loss, not a theoretical one.

**History is the provider's versioning.** Setup offers to turn on object
versioning where the provider has it, and that is the backup: every write keeps
the previous document, so a bad edit or a corrupt write is recoverable without
a backup job anybody has to remember. The app does not keep a second copy of
history of its own.

The gap is honest and should be said out loud in setup: **local-only and a NAS
have no versioning**, so those configurations have no undo beyond the audit
log. Someone choosing local-only should be told that plainly.

**One source of truth, any number of backups.** An ordered list. The first is
authoritative and the rest are consulted in order when it cannot be reached, so
"S3 for real, Drive as well, and the NAS because it is there" needs no special
cases.

**What it is called.** On screen: the **source of truth**, and **backups**. In
the code they are `primary` and `mirrors`, which is what they technically are;
the mapping is deliberate, not drift. "Backups" is the honest word for the UI
because it sets the right expectation — they are copies you could fall back to,
not places being written to independently.
