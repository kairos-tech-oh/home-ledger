# Security policy

Home Ledger holds a household's finances and the credentials that reach where
they are stored, so a security problem here is taken seriously even though the
project is small and early.

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Report it privately through GitHub instead: on the repository's **Security**
tab, choose **Report a vulnerability**. That opens a private advisory that only
the maintainers can see, where the problem can be discussed and fixed before
anything is made public.

A useful report says:

- what an attacker could do, and what they need first (local access, a
  malicious ledger file, a hostile network, a compromised storage provider…)
- the version or commit, and the platform
- the steps to reproduce it, or a proof of concept

This is maintained by a very small team, so responses are best effort. You
should hear back within two weeks. Once a fix is released, the advisory is
published and you are credited, unless you would rather not be.

## Supported versions

Only the latest release receives security fixes. Before 1.0 there are no
maintained older branches, so the fix for a problem is to upgrade.

## What is in scope

Anything that lets someone read, change or destroy a ledger, or its
credentials, that they should not be able to. For example:

- **Credentials leaking:** a secret written to `config.json`, a log, the
  audit history, the ledger document, or anywhere else outside the OS
  keychain or the AWS credentials file it was read from.
- **Storage guarantees failing:** a write that lands on a mirror instead of
  the primary, a compare-and-swap that can be bypassed so a newer copy is
  overwritten, or an unreachable store read as an empty one.
- **Hostile input:** a crafted ledger file, network response (S3, Google
  Drive, Finnhub, Yahoo) or history object that crashes the app, corrupts the
  ledger, or runs code.
- **The desktop shell:** the interface reaching commands, files or network
  destinations it should not, or content breaking out of the webview's
  content security policy.
- **Sign-in:** the Google Drive OAuth flow accepting a code meant for another
  app or session.

## What is not in scope

- A storage account the user configured insecurely themselves, such as a
  public S3 bucket or an over-broad IAM policy.
- Someone who already controls the user's account on the machine. They can
  read the keychain and the files as that user; Home Ledger cannot defend
  against its own user.
- The ledger being readable on disk. It is stored as plain JSON by design at
  present, protected by the operating system's file permissions and, in the
  cloud, by the provider's own access control and encryption.
- Problems in third-party services themselves (AWS, Google, Finnhub, Yahoo).

## How the app is built to limit damage

- Secrets live in the OS keychain, or are read at request time from
  `~/.aws/credentials`, and never enter the stored configuration; a test and a
  kb claim hold that in place.
- Only the primary store is ever written, and every write is conditional, so a
  stale copy cannot overwrite a newer one.
- Every derived figure is computed in Rust; the interface only renders it.
- No personal identity is written into the ledger or its history; machines are
  named by the user, never by hostname.
