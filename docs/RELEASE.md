# Releases, cross-platform builds and auto-update

## Workflows
* `.github/workflows/ci.yml` — on push/PR/manual: `cargo test --workspace`, desktop + extension `npm test`,
  and `cargo check` of the Tauri app on Linux, macOS and Windows.
* `.github/workflows/release.yml` — on tag `v*`: builds the desktop app for macOS arm64 + x64 (app, dmg;
  **ad-hoc signed**, not notarized), Linux x64 (AppImage, deb) and Windows x64 (msi, nsis), creates a GitHub
  Release on **`itsd0/gitpass`** (this repo, same tag) with the installers, updater artifacts (`*.sig`) and `latest.json`;
  the `publish` job then publishes it as Latest (no mirror to `itsd0/gitpass-releases` any more). Also builds the CLI and
  `gitpass-host` binaries as workflow artifacts. `workflow_dispatch` builds the same things as artifacts only.
  The `android` job builds a signed release APK + AAB (see [Android release signing](#android-release-signing));
  on a tag, `publish` attaches them and a `.sha256` file to the release.

## Platform support matrix
| Feature | macOS | Linux | Windows |
|---|---|---|---|
| Biometric unlock | Touch ID + Keychain | not supported (NoBiometrics) | **stub** (NoBiometrics; Windows Hello / Credential Manager not implemented) |
| App ⇄ extension agent | UNIX socket `~/Library/Application Support/gitpass/agent.sock` | UNIX socket `$XDG_RUNTIME_DIR/gitpass/agent.sock` | **stub**: named pipe `\\.\pipe\gitpass-agent` planned; app logs "not implemented", host runs standalone |
| Native host install | `~/Library/Application Support/<browser>/NativeMessagingHosts` | `~/.config/{google-chrome,chromium,BraveSoftware/Brave-Browser}/NativeMessagingHosts`, `~/.mozilla/native-messaging-hosts` | manifest in `%LOCALAPPDATA%\gitpass\<browser>\`, registered under `HKCU\Software\{Google\Chrome,Chromium,BraveSoftware\Brave-Browser,Mozilla}\NativeMessagingHosts\com.gitpass.host` |

## Auto-update (tauri-plugin-updater)
* Endpoint: `https://github.com/itsd0/gitpass/releases/latest/download/latest.json`; public key in
  `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
* Releases are hosted on `itsd0/gitpass` (#292). The `itsd0/gitpass-releases` mirror was dropped: apps older than
  0.6.7 only check `gitpass-releases` and must be updated manually by installing the DMG from `itsd0/gitpass`.
* Private signing key: only on the release Mac at `~/.gitpass-release/updater.key` (0600) and in the
  repo Actions secret `TAURI_SIGNING_PRIVATE_KEY`. Losing it means existing installs can't verify future updates.
* **Password-protect the signing key.** New signing keys must be generated with a password (the Tauri
  signer prompts for one when generating). Store the key as the Actions secret `TAURI_SIGNING_PRIVATE_KEY`
  and the password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The release workflow passes both through to
  the build (see "Workflow changes for owner to commit" in the PR that introduced this — the workflow
  file itself is not edited by the GitHub App). If the password secret is unset or empty, signing
  behaves exactly as before, so an old unprotected key keeps working until it is replaced. No code
  change was needed for this: the Tauri signer already reads `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  from the environment at build time; the gap was that the workflow hardcoded it to `""`.
* **Rotating the signing keypair needs a transition release.** Clients verify updates with the public
  key embedded in `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`), so a new keypair
  cannot be switched in one step: first ship a release signed with the *old* key that contains the
  *new* public key, then switch the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  secrets to the new keypair. Clients that never install the transition release cannot verify
  anything signed with the new key — that is inherent to the updater's trust model, not a gitpass bug.
  After the transition release, retire and destroy the old private key.
* Settings → About → "Check for updates".
* Releases must be **published** (and not marked pre-release) for `releases/latest` to resolve; the `publish`
  job does this after both macOS builds succeed.
* Local `npm run tauri build` now also produces updater artifacts and therefore needs
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.gitpass-release/updater.key)"` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set to the key's password (empty only for a legacy unprotected key).
  Never put either value in a shell history file, a committed file, or chat — export them in the
  current shell only.

### Cutting a release
1. Bump the version in `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/package.json`, the Cargo manifests.
2. Add a `RELEASES` entry for the new version in `apps/desktop/src/whats-new/releases.ts` (3–6 highlights, en/my/th).
   Required: the desktop test `has highlights for the app version being released` fails CI when the bumped
   version has no entry, so the release PR cannot merge without it.
