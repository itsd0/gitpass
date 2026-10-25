# gitpass — zero-knowledge password manager synced through a private GitHub repo (MVP)

A clean-room Rust CLI. Its design follows the *publicly documented* two-secret model used by 1Password: a master password plus a Secret Key, with end-to-end encryption. It contains no proprietary code. It only uses vetted RustCrypto crates (`argon2`, `hkdf`, `sha2`, `chacha20poly1305`, `zeroize`, `rand`).

> **MVP: NOT audited.** Don't trust it with real secrets until it has been reviewed independently.

## Build
```bash
cargo build --release      # binary: target/release/gitpass
cargo test
```

## Key hierarchy (two-secret model)
```
pw_key = Argon2id(master_password, salt16)            m=64 MiB, t=3, p=4 -> 32 B
sk_key = HKDF-SHA256(ikm=SecretKey(128 bit), salt=account_id, info="gitpass-sk-v1") -> 32 B
AUK    = pw_key XOR sk_key
AUK  --wraps-->  Account Key (random 32 B)      keyset.enc            AAD "<account_id>|keyset|1"
Account Key --wraps--> Vault Key (random 32 B)  vaults/<id>/vault.key.enc  AAD "<vault_id>|vault-key|1"
Vault Key --encrypts--> item JSON               vaults/<id>/items/<id>.enc AAD "<vault_id>|<item_id>|<version>"
```
All encryption uses XChaCha20-Poly1305 with a random 24-byte nonce. Key material sits in `Zeroize` types, so it is wiped on drop.
Because the item AAD includes the item version, an attacker can't swap item files between slots or roll back the version counter without decryption failing.
Someone who steals the repo still needs **both** secrets. The 128-bit Secret Key makes offline guessing of the password infeasible, even if the password is weak.

Secret Key format: `A3-XXXXXX-XXXXXX-XXXXX-XXXXX-XXXXX`. That is 26 base32 characters (128 bits) plus 1 checksum character, which catches typos.

## Layout
```
format.json   keyset.enc   .gitignore (ignores secret.key)
vaults/<uuid>/vault.key.enc  meta.json (plaintext vault name)  items/<uuid>.enc
```

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
gitpass sync
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

## Security notes
- **Git history is forever.** Deleted or rotated items stay in history as ciphertext. If a secret is compromised, rotate it at the source. Rewriting git history doesn't remove copies other people already cloned.
- **Back up the Secret Key** (print the Emergency Kit). If you lose either secret, your data is unrecoverable. There is no reset.
- **Argon2 memory:** the MVP uses 64 MiB. Desktop/production target is **256 MiB** (`m_kib: 262144` in `format.json` for new accounts).
- Metadata leaks: vault names, item counts, item IDs, file sizes and commit times are visible to anyone who can read the repo.
- `get` prints passwords to stdout, so they can end up in terminal scrollback. Clipboard support and auto-clear aren't implemented yet.
- No password change or Secret Key rotation yet. Merge conflicts on the same item during `pull --rebase` must be resolved by hand.
