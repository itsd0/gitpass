# gitpass — zero-knowledge password manager synced through a private GitHub repo (format v2)

A clean-room Rust CLI. Its design follows the *publicly documented* two-secret model used by 1Password: a master password plus a Secret Key, with end-to-end encryption. It contains no proprietary code. It only uses vetted RustCrypto crates (`argon2`, `hkdf`, `sha2`, `chacha20poly1305`, `zeroize`, `rand`).

> **NOT audited.** Don't trust it with real secrets until it has been reviewed independently.

## Build
```bash
cargo build --release      # binary: target/release/gitpass
cargo test                 # unit/integration/KAT/proptest suites (tests use 8 MiB Argon2 for speed)
```

## Key hierarchy (two-secret model)
```
pw_key = Argon2id(master_password, salt16, params from format.json)   default m=256 MiB, t=3, p=4 -> 32 B
sk_key = HKDF-SHA256(ikm=SecretKey(128 bit), salt=account_id, info="gitpass-sk-v1") -> 32 B
AUK    = pw_key XOR sk_key
AUK         --wraps--> Account Key   keyset.enc                 AAD "gitpass/keyset/v2|" || canonical_json(format.json)
Account Key --HKDF "gitpass/manifest/v2"--> manifest_key -> manifest.enc   AAD "<account_id>|manifest|2"
Account Key --wraps--> Vault Key     vaults/<id>/vault.key.enc  AAD "<vault_id>|vault-key|<key_version>"
Vault Key   --seals--> meta.enc {name, description}             AAD "<vault_id>|meta|2"
Vault Key   --seals--> padded item   vaults/<id>/items/<id>.enc AAD "<vault_id>|<item_id>|<version>"
```
All encryption is XChaCha20-Poly1305 with a random 24-byte nonce. Key material is held in `Zeroize` types.

Secret Key format: `A3-XXXXXX-XXXXXX-XXXXX-XXXXX-XXXXX`. That's 26 base32 characters (128 bits) plus 1 checksum character.

## Format v2 spec
```
format.json        {version:2, kdf:{alg,m_kib,t,p}, salt, account_id}     (plaintext, but authenticated via keyset AAD)
keyset.enc         Envelope{alg, nonce, ct}
manifest.enc       Envelope; plaintext = serde_json (BTreeMaps => sorted keys, deterministic) of
                   {format:2, account_id, epoch:u64, keyset_hash:blake3(keyset.enc),
                    vaults:{vault_id:{key_version:u32, key_hash, meta_hash,
                                      items:{item_id:{version:u64, hash:blake3(item file)}}}}}
vaults/<uuid>/vault.key.enc   {key_version, envelope}
vaults/<uuid>/meta.enc        Envelope (encrypted {name, description})
vaults/<uuid>/items/<uuid>.enc {version, envelope}; plaintext = u32le(len) || item_json || zero padding
.gitignore         secret.key, *.tmp, .gitpass.lock, .gitpass-state.json, .gitpass-backup-*/, .gitpass-migrate-tmp/, .gitpass-migrating
```
**Padding buckets:** the padded size is the smallest of 512 B, 1 KiB, 2 KiB or 4 KiB that fits, and the next power of two above that. Within a bucket, file size doesn't reveal item length.

**Integrity / rollback protection.** Every mutating command takes `.gitpass.lock`, writes its files atomically, bumps `epoch`, recomputes hashes and re-seals `manifest.enc`. On every unlock, and after `sync` pulls, gitpass decrypts the manifest and checks the following. Every listed file must exist with the recorded blake3 hash. There must be no unlisted vault dirs or item files. `keyset.enc` must match. `epoch` must be ≥ the last epoch this device saw, which is stored in the git-ignored `<path>/.gitpass-state.json`. If any check fails, gitpass refuses. `--accept-rollback` accepts the current disk state: it rebuilds the manifest from the files that still authenticate and moves the epoch forward. A manifest that fails authentication is always refused.

**TOFU limitation.** A fresh device, or one where `.gitpass-state.json` was deleted, has no stored epoch. It will accept any *self-consistent, older* snapshot of the repo that your keys can open. Per-file tampering and deletion are still caught. To mitigate this, compare `gitpass unlock`'s reported epoch across your devices.

**KDF params.** The params live in `format.json` and are bound into the AAD of `keyset.enc`. If someone edits them, unlock fails, and params below 8 MiB / t=1 are rejected outright. If the stored params are below the current target (256 MiB, t=3, p=4), unlock re-derives the key with a new salt, re-wraps `keyset.enc`, and bumps the epoch. A keyset re-wrap writes `format.json.new` + `keyset.enc.new` and then renames them. An interrupted re-wrap is rolled forward on the next unlock.

