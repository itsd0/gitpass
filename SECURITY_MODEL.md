# gitpass threat model

Personal, single-owner (@itsd0) zero-knowledge password manager whose encrypted vault syncs through a private git repo. Client-side only: there are no gitpass servers. Targets macOS (desktop app, CLI, iOS) and Chromium (MV3 extension). An Android companion is in staged development and is not a supported production app. No infrastructure-as-code exists; the only external infrastructure is GitHub (the vault remote, CI, and release/update feed on `itsd0/gitpass` releases). `SECURITY.md` is a disclosure policy only; the crypto design is in `README.md`. Pairing v2 is in [docs/PAIRING.md](docs/PAIRING.md).

## Owner's decisions
- **Attackers:** every attacker below matters; none is ruled out.
- **Out of scope / accepted risk:** nothing. All platforms and code paths (incl. Linux/Windows fallbacks, Firefox manifest, iOS) stay in scope.
- **Assets:** all assets below matter equally.
- **Infrastructure:** client-only app, no infrastructure repo; GitHub Actions is not treated as an extra surface beyond what the scanner reports.

## Components
| Name | Code | Notes |
|---|---|---|
| **core** | `crates/gitpass-core` | Vault format v5 (reads v2+; `init` writes v2, bumped to v5 on keyset re-wrap), crypto (`crypto.rs`, `keys.rs`), store/manifest/epoch (`store/`), merge/sync/remote (`merge.rs`, `sync.rs`, `remote.rs`), import, audit/HIBP, passkeys, SSH keys, TOTP, sharing (X25519 sealbox + Ed25519 share signatures), pairing v2 (`pairing.rs`; the crate does not open sockets) |
| **cli** | `crates/gitpass-cli`, `crates/gitpass-pasteboard` | `gitpass` binary: `run`/`inject` secret injection into child env, ssh agent, import/export, clipboard |
| **native-host** | `crates/gitpass-host` | Native-messaging host for the extension; origin matching (`origin.rs`), request handler (`handler.rs`), install manifests (`install.rs`), bridge to desktop agent |
| **extension** | `extension/` | MV3 service worker (`background.js`, `bg-*.js`), content scripts on all https pages (`content.js`, `lib/forms.js`, `lib/passkey-main.js` in MAIN world, `lib/passkey-bridge.js`), popup, options |
| **desktop** | `apps/desktop/gui-core`, `apps/desktop/src-tauri`, `apps/desktop/src` | Tauri app: Rust API (`api/`), browser agent UNIX socket (`agent.rs`), ssh-agent (`ssh_agent.rs`), favicon fetcher (`favicon.rs`), Touch ID/Keychain (`biometric.rs`), updater, React UI |
| **ios** | `ios/`, `crates/gitpass-mobile-ffi` | SwiftUI app + AutoFill credential provider via UniFFI. Pairing scans the desktop QR and opens a raw TCP connection |
| **android** | `android/`, `crates/gitpass-mobile-ffi` | Staged companion, not a supported production app. Pairing uses the same bridge. Passkeys are Android 14+ only |
| **release** | `.github/workflows`, `docs/RELEASE.md`, `scripts/`, `tauri.conf.json` updater | Build, signing, minisign-verified auto-update from `itsd0/gitpass` releases |

