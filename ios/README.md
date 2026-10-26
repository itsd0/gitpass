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
- GitHub sync over HTTPS (device flow, token in the Keychain) with
  the desktop's field-level merge rules (higher clock wins; equal
  clock + different value keeps local). See "Sync scope" below.
- AutoFill credential provider extension for passwords, sharing the
  vault files through the app group. The extension asks for the
  password itself — it does not reuse the app's Keychain credentials.

## What is NOT in this version (device-only work)

- Passkey registration / assertion in AutoFill (needs associated
  domains, an RP ID and a real device). Password filling works;
  passkey items are listed in the app with their stored fields.
- Face ID unlock on a real device (simulator has no biometrics).
- End-to-end sync against a live vault repository (CI cannot do
  this without a token and a real repo; the sync engine, device
  flow and merge logic are unit-tested, the live path is not).
- Vault creation / deletion through sync. See "Sync scope".
- Vault renaming (the desktop app does not offer it either).

## Sync scope

Sync uses the GitHub REST / Git Data API, not the `git` CLI (iOS
has no git binary). One sync:

1. Fetches the remote tree for the configured repository.
2. Downloads it to a staging directory and opens it with the same
   credentials (read-only comparison; staging is deleted after).
3. For vaults present on both sides, merges items: remote-only
   items are added locally, differing items are merged with
   `gitpass-core`'s `merge_records`, local-only items are uploaded.
4. Uploads the resulting local tree as one commit whose parent is
   the fetched remote HEAD, without forcing the ref update — if the
   remote moved in between, the update fails and you retry instead
   of overwriting someone else's push.

A vault that exists on only one side aborts the sync without
changing local data (the desktop `sync --merge` rule for
local-only vaults, extended to remote-only vaults, because the
encrypted manifest cannot be rebuilt on iOS for a whole new vault).
A first pull on a device with no local vault copies the whole
remote tree, after which you unlock normally.

Local-only files (`.gitpass-state.json`, the local device registry
file, backups) are never uploaded.

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

To connect sync: in the app, Settings → GitHub sync, enter your
OAuth App client ID and the vault repository (default
`itsd0/gitpass-vault`), follow the device-code flow, then Sync now.
For a new device with no local vault, use "Pull remote vault first"
before unlocking.

## CI

`.github/workflows/ios.yml` (macOS) runs the Rust FFI tests, builds
the XCFramework, generates the project with XcodeGen, builds for
the iOS Simulator unsigned, runs the XCTest suite on the simulator,
builds an unsigned archive, and uploads it as an artifact. It never
signs, never needs a team, and never touches a vault repository.

## Layout

```
ios/
  rust/gitpass-ios-ffi/   UniFFI crate (thin wrapper over gitpass-core)
  Sources/GitpassApp/     SwiftUI app
  Sources/AutoFill/       ASCredentialProviderViewController extension
  Tests/                  XCTest: FFI round-trip, Keychain, sync requests
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
