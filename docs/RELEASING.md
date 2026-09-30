# Releasing

How a version goes from `main` to every installed copy, and how Windows code
signing is switched on once there is a certificate.

## What a release is

Pushing a tag `vX.Y.Z` runs `.github/workflows/build.yml`:

1. `check` runs formatting, lints and every test.
2. `bundle` builds the Windows (NSIS, MSI) and Linux (deb, rpm, AppImage)
   installers. It runs only for a tag or a manual run from the Actions tab,
   and keeps its files for two days: the release is their permanent home.
   With the update key available it also writes a `.sig` beside each
   installer the updater can use.
3. `release` writes `latest.json` from those signatures and creates a **draft**
   GitHub release with everything attached.

A draft is invisible to installed copies. They read
`https://github.com/kairos-tech-oh/home-ledger/releases/latest/download/latest.json`,
which only resolves once the release is **published**. Publishing the draft
Actions made — not a new release made by hand from the tag — is what ships it.

## Cutting one

1. Set the version in all four places, which must agree:
   `Cargo.toml` (`workspace.package.version`), `src-tauri/tauri.conf.json`,
   `package.json` and `ui/package.json` (and their lockfiles).
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`
   and start a new empty `Unreleased` above it. That section becomes the notes
   the app shows under "What's new".
3. Merge to `main`, tag the merge commit `vX.Y.Z`, push the tag.
4. When the workflow is green, open the draft release, read it, publish it.

Each copy checks on launch and offers the update; Settings → About checks on
demand. On Windows the installer runs passively and the app restarts itself.

## The update key

Updates are signed with a minisign key. The public half is in
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) and is compiled into
every build; the private half signs releases in CI. An installed copy refuses
any update the private key did not sign.

The private key and its password are held by the maintainer, outside the
repository, and are in the repository's Actions secrets as
`TAURI_SIGNING_PRIVATE_KEY` (the contents of the key file) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

**If the private key is lost, installed copies can never be updated
automatically again**: a new key means a new public key, which only a manually
installed build carries. Keep it in a password manager as well as in the
secrets.

A build without the key — a local `npm run build`, a pull request from a fork
— simply produces no update artifacts. Only a tagged release insists on them.

## Windows code signing

Unsigned installers work, but Windows SmartScreen warns that the publisher is
unknown. Signing removes the "unknown publisher" and builds SmartScreen
reputation over time. The certificate names **Kairos Technologies** as the
publisher, validated as an organisation, so no person's name appears.

The workflow is wired for [Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/)
and switched off. To switch it on:

1. In Azure, create a Trusted Signing account, complete organisation identity
   validation for Kairos Technologies, and create a *public trust* certificate
   profile.
2. Create an app registration (service principal) with the *Trusted Signing
   Certificate Profile Signer* role on that account.
3. In the repository's Actions settings add:
   - variables: `AZURE_SIGNING_ENDPOINT` (the account's region endpoint, such as
     `https://eus.codesigning.azure.net`), `AZURE_SIGNING_ACCOUNT`,
     `AZURE_SIGNING_PROFILE`, and `WINDOWS_SIGNING` = `trusted-signing`;
   - secrets: `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`.

`tools/release/ci-config.mjs` then adds a `signCommand` and the Windows job
installs `trusted-signing-cli` to run it. With `WINDOWS_SIGNING` set but a
setting missing, the build fails rather than shipping unsigned.

Linux packages are not Authenticode-signed; their integrity for updates comes
from the update key, as on Windows.
