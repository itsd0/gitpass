# Changelog

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
- New `import.rs`: importers for `bitwarden-json`, `bitwarden-csv`, `1password-csv`, `keepass-csv`, `chrome-csv` and `gitpass-json`.
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
