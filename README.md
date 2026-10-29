# gitpass: zero-knowledge password manager synced through a private git repo (format v4)

A clean-room Rust password manager that follows the publicly documented two-secret model (master password + Secret Key). It is end-to-end encrypted and syncs through git. It includes real device revoke with key rotation, vault sharing (X25519),
import/export, a security audit with optional HIBP, a Tauri desktop app, an iOS app, and a browser extension with a native messaging host.

> **NOT audited.** Don't trust it with real secrets until it has been reviewed independently.

## Workspace layout
```
Cargo.toml                 # workspace root; excludes src-tauri
crates/gitpass-core/       # library: crypto, store (v4), keys (X25519), device, merge, sync, migrate, import, audit, totp, search
crates/gitpass-cli/        # binary `gitpass` (CLI, secret injection, import/export, audit, SSH agent)
crates/gitpass-host/       # binary `gitpass-host`: native messaging host for the extension
crates/gitpass-pasteboard/ # concealed/transient pasteboard copies (macOS NSPasteboard / Linux fallback)
apps/desktop/gui-core/     # Tauri-free GUI command layer (root workspace member)
apps/desktop/src-tauri/    # Tauri 2 app (own Cargo.lock + .cargo/config.toml; NOT in root workspace)
apps/desktop/src/          # React + TypeScript frontend (Vite)
extension/                 # MV3 browser extension (plain JS, `npm test`)
ios/rust/gitpass-ios-ffi/  # UniFFI bridge between gitpass-core and the Swift iOS app
ios/                       # iOS SwiftUI app and AutoFill credential provider extension
```

## Install
### CLI
```bash
cargo build --release -p gitpass-cli      # target/release/gitpass
cargo test --workspace
```
### Desktop
```bash
# Linux deps: libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev pkg-config
cd apps/desktop && npm ci && npm run build
npx tauri build --bundles deb             # or: npx tauri dev
```
### Browser extension
```bash
cargo build --release -p gitpass-host
# Install the host at a stable path (outside target/); on macOS sign it with the local identity (scripts/macos-local-sign.md)
D="$HOME/Library/Application Support/gitpass/bin"; mkdir -p "$D"
cp target/release/gitpass-host "$D/" && codesign -f -s "gitpass Local Signing" -i app.gitpass.host -o runtime "$D/gitpass-host"
# Chromium: chrome://extensions > Developer mode > Load unpacked > extension/, copy the ID, then
"$D/gitpass-host" install --browser chrome --extension-id <id> --vault ~/.gitpass
# Firefox: (cd extension && npm run build:firefox), then about:debugging > Load Temporary Add-on > extension/dist/firefox/manifest.json
"$D/gitpass-host" install --browser firefox --extension-id gitpass@gitpass.dev
```
`extension/manifest.json` is the Chrome (MV3 service worker) manifest; `extension/manifest.firefox.json` is the Firefox one (`background.scripts`).
User guide: [`docs/EXTENSION.md`](docs/EXTENSION.md) covers the popup, inline and right-click menus, keyboard commands, the save banner, passkeys, and what the extension never stores.

Unlocking from the popup: enter the master password and, if there's no `<vault>/secret.key`, your Secret Key. The popup sends both
only to the background worker, which passes them over native messaging to `gitpass-host`. Content scripts never see them. The host uses the
Secret Key once to derive the session keys and then zeroizes it. Nothing is written to disk, and the extension doesn't store it. After Lock, an idle
auto-lock, or a browser restart, you need to enter it again. Unlocking from the extension doesn't write to the vault repo.
See `crates/gitpass-host/README.md` and `apps/desktop/README.md` for details, and CHANGELOG.md for the limitations of each feature.

## Format compatibility (0.4.0)
- Formats 2, 3 and 4 are read (`MIN_READ_FORMAT = 2`, `FORMAT_VERSION = 4`). The manifest becomes 4 only once an identity or vault members exist.
  0.3.x clients refuse v4 vaults.

