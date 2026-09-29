---
id: home-ledger.a-sign-in-code-is-bound-to-this-app
project: home-ledger
category: security
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# An OAuth code is only accepted from a sign-in this app started

## Claim
The loopback listener binds 127.0.0.1 only, refuses a redirect whose `state`
does not match the one generated for that attempt, and never sends the PKCE
verifier to the authorization endpoint.

## Why
The redirect lands on a plain local port that anything on the machine can
reach. Without the state check, a page someone happens to visit could aim a
code of its own at that port and have the app silently adopt a different
account. Without PKCE, a code intercepted on the way back would be enough to
exchange for tokens.

## Check
```bash
cargo test -p ledger-store a_code_carrying_the_wrong_state_is_refused
cargo test -p ledger-store the_listener_binds_loopback_only
cargo test -p ledger-store a_challenge_is_the_hash_of_the_verifier_not_the_verifier
cargo test -p ledger-store the_authorize_url_asks_for_offline_access_and_carries_the_challenge
```

## Depends On
[[core-tests-pass]]