## Trust boundaries and entry points
1. **Web page -> extension.** Any https page (and http loopback) runs alongside content scripts and the MAIN-world passkey shim. Page-controlled DOM, frames, messages and WebAuthn calls. Threat: credential/passkey release to the wrong origin, phishing lookalikes, content-script message spoofing.
2. **Extension -> native host.** Chrome native messaging; host manifest `allowed_origins` pins the extension ID. The host re-checks the requesting origin (`check_request_origin`, `origin_matches`: https or loopback, eTLD+1 via PSL, scheme equality). The origin string comes from the extension, so the service worker must derive it from `sender.tab`/`sender.url`, never from page data.
3. **Local processes -> desktop agent / ssh-agent sockets.** Socket dir 0700, socket 0600, peer-UID check, 32-byte pairing token in `agent.token` (0600). Code-signature check (SecCode) not implemented, so same-user processes that can read the token are trusted.
4. **Git remote -> vault.** The remote (GitHub) is untrusted storage: all content is XChaCha20-Poly1305 with AAD binding; manifest hashes + monotonic epoch detect tampering/rollback; `remote.rs` allowlists staged paths and refuses tokens in URLs. Known documented limitation: TOFU rollback on a fresh device (README).
5. **Imported / shared data.** Import files (`import.rs`), shared vaults/items from other identities (signed shares; sender identity verified out of band), SSH keys, otpauth URIs.
6. **Network fetches.** Favicon fetcher (https-only, public-IP-only, no redirects, size cap), HIBP k-anonymity audit, updater feed.
7. **Device / OS.** Keychain, Touch ID gating, pasteboard (concealed/transient types), iOS AutoFill extension, files under `~/.gitpass` and `~/Library/Application Support/gitpass`.
8. **LAN device pairing.** An already-unlocked desktop shows one QR. The phone scans it and connects to a numeric address in that QR over raw TCP on the local network. There is no gitpass server and no mailbox in the sync repo. The master password is not in the QR or in any frame; the person types it on the new phone. See [Device pairing](#device-pairing).

## Assets (all equal priority)
- Master password, Secret Key (`secret.key` / Emergency Kit), AUK, Account Key, vault keys, device keys, X25519/Ed25519 identity secrets.
- Vault item plaintext: passwords, notes, TOTP seeds, passkey private keys, SSH private keys, API credentials, injected env secrets.
- Sync token (fine-grained PAT) while it is being paired onto a new device and while it sits in that device's keystore.
- Vault integrity and anti-rollback state (`manifest.enc`, epoch in `.gitpass-state.json`).
- Release/updater integrity: minisign updater key, signing keys in `~/.gitpass-release`.

## Attacker profiles (all in scope)
- Malicious or compromised website (incl. XSS on a legitimate site, lookalike domains, iframes).
- Whoever controls the git remote / GitHub account (read, modify, roll back, delete vault files).
- Other apps or malware running as the same macOS user.
- Thief with a stolen or lost (locked) device.
- Sender of a malicious import file or shared vault.
- Network attacker (MITM on favicon/HIBP/update traffic, and anyone on the same LAN during pairing).
- Supply chain (npm/crates dependencies, GitHub Actions).

## Existing controls
Argon2id (256 MiB, t=3, p=4) + Secret Key two-secret KDF; XChaCha20-Poly1305 everywhere with structured AAD; padding buckets; Zeroize types; manifest hash/epoch checks; atomic writes + lock file; origin matching rules in `crates/gitpass-host/src/origin.rs`; native-host `allowed_origins`; agent socket permissions + UID + token; secrets never written by the extension; concealed pasteboard + auto-clear; favicon SSRF guards; low-order point rejection in sealbox; signed shares; `cargo deny`/`cargo audit` in CI; actions pinned by SHA; unlock failure throttling (after 3 consecutive failed master password attempts per vault, further attempts are refused with exponential backoff doubling from 2 s up to a 5 min cap; state persists in `.gitpass-throttle.json` with clock rollback protection and monotonic timer).

## Device pairing

Pairing v2 (#491) replaced the v1 offer/announce/grant frames. The sync-repo mailbox considered after #487 was not built: the phone connects directly to the desktop on the LAN. The byte layout is in [docs/PAIRING.md](docs/PAIRING.md). `crates/gitpass-core` does not open sockets. Swift, Kotlin and JavaScript only move opaque bytes.

The desktop, already unlocked, shows one QR (`gitpass-pair:2:qr:…`). The QR has no secrets. It does carry the session id, the expiry, the TCP port, up to eight numeric addresses, and the desktop's ephemeral X25519 public key. A photograph of the QR reveals those values and nothing else. Version 1 text is rejected.

The phone commits first: it sends `SHA-256(phone_pub || nonce_p)` and does not reveal either value yet. The desktop then reveals a fresh `nonce_d` and repeats its public key, which must match the QR. The phone opens the commitment. A different key fails the hash, so an active LAN peer gets one guess at the 6-digit code per session (at most 1 in 10^6), not an offline grind. The code is the first four bytes of the SAS key, as a big-endian integer modulo 1_000_000. It is a comparison string, not an encryption key. The desktop compares it in constant time and does not return its own copy. Three wrong codes wipe the session. The phone is not told that a code was wrong: a fail frame is either "expired" or a generic refusal.

Both sides derive keys with HKDF-SHA256. The input is the X25519 shared secret (`was_contributory`; the all-zero output is rejected). The salt is `SHA-256` of the transcript. The info strings are `gitpass/pair/v2/aead` and `gitpass/pair/v2/sas`. The grant is the existing XChaCha20-Poly1305 envelope. Its additional data is `gitpass/pair/v2` concatenated with the salt. There is no TLS. The AEAD is the only protection on the TCP connection.

The payload is version 2, account id, repo owner, repo name, branch, Secret Key, a fine-grained GitHub PAT (`github_pat_…`) and `token_kind` (`fine_grained_pat`). A `password` or `master_password` field is rejected. Classic `ghp_` tokens are rejected. The desktop reads the owner, repository, branch and token from the vault's git `origin` and the git-ignored `.gitpass-sync-token` file, not from pairing UI arguments. The listener binds LAN addresses only, never `0.0.0.0`, and the production listener skips loopback. The app is not sandboxed, so no `com.apple.security.network.server` entitlement was added. macOS may still ask to allow incoming connections; the screen says to allow that prompt. A guest network that isolates clients cannot be fixed in the protocol.

The phone types the Master Password locally after the grant, then runs the existing first pull. That password is not in the QR, the frames, a log or an error. Session ids are single-use for 24 hours after the two-minute expiry (`.gitpass-pairing-consumed.json`, git-ignored, ids and expiries only). Ephemeral private keys are wiped when the grant is taken, the session fails, or it expires. Errors are fixed strings: they do not include the token, the Secret Key, the master password, or a filesystem path.

What this does not stop: a person who types a code for a session they are looking at when that session is the attacker's (they are comparing the attacker's session with itself); an active peer who connects before the real phone and then gets the one guess; anyone who already holds the phone's ephemeral private key. Whoever connects first occupies the desktop's one session. Client isolation on the LAN is outside the protocol. Registering the new phone in `devices.enc` happens later, when that phone unlocks with the typed master password and the delivered Secret Key.

## Open questions
- Should same-user local malware be treated as fully trusted (the agent token is readable by the user)? Currently in scope.
- Is the Firefox build actually shipped, given AGENTS.md says Chromium only? Currently in scope.