## Format compatibility (0.3.0 notes)
- v2 vaults open unchanged. First unlock creates `devices.enc` and the next write re-seals the manifest as format 3.
- v2 item payloads have no clocks; clocks are synthesized from `updated_at` and written on the next update/merge.
- `format.json` stays at 2 until the keyset is re-wrapped (passwd / rotate-secret-key / KDF upgrade / v1 migrate),
  because the keyset AAD binds format.json; then it becomes 3. Formats newer than 3 are refused.

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
gitpass add personal --category api-credential --title "OpenAI API" --url https://api.openai.com
gitpass list personal
gitpass get personal github [--password-only]     # query = id, id prefix or title substring
gitpass edit personal <id|query> --notes "..." [--password | --generate 20]
gitpass rm personal <id|query>
gitpass gen 32               # random password (default 24; --no-symbols, --no-digits)
gitpass gen --words 5 [--separator .] [--capitalize]   # memorable passphrase (128-word list)
gitpass gen --pin 6          # digits-only PIN
gitpass gen --copy 32 [--clear-secs 30]   # copy as concealed/transient; never printed
gitpass sync                 # lock, commit, backup, pull --rebase, push, then verify the manifest
gitpass vault create work --description "work logins"   # name + description are encrypted
gitpass passwd               # new master password (GITPASS_NEW_PASSWORD for scripts)
gitpass rotate-secret-key [--save-secret-key]   # new Secret Key + new Emergency Kit
gitpass rotate-account-key [--with-vault-keys]  # new Account Key; re-wraps all vault keys
gitpass vault rotate work    # new Vault Key; re-encrypts every item in the vault
gitpass migrate              # v1 -> v2 (also offered automatically on unlock; --yes to auto-accept)
gitpass --accept-rollback unlock   # accept a detected rollback/tamper and re-seal (dangerous)
```
Every subcommand, including `history`, `restore`, `import`/`export`, `ssh`, `identity` and
`remote`, is listed with a one-line example in [docs/CLI.md](docs/CLI.md).

Options and environment variables:
- `--path DIR` or `GITPASS_PATH` sets the vault dir (default `~/.gitpass`).
- The Secret Key is read in this order: `GITPASS_SECRET_KEY`, then `<path>/secret.key` (mode 0600, git-ignored, written by `init --save-secret-key`), then an interactive prompt.
- `GITPASS_PASSWORD` and `GITPASS_ITEM_PASSWORD` supply passwords without prompting. They are meant for scripts and tests only, because environment variables can leak.
- `GITPASS_IMPORT_PASSWORD` supplies the export password for a password-protected Bitwarden JSON import (`gitpass import --from bitwarden-json`); the desktop Import wizard asks for it in a password step.

## Shell completions

Generate tab completions for `zsh`, `bash`, or `fish` directly to stdout:

```bash
# zsh (place in ~/.zfunc or any directory on your $fpath)
mkdir -p ~/.zfunc
gitpass completions zsh > ~/.zfunc/_gitpass
# Ensure ~/.zshrc contains:
#   fpath=(~/.zfunc $fpath)
#   autoload -Uz compinit && compinit

# bash
mkdir -p ~/.local/share/bash-completion/completions
gitpass completions bash > ~/.local/share/bash-completion/completions/gitpass
# or source on demand in ~/.bashrc:
#   source <(gitpass completions bash)

# fish
mkdir -p ~/.config/fish/completions
gitpass completions fish > ~/.config/fish/completions/gitpass.fish
```

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

## Secrets in commands and config files: `gitpass run` / `gitpass inject`

Reference a field as `gp://<vault>/<item>/<field>`. `<item>` is an item id, an exact title
(case-insensitive) or a unique fuzzy match; `<field>` is `password`, `username`, `url`, `notes`,
`title`, `totp` (current code) or the label/id of a custom field. Percent-encode `/` (`%2F`), spaces
(`%20`) and `%` (`%25`) inside a segment.

```sh
# Inherited environment variables
export GITHUB_TOKEN=gp://Personal/GitHub/token
gitpass run -- gh repo list

# Or dotenv syntax via --env-file (optional; overrides inherited variables of the same name)
# .env.gitpass (`#` comments, optional quotes, `export ` prefix allowed)
DB_PASSWORD=gp://Dev/Postgres/password
DATABASE_URL="postgres://{{ gp://Dev/Postgres/username }}:{{ gp://Dev/Postgres/password }}@db:5432/app"
LOG_LEVEL=info

