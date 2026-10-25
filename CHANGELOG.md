# Changelog

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
