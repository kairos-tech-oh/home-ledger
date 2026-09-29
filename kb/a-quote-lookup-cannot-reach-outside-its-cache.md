---
id: home-ledger.a-quote-lookup-cannot-reach-outside-its-cache
project: home-ledger
category: security
severity: high
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# A ticker is validated before it becomes a URL or a filename

## Claim
`clean_ticker` accepts only an uppercase symbol of at most sixteen characters
starting alphanumeric, refuses an over-long one rather than truncating it, and
every accepted symbol is percent-encoded into the request and has its colon
swapped before it becomes a cache filename.

## Why
The symbol comes out of the ledger and goes into both an outbound URL and a
path under the data directory. Truncating an over-long one would quietly look
up a different company; accepting a slash or a leading dot would let a holding's
name decide which file is written.

## Check
```bash
cargo test -p home-ledger a_symbol_that_is_not_one_is_refused_rather_than_looked_up
cargo test -p home-ledger a_cache_name_cannot_climb_out_of_its_directory
cargo test -p home-ledger a_share_class_is_written_the_way_yahoo_writes_it
```

## Depends On
[[core-tests-pass]]
