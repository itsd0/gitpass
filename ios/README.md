# gitpass for iOS

The iPhone companion app for gitpass. Dark theme, orange accent
(`#F28C28`, the desktop accent), the desktop's nine item categories,
and English / Burmese / Thai UI. All cryptography runs in
`gitpass-core` (Rust) through UniFFI — Swift never implements it.

## What works in this version

- Unlock with master password + Secret Key (vault format v5, Argon2id
  256 MiB, the same `Session::unlock` the desktop uses).
- Face ID unlock: unlock credentials are stored in the Keychain
  under a biometry access control. Device-only (the simulator has
  no biometrics).
- Vault list, item list + search, item detail with copy and
  auto-clearing clipboard (30 s, only if the clipboard still holds
  what gitpass copied), add / edit / delete (soft-delete) / restore /
  favorite / archive.
- TOTP codes from the Rust `totp` module, with a live code and a
  refresh button (the code window is shown next to the code).
- Password generator from `gitpass-core` (letters, digits, symbols).
- All nine categories: login, credit card, secure note, identity,
  API credential, document, SSH key, Wi-Fi, passkey. Non-login items
  store their fields as v5 sections; login items keep the legacy
  login fields, exactly as desktop and the CLI read them.
- Two-way GitHub sync on the shared core engine (#456): the same
  `gitpass-core` sync the desktop, CLI and Android use, through the
  bridge. Repository, branch and a fine-grained PAT are entered by
  you (token in the Keychain). See "Sync" below.
- AutoFill credential provider extension for passwords, sharing the
  vault files through the app group. The extension asks for the
  password itself — it does not reuse the app's Keychain credentials.

## What is NOT in this version (device-only work)

- Passkey registration / assertion in AutoFill (needs associated
  domains, an RP ID and a real device). Password filling works;
  passkey items are listed in the app with their stored fields.
- Face ID unlock on a real device (simulator has no biometrics).
- End-to-end sync against a live vault repository or on a physical
  device (CI cannot do this without a token and a real repo; the
  engine, first pull, the Swift transport, settings and scheduling
  are tested against an in-process fake GitHub; the live path is not).
- Background sync while locked (the master password is never stored
  for it) and an offline indicator (a network failure shows as a
  sync error).
- Vault renaming (the desktop app does not offer it either).

## Sync

All sync logic runs in `gitpass-core` through the bridge; Swift only
performs the HTTPS round trip, stores settings, and decides when to sync.

- **Engine.** `Vault.syncGithub` runs core `sync_with_remote` over the
  core GitHub Git Data API client: three-way merge against the last
  synced base, field-level merges (equal clock + different value keeps
  local and is reported), deletes in both directions, delete-vs-edit
  keeps the edit, vault create/delete. It refuses a rolled-back remote
  (epoch below the last synced one), files outside the vault allowlist,
  non-regular files, and a remote password/KDF change, without changing
  local data. Pushes are one commit with a non-forced, compare-and-swap
  ref update (retried on a concurrent push). An empty repository is
  initialized from this device.
- **Transport.** `GitHubHTTPSTransport` (Swift) implements the bridge's
  `HttpTransport`: fixed origin `https://api.github.com`, `/repos/...`
  paths only, an ephemeral `URLSession` (no cache, cookies or credential
  storage), redirects never followed, bounded response size, timeouts and
  cancellation. The token is added only as the `Authorization` header
  (it is never part of the request record from Rust, a URL, a log or an
  error). Errors are fixed text from core.
- **Settings.** Owner, repository and branch (no default repository) are
  stored in `UserDefaults`; the fine-grained PAT (`github_pat_…`, limit it
  to the vault repository with Contents: Read and write) is stored in the
  Keychain (`WhenUnlockedThisDeviceOnly`) and never shown again. A token
  saved by an earlier version that is not a fine-grained PAT is deleted.
- **When.** On unlock, a few seconds after an edit, every 15 minutes while
  unlocked, and on **Sync now**. Locking cancels a running sync. While a
  sync runs (it holds the Rust session), item actions show "Sync in
  progress" instead of blocking the UI.
- **Status.** Settings shows last sync / syncing / the error, and the sync
  screen lists the last sync's conflicts (ids and field names only).
- **First pull (new device).** With no vault on the device, the sync
  screen pulls the branch head with `pullGithubSnapshot` (core transport;
  allowlist, tree and blob ids checked) into a private staging directory,
  authenticates the whole snapshot with `validateFirstPullSnapshot` (Master
  Password + Secret Key), and only then moves it into place and unlocks.
  It never replaces an existing vault and never writes to the remote.

Local-only files (`.gitpass-state.json`, the local device registry file,
backups) are never uploaded (core's single allowlist).

## Build (local, on a Mac)

Prerequisites: Xcode, Rust (`rustup`), XcodeGen (`brew install xcodegen`).

```bash
cd ios
./scripts/build-xcframework.sh   # builds GitpassCore.xcframework + Swift bindings
xcodegen generate                # creates Gitpass.xcodeproj (not committed)
open Gitpass.xcodeproj
```

In Xcode, pick your personal team under Signing & Capabilities for
both the app and the AutoFill extension, then run on your iPhone.
The bundle IDs are `dev.gitpass.ios` (app) and
`dev.gitpass.ios.autofill` (extension); no team is committed in
`project.yml` on purpose.

To enable AutoFill on the phone: Settings → General → AutoFill &
Passwords → enable gitpass, then use it from a login screen.

To connect sync: in the app, Settings → GitHub sync (or "Sync from
GitHub first" on the unlock screen), enter the owner, repository and
branch of your vault repository and a fine-grained PAT limited to it,
then Save. On a new device with no local vault, enter your master
password and Secret Key under "First pull" to pull and unlock.

## CI

`.github/workflows/ios.yml` (macOS) runs the Rust FFI tests, builds
the XCFramework, generates the project with XcodeGen, builds for
the iOS Simulator unsigned, runs the XCTest suite on the simulator,
builds an unsigned archive, and uploads it as an artifact. It never
signs, never needs a team, and never touches a vault repository.

## Layout

```
ios/
  Sources/GitpassApp/     SwiftUI app
  Sources/AutoFill/       ASCredentialProviderViewController extension
  Tests/                  XCTest: FFI round-trip, Keychain, sync on a fake GitHub
  scripts/                XCFramework build script
  Generated/              generated Swift bindings (build output, gitignored)
  Frameworks/             GitpassCore.xcframework (build output, gitignored)
```

## Security notes

- No secret, token or key is ever logged or committed. The only
  persisted secrets are the Keychain items described above.
- The app and the extension do not share Keychain items. The
  extension prompts for the password itself.
- If you lose the master password and the Secret Key, there is no
  recovery — the same as desktop.
- This app has not been independently audited, same as the rest of
  gitpass. Do not trust it with real secrets until it has been
  reviewed.