3. `git tag vX.Y.Z && git push origin vX.Y.Z` (on `itsd0/gitpass`).
4. Wait for `release.yml`: it publishes the release on `itsd0/gitpass` as Latest. Then set the polished notes on the `itsd0/gitpass` release (`gh release edit --notes-file`)
   and check that `releases/latest/download/latest.json` shows the new version.

### Writing release notes

Start from `.github/release-notes-template.md` and paste it into the draft
release body; do not reuse the old one-line body ("See CHANGELOG.md. …").
The template is the contract for what a release page contains:

- A short, human-written intro paragraph (what this release is about).
- Themed `##` sections — **Highlights**, **Desktop app**, **Browser
  extension**, **CLI**, **Security**, **Fixes**, **Docs & maintenance** —
  each bullet with a **bold lead-in** and a PR link
  (`([#N](https://github.com/itsd0/gitpass/pull/N))`). Delete any section
  with no entries; empty sections must not ship.
- A **What's Changed** list: on the draft release page, click "Generate
  release notes" and paste the result there. Generation uses
  `.github/release.yml` in this repo, since releases are created on `itsd0/gitpass`.
- **Install / Upgrade** notes: Apple Silicon (`aarch64` DMG) vs Intel (`x64`
  DMG), auto-update via Settings → About → Check for updates, any required
  user action (e.g. re-sharing, re-enabling Touch ID — omit the line when
  there is none), and the self-signed / not-notarized Gatekeeper note. The
  updater files (`*.app.tar.gz`, `.sig`, `latest.json`) are described as
  auto-update files the user does not need to download.
