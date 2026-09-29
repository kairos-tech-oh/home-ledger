# Adding a place a ledger can live

A store is a versioned slot holding one JSON document. That is the whole
abstraction, and it is deliberately small: four methods, no filesystem, no
listing, no directories. If your service can hold a few hundred kilobytes and
tell you whether it changed, it can back this app.

## The one rule that matters

**Declare what your service can actually promise, not what you wish it could.**

Everything else here is mechanics. This is the part that protects people's
data, and it is the part that is easy to get wrong in a way nobody notices
until two machines are writing at once.

```rust
fn capabilities(&self) -> Capabilities {
    Capabilities { cas: Cas::Native, shared: true, max_bytes: 32 * 1024 * 1024 }
}
```

`Cas` says whether a *stale* write can be refused — a write based on a version
that is no longer current:

| Level | Means | Example |
|---|---|---|
| `Native` | The service itself rejects it | S3 `If-Match` |
| `CheckedAfterWrite` | It cannot be refused, but a clobber is detected immediately and the old version survives | Google Drive |
| `LocalLock` | Safe between processes on one machine, meaningless across machines | a local file |
| `BestEffort` | Nothing notices | an SMB share |
| `Exclusive` | Nothing else can write here at all | — |

Do not copy `Native` from the S3 store because it compiles. The engine uses
this: a store that cannot lock is refused as the source of truth unless the
person is told and says yes anyway, and the UI shows the caveat next to the
store's name. Getting it wrong tells someone their data is safe from something
it is not safe from.

When in doubt, pick the weaker level and say why in a comment. Drive is a
worked example — see the module comment in `gdrive.rs`, which exists because an
earlier draft of `STORAGE.md` claimed an `If-Match` that Drive does not have.

## The trait

```rust
#[async_trait]
impl Store for MyStore {
    fn id(&self) -> &StoreId;
    fn kind(&self) -> StoreKind;
    fn capabilities(&self) -> Capabilities;

    async fn health(&self) -> Health;
    async fn load(&self) -> Result<Option<Snapshot>, StoreError>;
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError>;
}
```

**`health`** must be cheap and must not download the document. It answers one
question: can these credentials reach this thing. A missing document is
`Reachable` — that is a first run, not a fault.

**`load`** returns `None` when nothing is stored yet. Do not invent an empty
document: "unreachable" and "empty" have to stay different answers, or an
unmounted share reads as a fresh start and the app offers to replace real data.

**`save`** honours `expect`:

- `Expect::Absent` — create only if nothing is there
- `Expect::Version(v)` — replace only if `v` is still current
- `Expect::Force` — overwrite regardless; only ever used after a person has
  been shown the conflict and picked a side

Return `StoreError::Conflict` when a precondition fails. The engine treats that
as normal: it re-reads, replays the queued edits onto the newer document, and
writes again. It is not an error anyone sees.

## Versions

A `Version` is any string your service can compare for equality — an ETag, a
generation number, a revision id, a content hash. The engine never parses it.
It only ever asks "is this the same one I read?"

If your service offers nothing, hash the bytes. That is what the local store
does, and it means a file edited outside the app is still caught.

## Telling failures apart

This decides whether the app retries forever or tells someone their key is
wrong, so it is worth care:

```rust
StoreError::Unreachable(..)  // transient: edits wait in the outbox and go later
StoreError::Denied(..)       // not transient: credentials, permissions, no such bucket
StoreError::Conflict         // someone wrote first; the engine replays
StoreError::TooLarge { .. }  // refuse before sending
StoreError::Corrupt(..)      // what came back is not readable
```

Rate limiting and 5xx are `Unreachable`. A refused key is `Denied` — retrying
it forever helps nobody and hides the real problem.

## Wiring it up

1. `crates/ledger-store/src/<yours>.rs`, exported from `lib.rs`.
2. Add a `StoreKind` variant.
3. Add a `Settings` variant in `ledger-config` — **non-secret fields only**.
   Bucket names, regions, endpoints, paths. There is no field for a secret and
   there must not be one.
4. If it needs a new kind of credential, add a `Secret` variant. Secrets live
   in the OS keychain, filed under the store's id.
5. Handle both in `build_store`.
6. Add it to `ui/src/storage.ts` — the `kinds` table carries the name and the
   sentence shown to someone choosing.

## Testing

Offline tests are expected and they catch most of it: URL and path
construction, error mapping, version comparison, anything with escaping. Look
at `s3.rs` and `gdrive.rs`.

What offline tests cannot do is tell you the service agrees with you. Both
existing stores ship a live check that needs real credentials and changes
nothing:

```
cargo run -p ledger-store --example s3-check -- <bucket> <key> <region> [endpoint]
```

It reads the document, then attempts a write carrying a deliberately stale
version, which must be refused. That second half is the valuable one — it
exercises the whole conditional-write path and provably stores nothing,
because the precondition fails. **Write the equivalent for your store, and say
in the pull request whether you ran it and what it printed.**

A store that has never been pointed at the real service is not finished, and
should say so in `STORAGE.md` rather than sitting in the list looking equal to
the ones that have.

## Why the bar is here

Two bugs in this project's short history looked completely correct on review:
a SigV4 constant written from memory, and a balance-sheet rollup that was out
by six figures on real data. Both were found by checking against something
independent rather than by reading the code again.

Storage is worse than either, because the failure is silent and arrives later,
on someone else's machine, as a number that is quietly wrong.
