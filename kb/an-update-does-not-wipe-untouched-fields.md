---
id: home-ledger.an-update-does-not-wipe-untouched-fields
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# An update keeps the fields it was not given

## Claim
`Op::Set` merges over the stored record, so a field the caller omits keeps
its stored value; sending a field explicitly is what clears it.

## Why
Replacing the whole record loses anything edited from a different screen —
in the app this replaces, saving a Roth account from its editor erased the
contributions set in the Roth dialog. Every editor sends a partial record,
so this is the default path, not an edge case.

## Check
```bash
cargo test -p ledger-writer a_field_left_out_of_an_update_keeps_what_was_stored
cargo test -p ledger-writer a_field_can_be_cleared_by_sending_it_explicitly
cargo test -p ledger-writer an_update_keeps_the_id_rather_than_adding_a_second_record
```

## Depends On
[[core-tests-pass]]