gitpass run --env-file .env.gitpass -- ./server --port 8080
```

* `--env-file` is optional. If omitted, `gitpass run` scans inherited environment variables and
  resolves any variable whose value is (or contains) a `gp://` reference. Values from `--env-file`
  override inherited environment variables for the same name. If no `--env-file` is given and no
  inherited variable contains a `gp://` reference, gitpass exits non-zero with an error instead of
  running the command unchanged.
* A value that is a whole `gp://` reference is resolved; references embedded in other text must be
  wrapped as `{{ gp://... }}` (an unwrapped embedded reference is an error).
* Resolved values go **only** into the child's environment; nothing is written to disk. Any
  reference that fails to resolve aborts before the child starts.
* The child's stdout/stderr are piped through a masker that replaces every resolved value with
  `<concealed by gitpass>` (also when a value is split across writes). To do that the masker holds
  back up to `len(longest secret) - 1` bytes until more output arrives, so a prompt without a
  trailing newline may appear slightly late. `--no-masking` connects the child directly to the
  terminal. stdin is inherited. The child's exit code is returned (128+N for signal N).
* Limits: masking cannot stop the child from writing a secret to a file, sending it over the
  network, or printing it transformed (base64, split by characters, etc.). The child's environment
  is visible to same-user processes (e.g. `ps eww` on some systems).

```sh
# Stack multiple env files (e.g. shared settings + secret overrides)
gitpass run --env-file .env.common --env-file .env.gitpass -- ./server --port 8080
```

* Supported fields: standard fields (`password`, `username`, `url`, `notes`, `title`, `totp`/`otp`
  for generating current one-time codes) and custom section fields (by label or id, e.g. `secret_key`).
* If multiple items share the same title, gitpass aborts with an error indicating collision; reference
  the item by its unique item ID instead (`gp://<vault>/<item_id>/<field>`).
* Pre-flight safety: references are resolved atomically before the child process is launched or any
  file is written. If any vault, item, or field is missing (or empty), gitpass exits with code 1 and
  does not run the command. Error messages never disclose secret values.

`gitpass inject` renders templates, substituting `{{ gp://<vault>/<item>/<field> }}` placeholders:

```sh
# config.yml.tpl or .env.template:
#   APP_ENV=production
#   user: {{ gp://Dev/Postgres/username }}
#   pass: {{ gp://Dev/Postgres/password }}
#   api_key: {{ gp://Dev/Stripe/secret_key }}
#   untouched: {{ PASSTHROUGH_VAR }}

gitpass inject -i config.yml.tpl                 # rendered to stdout
gitpass inject -i config.yml.tpl -o config.yml   # written with mode 0600 (plaintext secrets on disk!)
gitpass inject -i .env.template -o .env          # generate .env file with mode 0600
```
`{{ ... }}` blocks that do not contain a `gp://` reference are left untouched.

## Verifying sharing and revocation

`crates/gitpass-core/tests/share_git.rs` (`cargo test -p gitpass-core --test share_git`) runs this
flow automatically with two accounts in separate temp dirs and a temp bare git repo. By hand:

```sh
T=$(mktemp -d); git init --bare -q -b main $T/remote.git
export GITPASS_PASSWORD='fake-master-pw-123'
A="gitpass --yes --path $T/alice --device-name alice"; B="gitpass --yes --path $T/bob --device-name bob"  # --yes skips confirmations
$A init --save-secret-key; $B init --save-secret-key
$A vault create team; GITPASS_ITEM_PASSWORD=s3cret $A add team --title db
$B identity show             # prints Bob's fingerprint; writes $T/bob/identity.pub
$A vault share team --to $T/bob/identity.pub   # verify the fingerprint out of band
# Every share is Ed25519-signed by the sender (signing key derived from the
# sender identity secret). `vault accept` rejects unsigned or invalidly
# signed shares. On first contact with a sender, also verify the *signing*
# fingerprint out of band — accept prints it — since the signing key travels
# inside the member file itself. Pre-signature shares are rejected, not
# grandfathered: ask the sender to re-share with a current build.
$A remote add $T/remote.git --allow-local       # pushes the encrypted vault
git clone -q $T/remote.git $T/alice-clone
$B vault accept $T/alice-clone/vaults/<vault-id>/members/<bob-fpr>.enc
$B get team db --password-only                  # -> s3cret
$A vault unshare team <bob-fpr>                  # rotates the vault key, drops Bob's member file
GITPASS_ITEM_PASSWORD=after $A add team --title new; $A sync
git -C $T/alice-clone pull -q                    # members/<bob-fpr>.enc is gone; items re-encrypted
```
After revocation Bob still has his previously accepted snapshot (anything already copied cannot be
"un-shared"), but his key cannot decrypt the new item, the re-encrypted old items, or the vault meta,
and the vault's `key_version` has increased.

