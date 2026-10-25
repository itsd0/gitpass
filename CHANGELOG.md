# Changelog

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
