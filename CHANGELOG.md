# Changelog

## 0.6.6 — 2026-10-30

### Desktop
- "What's new" release sheet shown once after an update (#347).
- Compact/comfortable list density setting (#222); Text size setting with ⌘+ / ⌘− / ⌘0 (#312).
- Collapse the sidebar to an icon rail instead of hiding it (#283); collapsible Vaults/Tags sidebar sections with
  item counts (#284).
- Right-click context menu on the item list (#334); command palette recent items and actions (#269).
- Show as QR code for item detail fields (#348); scan a TOTP QR code from the screen, clipboard or a dropped image (#276).
- Colour-coded revealed passwords (desktop and extension) (#342).
- SSH Key item detail with key-type badge, fingerprint, Copy public key and Add to GitHub (#252); recent SSH agent
  activity in Settings (#256).
- Passkey item detail section (#260).
- Item editor field-type menu, reorder, undo and TOTP preview (#265).
- macOS menu bar icon with lock state, quick actions and a settings toggle (#224).
- Date section headers for date sorts (#247), and search results without date section headers (#299).
- Localized Watchtower page (#261).

### Extension / host
- Per-site Site settings panel in the popup (#349).
- Popup Archive and Recently Deleted views with restore and unarchive (#338); Large type and Move to Recently Deleted
  with Undo (#298).
- Attachments in popup item detail and a paperclip in list rows (#304).
- Browse popup items by tag (#321); filter chips replace the popup filter dropdown (#239); popup row quick
  actions (#266).
- Popup generator: Recent generated passwords (#230) and memorable-mode separator and capitalize options (#333).
- Choose between several passkeys for a site (#251).
- Website icons with monogram fallbacks (#214).
- Opt-in submit after fill for logins (#235).
- Open Watchtower, Settings and generator history in the app from the extension (#234).

### CLI
- Tag, favorite, archive, undelete and move items between vaults (#316).
- Create identity, Wi-Fi and SSH key items (#324).
- Copy a secret reference (`gp://vault/item/field`) and `gitpass ref` (CLI and desktop) (#327).
- `gitpass run` resolves `gp://` references from the inherited environment (#264).
- Sign Git commits and tags with an SSH key (CLI and desktop) (#272).

### Core / cross-platform
- Search query operators in core, desktop, CLI and extension (#306).
- Bank Account and Server categories (#240).
- API keys show their first 4 characters in masked form (#301).
- Import password-protected Bitwarden JSON exports (#285) and `.1pif` export files (#343).
- Watchtower flags unused items with Archive and Ignore actions (#302); RSA SSH keys (2048+ bits) and a
  weak_ssh_key finding (#245).

### Security
- Throttle repeated failed master-password unlocks with an increasing delay (core, CLI, desktop, extension) (#282).
- Caps Lock hint on master-password fields (#329).

### Fixes
- Passkeys created by gitpass can now be registered by sites; saved passkeys show in the popup site list (#273).
- Settings/Watchtower scrolling, Recently deleted type filter, and popup All items scrolling and paging (#275).
- Removed unused i18n keys, added the Burmese about.license string and a key parity test (#257).
- Rewrote the stale Quick Access README and used the typed plugin-dialog import in AcceptShare (#274).

### Docs
- EXTENSION.md updated for new popup features and the content script split (#353); submit-after-fill and website
  icons documented (#246).
- SECURITY_MODEL.md threat model (#250).
- Release-notes template, auto-notes config and labelling convention (#305).

### Maintenance
- Removed third-party trademark references (#300).
- Split popup.js (#328), content.js (#344), SettingsPage.tsx (#317) and core audit.rs (#337) into smaller modules;
  shared extension protocol logic between host and desktop (#297).
- Faster Rust CI with caching, nextest and split jobs (#323); GitHub Actions bumped to Node 24 majors (#311);
  cargo-machete guard and unused dependencies removed (#291); release runner cleanup (#310).

## 0.6.5 — 2026-10-29

The v0.6.4 tag has no published build: its release build failed on a Tauri npm/Rust version mismatch. 0.6.5 is the
first published release that contains every 0.6.4 change listed below, plus:

### Desktop
- Align Tauri npm and Rust versions (tauri/@tauri-apps/api 2.12, plugin-global-shortcut 2.4, plugin-updater 2.14),
  fixing the release build (#233).

### Extension / host
- The standalone gitpass-host now honors the "remember generated passwords" setting (#228).

### Docs
- List all import formats and aliases; document extension generator history (#229).

## 0.6.4 — 2026-10-29

### Desktop
- Render notes as safe Markdown with an edit/preview toggle (#207).
- Hide gitpass windows from screen recording and sharing (#210).
- Keyboard shortcuts for the selected item (#209).
- Wi-Fi items show a QR code to join the network (#202).
- Generator: words-mode separator and capitalize options (#197); generator popover on the item editor password
  field (#191); one shared generator request with common defaults and limits across core, CLI, desktop and host (#199).
- Drag items onto sidebar vaults and tags to move or tag them (#152).
- Website icons for logins with a monogram fallback (#164).
- Global hotkey registration and a settings recorder for Quick Access (#178).
- Lock the vault on Mac sleep, screen lock and user switch (#156).
- Encrypted password generator history (core, CLI, desktop, extension) (#198).
- Large type view for passwords and concealed secrets (#69).
- Restore a previous item version with a diff view (core, CLI, desktop) (#109).
- Add and remove attachments on any item (core, CLI, desktop) (#139).
- Track item usage with a "Recently used" sort and sections (core, CLI, desktop, extension) (#168).
- Watchtower: track password change time (#146), flag passwords containing the username or site name (#110),
  "Change password" action for findings (#102).
- Import LastPass and Proton Pass CSV (#163) and Apple Passwords / Safari CSV with OTPAuth (#119); import wizard
  polish with native picker, drag-and-drop, auto-detect and localized steps (#96).
- Search across all settings with a consistent section layout (#104); highlight item search matches with a localized
  result count (#120).
- Distinct styling for multi-select and a separate checkbox column (#169); polished localized empty states (#91);
  merged new-vault dialogs (#136).
- gitpass orange as the accent colour in desktop and extension (#130).
- Localized Home vault cards, Watchtower gauge and vault dialogs (#179), item detail strength meter, URL action and
  TOTP ring (#187), and unlock, create-vault and sidebar strings (#114) (en/my/th).
- Fixes: scrolling on Home and setup guide dismiss with undo (#175); keep the active settings section mounted (#115).

### CLI
- `gitpass add --category` for API credentials, secure notes and credit cards (#183).
- Search ranking deduplicated with `rank_by` (#141).

### Extension
- Offer to save a credit card after checkout (#203).
- Inline menu search for any login with a cross-site confirm (#127).
- Honor `passwordrules`/`maxlength` when suggesting a password (#194).
- Offer the one-time code on the next page after a login fill (#186).
- Show Watchtower warnings on logins in the popup (#161).
- Edit a login inline in the popup (#150); editable title, vault picker, "Save as new" and focus/saving states in the
  save banner (#153).
- Keyboard shortcuts: ⌘⇧L fills the best match without opening the popup (#131); ⌘⇧U copy username, ⌘O open
  website and a shortcuts sheet (#145).
- Password generator view with words/PIN via the host (#105).
- Polished options page, theme and desktop status card (#103).
- Highlighted search matches, keyboard hint footer and row aria attributes (#121); loading skeletons, request
  timeouts and inline error/retry states (#100).
- Right-click gitpass context menu (#215).
- Toolbar icon shows a locked state and a match-count badge (#221).
- Popup lists every item, with API secret preview and a sort menu (#174).

### Security
- Bump zip to 9.1.3 (GHSA-m665-v7pm-7grj) (#172); clear zip 4.6.1 from the desktop lockfile and enforce
  `cargo audit` in CI (#190).
- Use the full card number in fallback leak checks (#208).

### Tests / repo
- Extension user guide (docs/EXTENSION.md) (#223).
- Unit tests for fsutil crash safety, backup exclusions and locking (#151); e2e test for cancelling the in-page
  passkey prompt (#122); flaky test fixes (#113, #185).
- Removed unused locale keys with a locale parity test (#157), unused ScrollArea wrapper (#173) and unused
  `@playwright/test` (#162).
- Docs: complete CLI reference and CHANGELOG fixes (#140), audit findings documented (#132), README and AGENTS.md
  refreshed (#123).

## 0.6.3 — 2026-10-28

### Desktop
- Standalone password generator (⌘⇧G, command palette, save as new Login) (#64).
- Rename, merge and delete tags from the sidebar (#75).
- Flag duplicate logins with an "Archive older" action (#83).
- Purge countdown and "Empty Recently Deleted" (#90).
- "Ignore this warning" per item with an Ignored section (desktop + CLI) (#87).
- Flag logins saved with insecure `http://` URLs in Watchtower (#70).
- Flag expired and soon-to-expire cards and API credentials (core, CLI, desktop) (#58); expiry pills on item list
  rows and detail (#57); card/API credential expiry included in `ItemSummary` (#63).
- Render 2FA/passkey availability in Watchtower (#22); MFA dataset and matching follow-ups (#24); Watchtower
  scoring documented and tested (#35).
- Export the vault or selected items (#26).
- Multi-select with batched bulk actions, Shift+Arrow selection fixes (#3, #15), keyboard tests and undo toast for
  bulk actions (#38).
- Localized item detail actions, dialogs and toasts (en/my/th) (#81); Burmese/Thai Emergency Kit and settings
  wording (#18); harmonized Burmese "vault" term (#23).

### CLI
- `--json` and `--reveal` flags for `list`, `find` and `get` (#95).
- `completions` subcommand for zsh, bash and fish (#86).
- `gitpass gen` parity with the desktop generator (words, PIN, charset, `--copy`) (#82).
- Integration tests and docs for `gitpass run` and `gitpass inject` (#37); SSH agent socket tests (#36).

### Extension
- "Never for this site" banner and exceptions in options (#76).
- Live TOTP codes in the popup list with a countdown ring (#65).
- Injected UI follows the system light/dark theme (#77).
- Actionable popup empty states (#53).
- In-page passkey prompt, multi-step login usernames, one inline icon per form group, all item types in popup
  New item (#3); more e2e/unit coverage (#25, #39, #40/#47, #48, #71).

### Security
- Clipboard auto-clear and auto-lock on idle, sleep and screen lock (#44).
- Copied secrets marked concealed/transient on the macOS pasteboard (#59).
- Dependency security bumps: vite 6.4.5, vitest 4.1.12, time 0.3.47, serde_with 3.21.0 (#14).

### Tests / repo
- Desktop live refresh and lock events via host socket (#49); GitHub release check and signature verification
  against a mock server (#43).
- Public-repo readiness (LICENSE, SECURITY.md, hosted CI) (#10); multi-agent setup and agent docs (#4, #16, #34).

## 0.6.2 — 2026-10-27

### iOS (new, PR #1)
- First iOS app (`ios/`): SwiftUI app + AutoFill extension on top of gitpass-core via UniFFI (`gitpass-ios-ffi`);
  vault unlock, items, cards, Sync screen; free-team (personal) signing supported. iOS app version is 0.1.0.

### Security (PR #2)
- **Signed vault shares**: shares carry an Ed25519 signature (key derived from the identity secret via HKDF,
  `gitpass/share-signing/v1`) over every share field incl. the sealed vault key, plus a sender-binding tag keyed by
  X25519 DH(sender, recipient) (`gitpass/share-auth/v1`) so a share cannot claim someone else's identity.
  `vault accept` rejects unsigned, tampered or unbound shares. **Breaking:** shares created by older versions
  must be re-shared.
- **Touch ID (macOS)**: works with self-signed builds. The secret lives in the login keychain (ACL: gitpass's code
  signature) and is read only after a LocalAuthentication Touch ID check (one prompt); the biometry domain state is
  recorded at enrolment and a fingerprint change forces re-enrolment. Tradeoff: weaker than Secure-Enclave
  `.biometryCurrentSet` (which needs a paid Apple team entitlement) — see `biometric.rs`. Touch ID items from older
  builds must be re-enabled once (Settings → turn Touch ID off and on).
- Passkeys: p256 0.13 → 0.14 (crypto-bigint 0.7.10); storage format unchanged.

### Audit & Watchtower (PR #17, #20)
- **Offline 2FA / passkey availability**: compiled-in offline directory of MFA-capable sites (`mfa-sites.tsv`);
  audits logins without network calls and flags missing TOTP or passkeys. Longest/most-specific domain match wins,
  passkeys on subdomains (e.g. `accounts.google.com`) suppress passkey advice for the same registrable domain (`google.com`),
  and items on sites with documented TOTP support no longer receive double-flagged generic `MissingTotp` warnings.
- **Audit scope**: `gitpass audit` now audits only non-archived Login items (matching GUI/Watchtower behaviour),
  so `items_checked` can drop compared to earlier versions that audited all item categories and archived records.

### Supply chain / CI
- `deny.toml` + CI `supply-chain` job (`cargo deny check`, `cargo audit`) on the self-hosted Mac; every ignore has a
  written reason. Release signing key password read from the `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secret when set.

### Also included (from 0.6.1)
- Lock always redirects to the unlock screen (incl. auto-lock after sleep).

## 0.6.1 — 2026-10-27

### SSH keys, SSH agent and passkeys
- SSH Key items: generate Ed25519, import OpenSSH private keys (ed25519 / ECDSA P-256, passphrase-protected OK), public key + SHA256 fingerprint stored.
- Desktop SSH agent at `~/Library/Application Support/gitpass/ssh-agent.sock` (identities + sign, only while unlocked, optional approve-each-use). Settings → SSH Agent shows the `IdentityAgent` line.
- CLI: `gitpass ssh generate|import|list|public|config|agent`.
- New `passkey` item type; extension intercepts `navigator.credentials.create/get` (MAIN world) and asks for approval; host/broker `passkey_create` / `passkey_get` (ES256, none attestation).

### Desktop
- **Lock always returns to the unlock screen** (750bc18). The backend now emits `locked` on every lock path (idle auto-lock,
  lazy expiry inside a command, manual lock, extension/agent `lock`); any "vault is locked" command error also locks the UI.
  Item/vault state, modals and the item detail are cleared; after unlocking you return to the page you were on.
  No page or toast shows a raw "vault is locked" error anymore.
- **Auto-lock after system sleep**: the lock watcher detects a wall-clock gap (macOS `Instant` does not advance while
  asleep) and locks on wake.
- Cards: brand + last 4 in the list subtitle and a masked number; CVV no longer misdetected as the card number; skip
  "cardholder name" fields when locating the number (4cf2d44, f99c822). Extension popup too.
- Vault CRUD (rename / restyle / delete) in core, CLI and GUI; item list row heights, breadcrumb and empty-vault state fixes (992bb49).
- About shows the real app version; a 404 update check means "You're on the latest version"; my/th update strings (ffea0bd).

### Windows
- SSH agent gated behind `cfg(unix)` with a Windows stub; unix-only SSH tests (26e1d93, 479de0b).

### Release / CI
- Releases create the gitpass-releases tag from its own `main` (e0b2780).
- CI, iOS and macOS release builds run on a self-hosted Mac runner; tests use an isolated temp `HOME`; Windows/Linux
  release builds are opt-in (`include_windows_linux`); all actions pinned to commit SHAs. macOS builds are signed with the
  self-signed "gitpass Local Signing" identity (not notarized).

## 0.4.0 — integrated release (keys + data + ext + gui)

Merged from four parallel workspaces on top of 0.3.0. All crates, the desktop app and the extension are versioned 0.4.0.

### Format story
- `FORMAT_VERSION = 4`, `MIN_READ_FORMAT = 2`. v2 and v3 vaults still open; `init` still writes format 2.
  The manifest only becomes format 4 when an identity or vault members exist (`identity show`, `vault share/accept`).
  Vaults that never use sharing keep a v3 manifest. Clients older than 0.4.0 refuse a v4 manifest, so upgrade every device first.
- CLI fix (found separately in the keys, data and ext workspaces, kept once): `unlock_full` now checks `format.json` against
  `MIN_READ_FORMAT` instead of `FORMAT_VERSION` before it offers the v1 migration. Earlier, every v2/v3 vault got the
  "migrate" prompt.

### Real device revoke (keys)
- `device revoke <id>` now sets the revocation marker and also rotates the Account Key and every Vault Key, re-encrypting
  all items. The revoked device can no longer read changes made after that point.
- `--rotate-credentials` also changes the master password and generates a new Secret Key (`--save-secret-key` writes it to
  `<path>/secret.key`). `--marker-only` keeps the old v3 behaviour (marker only, no rotation). Core: `rotate_credentials`.
- A crash during rotation can be recovered: progress is journaled in the git-ignored `.gitpass-revoke-pending.json`.
- `device.rs` gives each device its own keypair (git-ignored `.gitpass-device-key.json`).
- **Limitation:** data the device already pulled or copied cannot be recalled. If the device knew the master password and
  Secret Key, you have to use `--rotate-credentials`.

### Vault sharing (keys)
- New `keys.rs`: X25519 identity (`x25519-dalek`), fingerprints, and an anonymous sealed box (ephemeral X25519 + HKDF-SHA256 + XChaCha20-Poly1305).
- CLI: `identity show`, `vault share <vault> --to <pubkey|file>`, `vault accept <member file>`, `vault unshare <vault> <member>`
  (removes the member and rotates the vault key), `vault members <vault>`.
- **Limitations:** the sealed box does not authenticate the sender, so check fingerprints out of band.
  `accept` imports a one-time snapshot: there is no live two-way shared vault. `unshare` cannot take back what the member already copied.

### Import / export (data)
- New `import.rs`: importers for `bitwarden-json`, `bitwarden-csv`, `generic-csv`, `keepass-csv`, `chrome-csv` and `gitpass-json`.
  CSV column matching is case-insensitive and accepts aliases. Unmapped columns are added to the notes. Duplicates
  (same title+username+url) are skipped unless `--allow-duplicates` is passed. `--dry-run` writes nothing.
- `export --format gitpass-json|csv` writes PLAINTEXT and needs `--yes`. `--encrypted` writes passphrase-protected gitpass-json
  (Argon2id + XChaCha20-Poly1305, passphrase from `$GITPASS_EXPORT_PASSPHRASE` or a prompt).
- **Limitations:** the column mappings come from vendor documentation and samples, so vendor format changes may need new aliases.
  Fields that don't map (folders and extra columns, for example) end up as text in the notes, not structured data.
  Plaintext exports are only as safe as wherever you store the file.

### Audit / HIBP (data)
- New `audit.rs` (core, `csv` dep): finds weak, reused and old (`--max-age-days`, default 365) passwords and items without TOTP.
  Findings list titles/ids only, never passwords.
- `audit --hibp` checks Pwned Passwords with k-anonymity: only the first 5 hex characters of the SHA-1 are sent, with
  `Add-Padding`. The HTTP client (`ureq`) lives only in the CLI. Core defines a `PwnedRangeClient` trait.
- **Limitations:** the weak-password check is a naive entropy estimate (length x log2 of the character pool), not a dictionary or pattern check. `--hibp` needs network access and reveals to HIBP that you ran a check.

### Desktop GUI (gui)
- `apps/desktop/`: a Tauri 2 app with a React + TypeScript (Vite) frontend. `gui-core` (the Tauri-free command layer, auto-lock,
  secret-free DTOs) is a member of the root workspace. `src-tauri` is excluded from it, with its own `Cargo.lock` and `.cargo/config.toml`.
- Secrets stay in Rust state and are zeroized on lock. Copying is done in Rust with a 30 s clipboard clear.
  `@tauri-apps/api` is pinned to `~2.11`.
- **Limitations:** the GUI does not expose the 0.4.0 sharing, import/export, audit or revoke-rotation features (use the CLI).
  A revoked device is read-only. While unlocked, sync keeps the password and Secret Key in memory. It has only been build-tested on Linux.

### Browser extension (ext)
- New crate `crates/gitpass-host`: a native messaging host (`com.gitpass.host`). It holds the Session and auto-locks after idle,
  and offers `status`, `unlock`, `lock`, `match`, `search`, `fill`, `totp` and `generate`. Origins are matched by eTLD+1 (`psl` + `url` crates).
  Callers are checked against `allowed_callers`. `gitpass-host install` writes the host manifests for Chrome, Chromium, Brave and Firefox.
- `extension/`: MV3 in plain JS with no build step (background, content script, popup). Tests run with `npm test`.
- **Limitations:** any subdomain of the same site matches. The extension is not published to any store (load it unpacked).
  The host is read-only: it has no add or edit command, so new logins cannot be saved from the browser.

## 0.3.0 — integrated release (merge + devices + usability + gitpass-core split)

### Workspace split
- Project is now a Cargo workspace: `crates/gitpass-core` (headless library: crypto, fsutil, store, merge, device, sync, migrate, totp, search) and `crates/gitpass-cli` (binary `gitpass`, clipboard behind the default `clipboard` feature via arboard).
- Core re-exports a GUI-friendly API: `Session`, `Item`, `ItemRecord`, `merge_items`, `device` registry, `totp_now`, `fuzzy_find`.

### Format story
- `FORMAT_VERSION = 3`, `MIN_READ_FORMAT = 2`. v2 vaults still unlock. Any write upgrades the *manifest* to v3; first unlock creates `devices.enc`; v2 item payloads get clocks synthesized from `updated_at`.
- `format.json` stays at 2 (init writes 2) and is bumped to 3 only when the keyset is re-wrapped (passwd, rotate-secret-key, KDF upgrade, v1 migrate), because the keyset AAD binds format.json.

### 0.3.0-merge — format v3 (field-level merge)
- **Per-field clocks.** The encrypted item payload is now `ItemRecord = {<v2 Item fields, flattened>, clocks:{title,username,
  password,url,notes,totp: u64 ms}}`. Same seal/open, AAD (`<vault>|<item>|<version>`), padding and manifest epoch bump.
  `update_item` bumps only the clocks of changed fields to `max(now_ms, old_clock+1)`; `updated_at` stays for display.
- **Tombstones.** Clearing a field = value `None` + bumped clock; a clear competes with edits by clock.
- **`src/merge.rs`**: `merge_records` / `merge_items` + `MergeReport {from_remote, from_local, conflicts}`.
- **`Session::merge_item(vid, iid, &ItemRecord)`**, `read_record`, `put_record`, `open_manifest_bytes`.
- **`gitpass sync --merge`**: snapshot local plaintext, fetch; on divergence verify merge-base epoch, take remote tree,
  field-merge every local item into it (re-add local-only items), re-seal, commit, push. Plain `sync` unchanged.
- **Compat / migrate (v2 -> v3 is "on next write", no migrate command needed):** v2 item payloads have no `clocks`;
  on read all fields get `updated_at*1000`. Every write emits clocks and writes `manifest.format = 3`. `format.json`
  is bound into the keyset AAD, so its `version` moves to 3 on the next keyset re-wrap (`passwd`, `rotate-secret-key`,
  KDF auto-upgrade) and on new `init`/`migrate`. Formats 2 and 3 are both read; > 3 refused. v0.2.0 will refuse a v3
  manifest/format.json ("manifest does not belong to this account/format") — upgrade all devices together.
- Known MVP limits of `sync --merge`: local-only vaults abort (backup kept); remote-deleted items you still have are
  resurrected; local deletes of items still present remotely are undone.


### 0.3.0-devices — device registry (manifest format v3)
- New `src/device.rs`: encrypted `devices.enc` (HKDF "gitpass/devices/v3", AAD "{account_id}|devices|3"), stable
  local id in git-ignored `.gitpass-device.json`, ISO 8601 UTC timestamps.
- Each unlock registers the device or refreshes `last_seen_at`. A revoked device's entry is left unchanged.
- Manifest gains optional `devices_hash` (left out when None, so v2 bytes are unchanged). `verify()` checks it, and
  `--accept-rollback` reseal includes it. The manifest `format` becomes 3 once devices.enc exists. `format.json`
  stays at v2.
- CLI: `device list|rename|revoke|unrevoke`, global `--device-name` / `GITPASS_DEVICE_NAME`.
- A revoked device is refused for create/add/edit/rm/passwd/rotate-secret-key/device mutations.
  `rotate-account-key` (with `--with-vault-keys`) stays allowed and re-seals devices.enc under the new Account Key.
- Limitation: revoke does NOT re-encrypt keys. See the README.


### 0.3.0-usability
- Added `copy` command and `get --copy` with 30s auto-clear (`--clear-secs`, `--field`, `--quiet`); `arboard` behind default `clipboard` feature.
- Added `totp` command (RFC 6238; otpauth URI or base32; in-tree HMAC via `hmac`/`sha1`).
- Added ranked fuzzy search (`src/search.rs`): `find`, `list --query`, `get --pick`.
- No format/schema changes.


## 0.2.0 — format v2
- **Authenticated encrypted manifest** (`manifest.enc`, key = HKDF(Account Key, "gitpass/manifest/v2")) with a monotonic
  `epoch`, per-file blake3 hashes and the keyset hash. Unlock (and `sync` after pulling) refuses on deletion, injection,
  substitution, or an epoch lower than the last one seen on this device. `--accept-rollback` overrides and re-seals.
- Vault names/descriptions encrypted (`meta.enc`). `meta.json` is gone.
- Item plaintext is padded to 512 B / 1 / 2 / 4 KiB / powers of two.
- format.json bound into the keyset.enc AAD. Default KDF is now Argon2id m=256 MiB, t=3, p=4, and weaker params are auto-upgraded on unlock.
- New commands: `passwd`, `rotate-secret-key`, `rotate-account-key [--with-vault-keys]`, `vault rotate`, `migrate`;
  `vault create --description`; global `--accept-rollback`, `--yes`.
- Atomic writes (temp + fsync + rename + dir fsync), `.gitpass.lock` (fs2 flock), pre-pull backups.
- `gitpass migrate` (v1 -> v2): backup, temp-dir build, marker-based recovery, format.json written last, single git commit.
- v2 refuses format versions > 2. v0.1.0 already refuses anything other than 1.