## Security notes
- **Git history is forever.** `passwd` and `rotate-secret-key` only re-wrap `keyset.enc`. The old `keyset.enc` stays in git history. If someone has the old password and the old Secret Key, they can still open the **old Account Key** from history. That key unwraps the old `vault.key.enc` files, and the vault keys inside are unchanged unless you rotate them. **To recover from a leaked password + Secret Key, run `gitpass rotate-account-key --with-vault-keys`** (and change the leaked password). The first part gives you a new Account Key, so old keysets can't unwrap the current `vault.key.enc` files or the manifest. The second part gives you new Vault Keys, so old vault keys from history can't decrypt items written after rotation. Ciphertext that's already in history stays readable to that attacker, so rotate the actual site passwords too.
- `rotate-account-key` isn't a single atomic step: it rewrites N vault key files plus the keyset. It takes a full backup (`.gitpass-backup-<ts>/`) first. If it's interrupted, restore from that backup. `vault rotate` builds the new vault in a temp dir and swaps the directories.
- Remaining metadata leaks: the number of vaults and items, UUIDs, the padded size bucket, and commit times.
- If you edit concurrently on two devices, the `manifest.enc` conflicts during `pull --rebase` must be resolved by hand. Keep one side, then run `gitpass --accept-rollback unlock` to re-seal. A backup is taken before every pull.
- `get` prints passwords to stdout. Clipboard support isn't implemented.
- **Back up the Secret Key** (Emergency Kit). If you lose either secret, your data is unrecoverable.
- `GITPASS_PASSWORD`, `GITPASS_NEW_PASSWORD` and `GITPASS_ITEM_PASSWORD` are for scripts and tests only.

## Device management (format v3)

Every successful unlock registers this device (or refreshes `last_seen_at`) in an encrypted registry.

- `devices.enc` (vault root, synced): XChaCha20-Poly1305 envelope, key `hkdf32(account_key, None, "gitpass/devices/v3")`,
  AAD `"{account_id}|devices|3"`. Plaintext `{ "format": 3, "devices": [{id, name, created_at, last_seen_at, revoked, revoked_at}] }`.
- Its blake3 hash is stored in the authenticated manifest as `devices_hash`; the manifest `format` becomes 3.
  `format.json`/`keyset.enc` are unchanged (still v2). v2 manifests (no `devices_hash`) still open; the registry is created on first unlock.
- `.gitpass-device.json` (git-ignored, local only) holds this checkout's stable device id.
- Name: `--device-name`, else `$GITPASS_DEVICE_NAME`, else hostname.

```
gitpass device list
gitpass device rename <id|prefix> <name>
gitpass device revoke <id|prefix>      # asks for confirmation unless --yes
gitpass device unrevoke <id|prefix>    # from another, non-revoked device
```

A revoked device can still unlock and run `device list` and `rotate-account-key`. Every other mutating operation is refused.

**Limitation:** revoke is only a trust marker that cooperating clients enforce. It does **not** re-encrypt anything.
A device that has the master password and Secret Key (or has stored the Account Key) can still decrypt data and could
use a modified client to ignore the marker. After revoking a lost or compromised device, run
`gitpass rotate-account-key --with-vault-keys`, then `gitpass passwd` and `gitpass rotate-secret-key`.
## Format v3: field-level merge
Item plaintext = `{title, username, password, url, notes, totp, updated_at, clocks:{<field>: u64 ms}}` (inside the same
padded, encrypted envelope). Merge per field: higher clock wins; equal clock + equal value = clean; equal clock + different
value = conflict, local kept and reported; `None` with a clock is a tombstone. v2 items read with all clocks = `updated_at`*1000.
`gitpass sync --merge` applies this to every item after fetching when local and remote histories diverged.
## Usability: clipboard, TOTP, fuzzy search

