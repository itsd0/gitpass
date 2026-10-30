# gitpass threat model

Personal, single-owner (@itsd0) zero-knowledge password manager whose encrypted vault syncs through a private git repo. Client-side only: there are no gitpass servers. Targets macOS (desktop app, CLI, iOS) and Chromium (MV3 extension). No infrastructure-as-code exists; the only external infrastructure is GitHub (the vault remote, CI, and the release/update feed `itsd0/gitpass-releases`). `SECURITY.md` is a disclosure policy only; the crypto design is in `README.md`.

## Owner's decisions
- **Attackers:** every attacker below matters; none is ruled out.
- **Out of scope / accepted risk:** nothing. All platforms and code paths (incl. Linux/Windows fallbacks, Firefox manifest, iOS) stay in scope.
- **Assets:** all assets below matter equally.
- **Infrastructure:** client-only app, no infrastructure repo; GitHub Actions is not treated as an extra surface beyond what the scanner reports.

## Components
| Name | Code | Notes |
|---|---|---|
| **core** | `crates/gitpass-core` | Vault format v5 (reads v2+; `init` writes v2, bumped to v5 on keyset re-wrap), crypto (`crypto.rs`, `keys.rs`), store/manifest/epoch (`store.rs`), merge/sync/remote (`merge.rs`, `sync.rs`, `remote.rs`), import, audit/HIBP, passkeys, SSH keys, TOTP, sharing (X25519 sealbox + Ed25519 share signatures) |
| **cli** | `crates/gitpass-cli`, `crates/gitpass-pasteboard` | `gitpass` binary: `run`/`inject` secret injection into child env, ssh agent, import/export, clipboard |
| **native-host** | `crates/gitpass-host` | Native-messaging host for the extension; origin matching (`origin.rs`), request handler (`handler.rs`), install manifests (`install.rs`), bridge to desktop agent |
| **extension** | `extension/` | MV3 service worker (`background.js`, `bg-*.js`), content scripts on all https pages (`content.js`, `lib/forms.js`, `lib/passkey-main.js` in MAIN world, `lib/passkey-bridge.js`), popup, options |
| **desktop** | `apps/desktop/gui-core`, `apps/desktop/src-tauri`, `apps/desktop/src` | Tauri app: Rust API (`api.rs`), browser agent UNIX socket (`agent.rs`), ssh-agent (`ssh_agent.rs`), favicon fetcher (`favicon.rs`), Touch ID/Keychain (`biometric.rs`), updater, React UI |
| **ios** | `ios/`, `ios/rust/gitpass-ios-ffi` | SwiftUI app + AutoFill credential provider via UniFFI |
| **release** | `.github/workflows`, `docs/RELEASE.md`, `scripts/`, `tauri.conf.json` updater | Build, signing, minisign-verified auto-update from `itsd0/gitpass` releases (mirrored to `gitpass-releases` during the transition) |

## Trust boundaries and entry points
1. **Web page -> extension.** Any https page (and http loopback) runs alongside content scripts and the MAIN-world passkey shim. Page-controlled DOM, frames, messages and WebAuthn calls. Threat: credential/passkey release to the wrong origin, phishing lookalikes, content-script message spoofing.
2. **Extension -> native host.** Chrome native messaging; host manifest `allowed_origins` pins the extension ID. The host re-checks the requesting origin (`check_request_origin`, `origin_matches`: https or loopback, eTLD+1 via PSL, scheme equality). The origin string comes from the extension, so the service worker must derive it from `sender.tab`/`sender.url`, never from page data.
3. **Local processes -> desktop agent / ssh-agent sockets.** Socket dir 0700, socket 0600, peer-UID check, 32-byte pairing token in `agent.token` (0600). Code-signature check (SecCode) not implemented, so same-user processes that can read the token are trusted.
4. **Git remote -> vault.** The remote (GitHub) is untrusted storage: all content is XChaCha20-Poly1305 with AAD binding; manifest hashes + monotonic epoch detect tampering/rollback; `remote.rs` allowlists staged paths and refuses tokens in URLs. Known documented limitation: TOFU rollback on a fresh device (README).
5. **Imported / shared data.** Import files (`import.rs`), shared vaults/items from other identities (signed shares; sender identity verified out of band), SSH keys, otpauth URIs.
6. **Network fetches.** Favicon fetcher (https-only, public-IP-only, no redirects, size cap), HIBP k-anonymity audit, updater feed.
7. **Device / OS.** Keychain, Touch ID gating, pasteboard (concealed/transient types), iOS AutoFill extension, files under `~/.gitpass` and `~/Library/Application Support/gitpass`.

## Assets (all equal priority)
- Master password, Secret Key (`secret.key` / Emergency Kit), AUK, Account Key, vault keys, device keys, X25519/Ed25519 identity secrets.
- Vault item plaintext: passwords, notes, TOTP seeds, passkey private keys, SSH private keys, API credentials, injected env secrets.
- Vault integrity and anti-rollback state (`manifest.enc`, epoch in `.gitpass-state.json`).
- Release/updater integrity: minisign updater key, signing keys in `~/.gitpass-release`.

## Attacker profiles (all in scope)
- Malicious or compromised website (incl. XSS on a legitimate site, lookalike domains, iframes).
- Whoever controls the git remote / GitHub account (read, modify, roll back, delete vault files).
- Other apps or malware running as the same macOS user.
- Thief with a stolen or lost (locked) device.
- Sender of a malicious import file or shared vault.
- Network attacker (MITM on favicon/HIBP/update traffic).
- Supply chain (npm/crates dependencies, GitHub Actions).

## Existing controls
Argon2id (256 MiB, t=3, p=4) + Secret Key two-secret KDF; XChaCha20-Poly1305 everywhere with structured AAD; padding buckets; Zeroize types; manifest hash/epoch checks; atomic writes + lock file; origin matching rules in `crates/gitpass-host/src/origin.rs`; native-host `allowed_origins`; agent socket permissions + UID + token; secrets never written by the extension; concealed pasteboard + auto-clear; favicon SSRF guards; low-order point rejection in sealbox; signed shares; `cargo deny`/`cargo audit` in CI; actions pinned by SHA; unlock failure throttling (after 3 consecutive failed master password attempts per vault, further attempts are refused with exponential backoff doubling from 2 s up to a 5 min cap; state persists in `.gitpass-throttle.json` with clock rollback protection and monotonic timer).

## Open questions
- Should same-user local malware be treated as fully trusted (the agent token is readable by the user)? Currently in scope.
- Is the Firefox build actually shipped, given AGENTS.md says Chromium only? Currently in scope.
