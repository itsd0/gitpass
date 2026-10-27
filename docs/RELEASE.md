# Releases, cross-platform builds and auto-update

## Workflows
* `.github/workflows/ci.yml` — on push/PR/manual: `cargo test --workspace`, desktop + extension `npm test`,
  and `cargo check` of the Tauri app on Linux, macOS and Windows.
* `.github/workflows/release.yml` — on tag `v*`: builds the desktop app for macOS arm64 + x64 (app, dmg;
  **ad-hoc signed**, not notarized), Linux x64 (AppImage, deb) and Windows x64 (msi, nsis), creates a **draft**
  GitHub Release on the public repo **`itsd0/gitpass-releases`** (same tag name) with the installers, updater artifacts (`*.sig`) and `latest.json`. Also builds the CLI and
  `gitpass-host` binaries as workflow artifacts. `workflow_dispatch` builds the same things as artifacts only.

## Platform support matrix
| Feature | macOS | Linux | Windows |
|---|---|---|---|
| Biometric unlock | Touch ID + Keychain | not supported (NoBiometrics) | **stub** (NoBiometrics; Windows Hello / Credential Manager not implemented) |
| App ⇄ extension agent | UNIX socket `~/Library/Application Support/gitpass/agent.sock` | UNIX socket `$XDG_RUNTIME_DIR/gitpass/agent.sock` | **stub**: named pipe `\\.\pipe\gitpass-agent` planned; app logs "not implemented", host runs standalone |
| Native host install | `~/Library/Application Support/<browser>/NativeMessagingHosts` | `~/.config/{google-chrome,chromium,BraveSoftware/Brave-Browser}/NativeMessagingHosts`, `~/.mozilla/native-messaging-hosts` | manifest in `%LOCALAPPDATA%\gitpass\<browser>\`, registered under `HKCU\Software\{Google\Chrome,Chromium,BraveSoftware\Brave-Browser,Mozilla}\NativeMessagingHosts\com.gitpass.host` |

## Auto-update (tauri-plugin-updater)
* Endpoint: `https://github.com/itsd0/gitpass-releases/releases/latest/download/latest.json`; public key in
  `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
* `itsd0/gitpass` (source) is private, so installers + `latest.json` are published to the **public releases-only
  repo** `https://github.com/itsd0/gitpass-releases` (no source code there). `latest.json` URLs point at that repo's
  release assets.
* Private signing key: only on the release Mac at `~/.gitpass-release/updater.key` (0600, no password) and in the
  repo Actions secret `TAURI_SIGNING_PRIVATE_KEY`. Losing it means existing installs can't verify future updates.
* Settings → About → "Check for updates".
* Release drafts on `gitpass-releases` must be **published** (and not marked pre-release) for `releases/latest` to resolve.
* Local `npm run tauri build` now also produces updater artifacts and therefore needs
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.gitpass-release/updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""`.

### Required secret: `RELEASES_REPO_TOKEN` (one-time, manual)
`GITHUB_TOKEN` of `itsd0/gitpass` cannot write to another repo. Create a **fine-grained personal access token**
(GitHub → Settings → Developer settings → Fine-grained tokens): Resource owner `itsd0`, Repository access
*Only select repositories* → `itsd0/gitpass-releases`, Permissions → *Contents: Read and write* (Metadata: read is
implied), with an expiry. Then store it on the source repo:

    gh secret set RELEASES_REPO_TOKEN -R itsd0/gitpass   # paste the token

Without it the tag-triggered `build + publish` step fails; `workflow_dispatch` builds are unaffected.

### Cutting a release
1. Bump the version in `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/package.json`, the Cargo manifests.
2. `git tag vX.Y.Z && git push origin vX.Y.Z` (on `itsd0/gitpass`).
3. Review the draft on `itsd0/gitpass-releases`, then publish it (non-prerelease) so the updater sees it.

### Self-hosted Mac runner (since v0.6.1)

CI, the iOS workflow and the macOS release builds run on a repo-level self-hosted runner on the
maintainer's Mac (`~/actions-runner`, labels `self-hosted,macOS,gitpass-mac`), registered to the
private repo **itsd0/gitpass only** — never attach it to a public repo (including gitpass-releases).

- Service: `cd ~/actions-runner && ./svc.sh status|start|stop` (LaunchAgent; runs while the user is logged in).
- Tests run with `HOME` set to a temp dir, so they never touch `~/.gitpass` or the app's agent socket.
- macOS release signing uses the login-keychain identity `gitpass Local Signing`. The release job's
  preflight step test-signs a file and fails with a clear error if the keychain is not reachable
  (locked, or the service runs outside the user's session). Fix: log in / unlock the login keychain.
- Windows/Linux builds are hosted (cost minutes) and only run via
  `gh workflow run release.yml -f include_windows_linux=true` (artifacts only, not published).
