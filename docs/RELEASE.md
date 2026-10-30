# Releases, cross-platform builds and auto-update

## Workflows
* `.github/workflows/ci.yml` — on push/PR/manual: `cargo test --workspace`, desktop + extension `npm test`,
  and `cargo check` of the Tauri app on Linux, macOS and Windows.
* `.github/workflows/release.yml` — on tag `v*`: builds the desktop app for macOS arm64 + x64 (app, dmg;
  **ad-hoc signed**, not notarized), Linux x64 (AppImage, deb) and Windows x64 (msi, nsis), creates a GitHub
  Release on **`itsd0/gitpass`** (this repo, same tag) with the installers, updater artifacts (`*.sig`) and `latest.json`;
  the `publish` job then publishes it as Latest and mirrors the same files to `itsd0/gitpass-releases` (transition,
  see below). Also builds the CLI and
  `gitpass-host` binaries as workflow artifacts. `workflow_dispatch` builds the same things as artifacts only.

## Platform support matrix
| Feature | macOS | Linux | Windows |
|---|---|---|---|
| Biometric unlock | Touch ID + Keychain | not supported (NoBiometrics) | **stub** (NoBiometrics; Windows Hello / Credential Manager not implemented) |
| App ⇄ extension agent | UNIX socket `~/Library/Application Support/gitpass/agent.sock` | UNIX socket `$XDG_RUNTIME_DIR/gitpass/agent.sock` | **stub**: named pipe `\\.\pipe\gitpass-agent` planned; app logs "not implemented", host runs standalone |
| Native host install | `~/Library/Application Support/<browser>/NativeMessagingHosts` | `~/.config/{google-chrome,chromium,BraveSoftware/Brave-Browser}/NativeMessagingHosts`, `~/.mozilla/native-messaging-hosts` | manifest in `%LOCALAPPDATA%\gitpass\<browser>\`, registered under `HKCU\Software\{Google\Chrome,Chromium,BraveSoftware\Brave-Browser,Mozilla}\NativeMessagingHosts\com.gitpass.host` |

## Auto-update (tauri-plugin-updater)
* Endpoints (tried in order): `https://github.com/itsd0/gitpass/releases/latest/download/latest.json`, then
  `https://github.com/itsd0/gitpass-releases/releases/latest/download/latest.json`; public key in
  `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
* Releases are hosted on `itsd0/gitpass` (#292). **Transition:** apps up to 0.6.7 only know the
  `gitpass-releases` endpoint, so every release is also mirrored there (installers, `*.app.tar.gz` + `.sig`, and a
  `latest.json` whose URLs point at the `gitpass-releases` copies). Keep the mirror until no supported install
  predates the two-endpoint config; then drop the mirror step and the second endpoint.
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
  job does this in both repos after both macOS builds succeed.
* Local `npm run tauri build` now also produces updater artifacts and therefore needs
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.gitpass-release/updater.key)"` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` set to the key's password (empty only for a legacy unprotected key).
  Never put either value in a shell history file, a committed file, or chat — export them in the
  current shell only.

### Required secret: `RELEASES_REPO_TOKEN` (one-time, manual)
Used only by the mirror step: `GITHUB_TOKEN` of `itsd0/gitpass` cannot write to another repo. Create a **fine-grained personal access token**
(GitHub → Settings → Developer settings → Fine-grained tokens): Resource owner `itsd0`, Repository access
*Only select repositories* → `itsd0/gitpass-releases`, Permissions → *Contents: Read and write* (Metadata: read is
implied), with an expiry. Then store it on the source repo:

    gh secret set RELEASES_REPO_TOKEN -R itsd0/gitpass   # paste the token

Without it the tag-triggered mirror step fails (the itsd0/gitpass release is still published); `workflow_dispatch` builds are unaffected.

### Cutting a release
1. Bump the version in `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/package.json`, the Cargo manifests.
2. Add a `RELEASES` entry for the new version in `apps/desktop/src/whats-new/releases.ts` (3–6 highlights, en/my/th).
   Required: the desktop test `has highlights for the app version being released` fails CI when the bumped
   version has no entry, so the release PR cannot merge without it.
3. `git tag vX.Y.Z && git push origin vX.Y.Z` (on `itsd0/gitpass`).
4. Wait for `release.yml`: it publishes the release on `itsd0/gitpass` as Latest and mirrors it to
   `itsd0/gitpass-releases`. Then set the polished notes on the `itsd0/gitpass` release (`gh release edit --notes-file`)
   and check that both repos' `releases/latest/download/latest.json` show the new version.

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

- Tag builds and the publish job run in the `release` environment, whose deployment rule only allows `v*` tags.
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