- A **Full Changelog** link:
  `https://github.com/itsd0/gitpass/compare/vPREV...vX.Y.Z` (tags live in
  `itsd0/gitpass`, so this link is correct whichever repo hosts the release
  page, per #292).
- For UI-facing releases, an optional Highlights screenshot or GIF.

`CHANGELOG.md` stays the source material for the themed sections (it already
uses themed sections with PR numbers), but it is not a substitute for the
release page: link the changelog sections, do not paste the whole file.

#### PR labels for release notes (manual convention for now)

`.github/release.yml` categorises PRs by label, so every PR merged into a
release should carry one conventional label (in addition to its `agent:*`
and `area:*` labels):

| PR title prefix | Label | Release-notes section |
|---|---|---|
| `feat` | `feat` (or existing `enhancement`) | Features |
| `fix` | `fix` (or existing `bug`) | Fixes |
| `perf` | `perf` | Performance |
| `security` | `security` | Security |
| `docs` | `docs` (or existing `documentation`) | Docs |
| `chore` | `chore` (or existing `dependencies`) | Maintenance |
| `chore(release)` | `release` | Excluded from the notes |
| any PR to leave out | `skip-changelog` | Excluded from the notes |

Until an owner-only labeler workflow lands, labels are applied by hand when
a PR is opened or merged — that workflow, if added, must run on
GitHub-hosted runners (never the self-hosted Mac for PR-triggered work),
use `pull_request_target` only without checking out or running PR code, and
carry minimal permissions (`pull-requests: write` only). It must not expose
secrets to fork PRs.

#### Still owner-only (not in this change)

Renaming the DMGs to arch-obvious names (e.g.
`gitpass_X.Y.Z_macos_apple-silicon.dmg` / `_intel.dmg`) and attaching a
`SHA256SUMS.txt` to every release both need a `release.yml` workflow change,
which agents must not make. Until the owner lands those, the template
documents the current asset names and carries a commented-out SHA256SUMS
placeholder instead of claiming the file exists.

### Runners and the `release` environment (since 0.6.8)

All workflows run on **GitHub-hosted runners**; the former self-hosted Mac runner was removed. CI and iOS use
`ubuntu-latest` / `macos-latest`; `release.yml` builds macOS arm64 and x64 on `macos-14` (x64 is cross-compiled).

- Desktop builds (tag and dry run) and the publish job run in the `release` environment, whose deployment rules
  allow `v*` tags and the `main` branch (needed so dry runs get the signing certificate). Publishing is tag-only.
  There is no required reviewer, so the unattended daily release (tags pushed by itsd0) does not stall; the
  owner-only actor guard in `release.yml` and the tag-only rule are the controls.
- **Dry run:** `gh workflow run release.yml -R itsd0/gitpass` (on `main`) builds both DMGs and the signed updater
  artifacts (`*.app.tar.gz` + `.sig`) as workflow artifacts. Nothing is published and no tag is created.
- **macOS code signing on hosted runners** needs the self-signed `gitpass Local Signing` identity as secrets:
  export it from Keychain Access as a `.p12` with a password, then
  `base64 -i gitpass-signing.p12 | gh secret set APPLE_CERTIFICATE -R itsd0/gitpass --env release` and
  `gh secret set APPLE_CERTIFICATE_PASSWORD -R itsd0/gitpass --env release`, and delete the `.p12` file. Until
  they are set, dry runs are ad-hoc signed and tag releases fail in preflight (so a release never silently
  changes the app's signing identity, which macOS uses for Keychain access).
- Tests run with `HOME` set to a temp dir.
- Windows/Linux builds are hosted (cost minutes) and only run via
  `gh workflow run release.yml -f include_windows_linux=true` (artifacts only, not published).

### Android release signing

`release.yml`'s `android` job (#479) builds `gitpass-android-X.Y.Z.apk` (APK Signature Scheme v2 + v3,
zipaligned, 16 KB ELF alignment checked) and `gitpass-android-X.Y.Z.aab`, plus `gitpass-android-X.Y.Z.sha256`.
On a `v*` tag the `publish` job attaches all three to the release on `itsd0/gitpass`. A dry run
(`gh workflow run release.yml -R itsd0/gitpass` on `main`) keeps them as the `gitpass-android-release` workflow
artifact. The job runs in the `release` environment and **fails if any of the four secrets is missing**, also on
a dry run.

- **Version:** `versionName` is the workspace version (`[workspace.package] version` in the root `Cargo.toml`), and
  `versionCode` is `major*10000 + minor*100 + patch` (0.6.8 → 608). Bumping the Cargo version in the release PR
  bumps the APK. On a tag the job fails when the APK version differs from the tag.
- **Keystore:** never committed and never on a runner disk longer than the job. CI decodes it to `$RUNNER_TEMP`
  and deletes it in an `always()` step. Gradle reads the signing config only from environment variables
  (`GITPASS_ANDROID_KEYSTORE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`) or the
  Gradle properties `gitpass.android.keystore`, `gitpass.android.keystorePassword`, `gitpass.android.keyAlias` and
  `gitpass.android.keyPassword`. Without any of them a local `./gradlew assembleRelease` builds an unsigned APK; a
  partial set fails the build.

**One-time setup by the owner (on the Mac).** Agents never create the keystore or set the secrets.

1. Generate the upload keystore. `keytool` ships with any JDK (`brew install --cask temurin` or Android Studio's
   bundled JBR). It prompts for the password, so the password never lands in shell history:

   ```sh
   keytool -genkeypair -v -storetype PKCS12 -keystore ~/gitpass-upload.jks -alias gitpass-upload \
     -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=gitpass, O=itsd0"
   ```

   With PKCS12 the key password is the keystore password, so `ANDROID_KEY_PASSWORD` gets the same value as
   `ANDROID_KEYSTORE_PASSWORD`.
2. Note the certificate fingerprint (this SHA-256 identifies the app, e.g. in Digital Asset Links):
   `keytool -list -v -keystore ~/gitpass-upload.jks -alias gitpass-upload`.
3. **Back it up before using it.** Copy `gitpass-upload.jks` to at least two offline places (e.g. an encrypted
   USB drive and an encrypted cloud or password-manager attachment). Store the password separately from the file.
   If the keystore or its password is lost, installed copies can never be updated: every user has to uninstall
   (losing the on-device vault data) and install a build with a new key.
4. Set the four secrets on the `release` environment. `gh secret set` prompts for each value:

   ```sh
   base64 -i ~/gitpass-upload.jks | gh secret set ANDROID_KEYSTORE_BASE64 -R itsd0/gitpass --env release
   gh secret set ANDROID_KEYSTORE_PASSWORD -R itsd0/gitpass --env release
   gh secret set ANDROID_KEY_ALIAS -R itsd0/gitpass --env release      # gitpass-upload
   gh secret set ANDROID_KEY_PASSWORD -R itsd0/gitpass --env release   # same as the keystore password
   ```
5. Check with a dry run on `main`, and keep the keystore only in the backups (not in the repo or `~/Downloads`).

A phone that has a CI **debug** build installed cannot update to the release-signed APK (different signer).
Uninstall the debug build first; this deletes its local vault data.