## Usage
```bash
gitpass init [--save-secret-key]     # prints the Emergency Kit with your Secret Key
gitpass unlock
gitpass vault create personal
gitpass vault list
gitpass add personal --title GitHub --username me --url https://github.com --generate 24
gitpass add personal --title Bank                 # prompts for the item password
gitpass list personal
gitpass get personal github [--password-only]     # query = id, id prefix or title substring
gitpass edit personal <id|query> --notes "..." [--password | --generate 20]
gitpass rm personal <id|query>
gitpass gen 32
gitpass sync                 # lock, commit, backup, pull --rebase, push, then verify the manifest
gitpass vault create work --description "work logins"   # name + description are encrypted
gitpass passwd               # new master password (GITPASS_NEW_PASSWORD for scripts)
gitpass rotate-secret-key [--save-secret-key]   # new Secret Key + new Emergency Kit
gitpass rotate-account-key [--with-vault-keys]  # new Account Key; re-wraps all vault keys
gitpass vault rotate work    # new Vault Key; re-encrypts every item in the vault
gitpass migrate              # v1 -> v2 (also offered automatically on unlock; --yes to auto-accept)
gitpass --accept-rollback unlock   # accept a detected rollback/tamper and re-seal (dangerous)
```
Options and environment variables:
- `--path DIR` or `GITPASS_PATH` sets the vault dir (default `~/.gitpass`).
- The Secret Key is read in this order: `GITPASS_SECRET_KEY`, then `<path>/secret.key` (mode 0600, git-ignored, written by `init --save-secret-key`), then an interactive prompt.
- `GITPASS_PASSWORD` and `GITPASS_ITEM_PASSWORD` supply passwords without prompting. They are meant for scripts and tests only, because environment variables can leak.

## GitHub sync
```bash
cd ~/.gitpass && git init
# create a PRIVATE repo on GitHub, then:
git remote add origin git@github.com:YOU/gitpass-vault.git   # SSH deploy key with write access (recommended)
# or https://github.com/YOU/gitpass-vault.git with a fine-grained PAT scoped to that repo
gitpass sync     # git add -A; commit "sync"; pull --rebase; push
```
If there's no `origin`, `sync` commits locally and skips pull/push. On other devices, `git clone` the repo, then use the same password and Secret Key.

## Migrating from v1 (gitpass 0.1.x)
```bash
gitpass sync                 # with the OLD binary on every device first, so nothing is unpushed
gitpass migrate              # or just run any command and answer "y"
gitpass sync                 # push the single "migrate to format v2" commit
```
Here's what migration does:
1. Checks that `version == 1`.
2. Copies the dir to `.gitpass-backup-v1-<ts>/` (git-ignored).
3. Decrypts everything into zeroizing memory.
4. Re-encrypts into `.gitpass-migrate-tmp/`: names go into `meta.enc`, items get padding, and the manifest is built.
5. Writes a recovery marker and renames the new files into place.
6. Writes `format.json` (version 2) **last**.
7. Makes a single git commit.

If it's interrupted, `format.json` still says v1. Rerun `gitpass migrate`, and it restores from the backup and starts over. Old (0.1.x) clients refuse version 2. Once one device has migrated, upgrade gitpass on the others too. The KDF auto-upgrade to 256 MiB happens on the first unlock after migration.

## Security notes
- **Git history is forever.** `passwd` and `rotate-secret-key` only re-wrap `keyset.enc`. The old `keyset.enc` stays in git history. If someone has the old password and the old Secret Key, they can still open the **old Account Key** from history. That key unwraps the old `vault.key.enc` files, and the vault keys inside are unchanged unless you rotate them. **To recover from a leaked password + Secret Key, run `gitpass rotate-account-key --with-vault-keys`** (and change the leaked password). The first part gives you a new Account Key, so old keysets can't unwrap the current `vault.key.enc` files or the manifest. The second part gives you new Vault Keys, so old vault keys from history can't decrypt items written after rotation. Ciphertext that's already in history stays readable to that attacker, so rotate the actual site passwords too.
- `rotate-account-key` isn't a single atomic step: it rewrites N vault key files plus the keyset. It takes a full backup (`.gitpass-backup-<ts>/`) first. If it's interrupted, restore from that backup. `vault rotate` builds the new vault in a temp dir and swaps the directories.
- Remaining metadata leaks: the number of vaults and items, UUIDs, the padded size bucket, and commit times.
- If you edit concurrently on two devices, the `manifest.enc` conflicts during `pull --rebase` must be resolved by hand. Keep one side, then run `gitpass --accept-rollback unlock` to re-seal. A backup is taken before every pull.
- `get` prints passwords to stdout. Clipboard support isn't implemented.
- **Back up the Secret Key** (Emergency Kit). If you lose either secret, your data is unrecoverable.
- `GITPASS_PASSWORD`, `GITPASS_NEW_PASSWORD` and `GITPASS_ITEM_PASSWORD` are for scripts and tests only.