- `gitpass copy <vault> <query> [--field password|username|url|totp|notes|title] [--pick N] [--clear-secs 30] [--quiet]`
  copies a field to the clipboard, **blocks** for `--clear-secs` (default 30; the process must stay alive to own
  the X11/Wayland selection), then clears it (only if it still holds our value). `--clear-secs 0` copies and exits
  without clearing. If no clipboard is available (headless), prints a warning and the value to stdout unless `--quiet`.
  `get` accepts the same `--copy --field --clear-secs --quiet` flags. On macOS, copied passwords, TOTP codes and
  other secrets carry the nspasteboard.org concealed/transient markers so clipboard managers and Handoff skip them;
  usernames, URLs, notes and titles are copied as plain text.
- `gitpass totp <vault> <query> [--pick N] [--copy] [--clear-secs 30]` prints the current RFC 6238 code and seconds
  remaining. `Item.totp` may be an `otpauth://totp/...?secret=...` URI (digits/period/algorithm SHA1|SHA256|SHA512
  honoured) or a raw base32 secret.
- `gitpass find <vault> <query>` ranked fuzzy search over title/username/url (exact > prefix > word > substring > subsequence).
- `gitpass list <vault> --query <q>` ranked filter. `get` with several matches lists ranked candidates and exits 1
  unless one is a clear winner or `--pick <n>` is given.
- Build without clipboard support: `cargo build --no-default-features`.

## Scripting with --json

`gitpass list`, `gitpass find` and `gitpass get` support `--json` output for automation and shell scripting:

- `gitpass list <vault> [--query <q>] --json` and `gitpass find <vault> <q> --json` print a JSON array of item summaries (`id`, `title`, `category`, `username`, `url`, `tags`, `favorite`, `updated_at`). No secrets (password, TOTP seed, concealed fields, notes) appear in list/find output.
- `gitpass get <vault> <query> --json` prints an item object with metadata and its fields (`label`, `kind`, `value`). Concealed values (passwords, TOTP seeds, secret custom fields) are `null` unless `--reveal` is also passed. `--json --password-only` is rejected with an error.
- Ambiguous queries keep `--pick <n>` behavior. In `--json` mode, ambiguity errors go to stderr with non-zero exit so stdout stays clean.
- Archived and deleted items are excluded by default unless `--include-archived` is passed.

Example with `jq`:

```bash
# Find an item ID by title and extract its revealed password
ITEM_ID=$(gitpass list Personal --query "github" --json | jq -r '.[0].id')
PASSWORD=$(gitpass get Personal "$ITEM_ID" --json --reveal | jq -r '.fields[] | select(.label == "password") | .value')

# List usernames for all login items
gitpass list Personal --json | jq -r '.[] | select(.category == "login") | "\(.title): \(.username)"'
```

## Security audit (Watchtower)

`gitpass audit [VAULT] [--max-age-days N] [--hibp]` checks live login items for weak, reused and old
passwords, missing TOTP, passwords containing the item's username or site name, and suggests TOTP/passkeys
from a bundled offline site list. `--hibp` opts in
to a k-anonymity breach check (only 5 hex characters of a SHA-1 leave the machine). The desktop app's
Watchtower shows the same findings plus a 0–100 score.

How the entropy estimate, reuse detection and score weighting work: [docs/WATCHTOWER.md](docs/WATCHTOWER.md).

## Contributing

Recommended prompt for any AI agent:

> "You are working on itsd0/gitpass. First read AGENTS.md and CONTRIBUTING.md in the repo root and follow every rule in them, and contribute to the repo."

See [AGENTS.md](AGENTS.md) (full workflow, commands, environment matrix, rules) and [CONTRIBUTING.md](CONTRIBUTING.md).
