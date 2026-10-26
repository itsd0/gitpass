# Releases, cross-platform builds and auto-update

## Workflows
* `.github/workflows/ci.yml` — on push/PR/manual: `cargo test --workspace`, desktop + extension `npm test`,
  and `cargo check` of the Tauri app on Linux, macOS and Windows.
* `.github/workflows/release.yml` — on tag `v*`: builds the desktop app for macOS arm64 + x64 (app, dmg;
  **ad-hoc signed**, not notarized), Linux x64 (AppImage, deb) and Windows x64 (msi, nsis), creates a **draft**
  GitHub Release with the installers, updater artifacts (`*.sig`) and `latest.json`. Also builds the CLI and
  `gitpass-host` binaries as workflow artifacts. `workflow_dispatch` builds the same things as artifacts only.

## Platform support matrix
| Feature | macOS | Linux | Windows |
|---|---|---|---|
| Biometric unlock | Touch ID + Keychain | not supported (NoBiometrics) | **stub** (NoBiometrics; Windows Hello / Credential Manager not implemented) |
| App ⇄ extension agent | UNIX socket `~/Library/Application Support/gitpass/agent.sock` | UNIX socket `$XDG_RUNTIME_DIR/gitpass/agent.sock` | **stub**: named pipe `\\.\pipe\gitpass-agent` planned; app logs "not implemented", host runs standalone |
| Native host install | `~/Library/Application Support/<browser>/NativeMessagingHosts` | `~/.config/{google-chrome,chromium,BraveSoftware/Brave-Browser}/NativeMessagingHosts`, `~/.mozilla/native-messaging-hosts` | manifest in `%LOCALAPPDATA%\gitpass\<browser>\`, registered under `HKCU\Software\{Google\Chrome,Chromium,BraveSoftware\Brave-Browser,Mozilla}\NativeMessagingHosts\com.gitpass.host` |

## Auto-update (tauri-plugin-updater)
* Endpoint: `https://github.com/itsd0/gitpass/releases/latest/download/latest.json`; public key in
  `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
* Private signing key: only on the release Mac at `~/.gitpass-release/updater.key` (0600, no password) and in the
  repo Actions secret `TAURI_SIGNING_PRIVATE_KEY`. Losing it means existing installs can't verify future updates.
* Settings → About → "Check for updates".
* Release drafts must be **published** (and not marked pre-release) for `releases/latest` to resolve.
* Local `npm run tauri build` now also produces updater artifacts and therefore needs
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.gitpass-release/updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""`.

### Decision needed: the repo is private
Anonymous requests to a private repo's release assets return 404, so the updater **cannot** fetch
`latest.json` or the installers from `itsd0/gitpass` as configured. Options:
1. **Public releases-only repo** (e.g. `itsd0/gitpass-releases`): release.yml publishes there
   (tauri-action `owner`/`repo` inputs + a fine-grained PAT secret with contents:write on that repo); change the
   endpoint to `https://github.com/itsd0/gitpass-releases/releases/latest/download/latest.json`. Source stays private.
2. **Self-hosted / static hosting** (S3, R2, Vercel, GitHub Pages of a public repo) for `latest.json` + artifacts.
3. **Token in the app** (updater `headers: Authorization`) — not recommended: a read token shipped in every install
   leaks access to the private repo.
4. Make `itsd0/gitpass` public.
Nothing has been created for this yet.
