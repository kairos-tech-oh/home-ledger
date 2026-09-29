---
id: home-ledger.a-quote-never-invents-a-price
project: home-ledger
category: correctness
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.an-update-does-not-wipe-untouched-fields]
---

# A refresh writes prices it was given and flags the rest

## Claim
`Op::PriceHoldings` never touches a holding with `fixedPrice` set, whatever was
sent for it; a symbol that did not answer keeps its last price and is marked
stale rather than cleared; and a sweep that matched no holding is refused
rather than written as an empty history entry.

## Why
A quote feed is the one place the app takes a figure from outside. A hand-set
price is a deliberate statement about something the feed cannot know — a gold
coin, a private holding — and must survive a refresh. Clearing a price on a
failed lookup would turn a network blip into an apparent loss of the whole
position, since an unpriced holding falls back to cost.

Finnhub answers an unknown symbol with zeroes rather than an error, so the
fetch treats a flat zero as "never heard of it" rather than "worth nothing".

## Check
```bash
cargo test -p ledger-writer a_holding_priced_by_hand_is_never_overwritten_by_a_quote
cargo test -p ledger-writer a_symbol_that_did_not_answer_keeps_its_last_price_and_is_flagged
cargo test -p ledger-writer a_sweep_that_touched_nothing_is_refused_rather_than_logged
cargo test -p home-ledger a_key_that_is_not_one_is_refused_before_it_becomes_a_header
```

## Depends On
[[core-tests-pass]], [[an-update-does-not-wipe-untouched-fields]]
