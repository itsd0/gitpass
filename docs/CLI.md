# CLI reference

`gitpass` is the command-line client for a gitpass vault. `gitpass --help` lists the
top-level commands and `gitpass <command> --help` shows the flags of one command; this
page has a one-line example for every subcommand. The examples and flags track the clap
definitions in `crates/gitpass-cli/src/main.rs`, and a test (`crates/gitpass-cli/tests/cli_docs.rs`)
fails if a visible subcommand is missing here.

The vault lives in `~/.gitpass` unless `--path DIR` or `GITPASS_PATH` says otherwise.
Commands that open the vault ask for the master password and read the Secret Key from
`GITPASS_SECRET_KEY`, `<path>/secret.key`, or a prompt, in that order.

A `<vault>` argument is a vault name or vault id. An item query is an item id, a unique
id prefix, or a title substring; `get`, `copy`, `totp`, `find` and `list --query` also
rank fuzzy matches on title/username/url. When a query is ambiguous, use the item id or
`--pick <n>` where the command offers it.

## Global options

| Option | Meaning |
|---|---|
| `--path <DIR>` | Vault directory (default `~/.gitpass`; env `GITPASS_PATH`) |
| `--yes` | Answer yes to confirmation prompts (v1 migration, share, revoke, plaintext export) |
| `--accept-rollback` | Accept a failed integrity/rollback check and re-seal the manifest from disk (dangerous) |
| `--device-name <NAME>` | Device name for the registry (default `$GITPASS_DEVICE_NAME`, else hostname) |

Global options can be given before or after the subcommand, for example
`gitpass --path /tmp/vault list Personal` or `gitpass list Personal --path /tmp/vault`.

## Account and keys

```sh
gitpass init --save-secret-key                     # create an account; prints the Emergency Kit
gitpass unlock                                     # verify master password + Secret Key
gitpass passwd                                     # change the master password
gitpass rotate-secret-key --save-secret-key        # new Secret Key + new Emergency Kit
gitpass rotate-account-key --with-vault-keys       # recovery from a leaked password + Secret Key
gitpass migrate                                    # migrate a format v1 vault, with a backup first
```

`init` and `rotate-secret-key` read `GITPASS_PASSWORD`; `passwd` reads
`GITPASS_NEW_PASSWORD`. Otherwise they prompt. `rotate-account-key` rewrites the keyset
(and with `--with-vault-keys`, every vault key and item). Read the security notes in the
README before relying on rotation.

## Vaults

```sh
gitpass vault create Personal --description "personal logins"
gitpass vault list
gitpass vault rename Personal Home
gitpass vault rm Home --yes                        # alias: vault delete; without --yes, type the name
gitpass vault rotate Personal                     # new Vault Key; re-encrypts every item
gitpass vault share Personal --to bob-identity.pub
gitpass vault accept ../alice-clone/vaults/<vault-id>/members/<your-fpr>.enc
gitpass vault unshare Personal <fingerprint>
gitpass vault members Personal
```

Sharing and revocation are described in the README ("Verifying sharing and revocation").
A share is a one-time snapshot; `unshare` rotates the vault key but cannot recall what the
member already copied.

## Items

```sh
gitpass add Personal --title GitHub --username me --url https://github.com --generate 24
gitpass add Personal --title Bank                  # prompts for the item password
gitpass add Personal --category api-credential --title "OpenAI API" --url https://api.openai.com --expires 2028-06-30
gitpass add Personal --category secure-note --title "Recovery Guide" --notes-stdin < recovery.txt
gitpass add Personal --category credit-card --title "Bank Visa" --holder "Alice Doe" --expiry 11/30
gitpass add Personal --category bank-account --title "Chase Checking" --bank "Chase" --holder "Alice Doe" --account-type checking --routing 021000021
gitpass add Personal --category server --title "Production Web" --url 192.168.1.50 --username ubuntu --admin-url https://aws.amazon.com/console
gitpass get Personal github
gitpass get Personal github --password-only
gitpass get Personal github --json --reveal
gitpass get Personal github --copy --field password --clear-secs 30
gitpass copy Personal github --field username
gitpass list Personal
gitpass list Personal --category bank-account  # filter by category
gitpass list Personal --category server
gitpass list Personal --sort recent   # title (default) or recent; ignored with --query
gitpass list Personal --query git --json
gitpass find Personal "git hub"
gitpass edit Personal github --notes "2FA enabled"
gitpass edit Personal github --generate 32
gitpass rm Personal github
gitpass history github
gitpass history github --vault Personal --json
gitpass restore github --version 3
```

- `add` creates an item in the vault. `--category` (alias `--type`) chooses the category:
  `login` (default), `api-credential`, `secure-note`, `credit-card`, `bank-account`, or `server`.
  Secrets are never accepted as command-line arguments to avoid shell-history leaks; `add` prompts
  without echo, or accepts stdin flags (`--password-stdin`, `--credential-stdin`, `--notes-stdin`,
  `--account-number-stdin`, `--pin-stdin`).
- `get` prints the item, or only the password with `--password-only`. `--json` prints the
  item with concealed values set to `null` unless `--reveal` is also passed; see the
  README section "Scripting with --json".
- `get --copy` and `copy` block for `--clear-secs` (default 30) while the clipboard is
  owned, then clear it; `--clear-secs 0` copies without waiting. `--quiet` never echoes
  the value when no clipboard is available.
- `--field` takes `password`, `username`, `url`, `totp`, `notes` or `title`.
- `edit` and `rm` take an item query; `edit` replaces only the fields you pass, and
  `--password` / `--generate [N]` set a new password.
- `history` lists versions without secrets (current version first); `restore --version N`
  restores a previous version as a new version. Both accept `<vault>/<item>` instead of
  `--vault` and refuse to guess when several items match.
- `list`, `find`, `get` and `get --json` skip archived and deleted items unless
  `--include-archived` is passed.

## Attachments

```sh
gitpass attach add Personal github ./recovery-codes.pdf
gitpass attach ls Personal github
gitpass attach ls Personal github --json
gitpass attach get Personal github recovery-codes.pdf
gitpass attach get Personal github recovery-codes.pdf -o ~/Desktop/codes.pdf --force
gitpass attach rm Personal github recovery-codes.pdf
```

- Files are encrypted under the vault key; the limit is 25 MiB per attachment.
- `get` and `rm` take an attachment id, unique id prefix, or name (ambiguous names need the id).
- Without `-o`, `get` writes into the current directory using only the final component of the
  stored name (a name like `../x` or `/etc/x` is saved as `x`), and refuses to overwrite an
  existing file unless `--force` is passed.
- `rm` deletes the encrypted file and records a tombstone so a concurrent edit synced from another
  device does not bring the attachment back.

## TOTP and password generator

```sh
gitpass totp Personal github
gitpass totp Personal github --copy --clear-secs 30
gitpass gen 32
gitpass gen 32 --no-history
gitpass gen history
gitpass gen history --reveal
gitpass gen history --json
gitpass gen history clear
gitpass gen --words 5 --separator . --capitalize
gitpass gen --pin 6
gitpass gen --copy 32 --clear-secs 30
```

`gen` defaults to a 24-character random password (8-128 characters). `--words N` makes a
passphrase from the 128-word list (3-20 words, joined with `-`, or `.` with `--no-symbols`),
`--pin N` a digits-only PIN (4-16 digits); `--no-symbols` and `--no-digits` apply to random
passwords. A length outside these ranges is an error (the desktop app and the browser host
use the same limits and defaults). `--clear-secs` requires `--copy`.

`gitpass gen` automatically records generated passwords to an encrypted vault history
unless `--no-history` is provided. `gitpass gen history` lists recently generated passwords
(masked by default; use `--reveal` or `--json` to view), and `gitpass gen history clear` clears it.

## Secrets in commands and templates

```sh
export GITHUB_TOKEN=gp://Personal/GitHub/token
gitpass run -- gh repo list
gitpass run --env-file .env.gitpass -- ./server --port 8080
gitpass run --env-file .env.common --env-file .env.gitpass -- ./server
gitpass inject -i config.yml.tpl                  # render to stdout
gitpass inject -i .env.template -o .env           # write mode 0600 (plaintext secrets on disk)
```

References are `gp://<vault>/<item>/<field>`; `run` resolves references from inherited
environment variables and/or `--env-file` (which is optional and overrides inherited
variables with the same name), puts resolved values only in the child's environment, and
masks them in its output. If no `--env-file` is given and no inherited variable contains a
`gp://` reference, `run` exits non-zero with an error. `inject` replaces
`{{ gp://... }}` placeholders. See the README sections "Secrets in commands and config
files" for the exact syntax, masking limits and `--no-masking`.

## Git sync

```sh
gitpass remote add git@github.com:YOU/gitpass-vault.git
gitpass remote show
gitpass remote remove
gitpass sync
gitpass sync --merge
```

`remote add` expects an existing EMPTY private repo, uses your git credentials and never
force-pushes. `sync` commits the encrypted files, pulls with `--rebase`, pushes, then
verifies the manifest; `--merge` field-merges items when local and remote histories
diverged. See the README "GitHub sync" section.

## Import and export

```sh
gitpass import Personal --from bitwarden-json export.json
gitpass import Personal --from 1password-csv export.csv --dry-run
gitpass import Personal --from gitpass-json backup.json --allow-duplicates
gitpass export Personal --format gitpass-json --out backup.json --encrypted
gitpass export Personal --format csv --out backup.csv --yes
```

`--from` accepts every format and alias in `crates/gitpass-core/src/import.rs`
(`ImportFormat`): `bitwarden-json`, `bitwarden-csv`, `1password-csv`,
`1password-1pux` (alias `1pux`), `keepass-csv` (alias `keepassxc-csv`),
`chrome-csv`, `apple-passwords-csv` (aliases `apple-passwords`, `apple-csv`,
`apple`, `safari-csv`, `safari`), `lastpass-csv` (alias `lastpass`),
`protonpass-csv` (aliases `proton-pass-csv`, `protonpass`, `proton-pass`) and
`gitpass-json`.
Duplicates (same title+username+url) are skipped unless `--allow-duplicates` is given;
`--dry-run` writes nothing. An encrypted `gitpass-json` export prompts for the passphrase
(or reads `GITPASS_EXPORT_PASSPHRASE`).

A **password-protected Bitwarden JSON** export (Bitwarden → Export → `.json` → "Password
protected") is decrypted in place: gitpass derives the key from the export password with the
file's KDF settings (PBKDF2-SHA256 or Argon2id), verifies the file's key-validation blob and
HMAC before decrypting, and prompts for the export password (or reads
`GITPASS_IMPORT_PASSWORD`). Wrong passwords, MAC failures and out-of-range KDF parameters are
reported without exposing the password or any key material. Bitwarden exports encrypted with
your **account key** are not supported; re-export with "Password protected" or unencrypted.

`export` writes **plaintext** unless `--encrypted` is given, warns, and requires a
confirmation or `--yes`; the file is mode 0600. `--encrypted` is only valid with
`--format gitpass-json` and uses Argon2id + XChaCha20-Poly1305 with
`GITPASS_EXPORT_PASSPHRASE` or a prompt.

## SSH keys

```sh
gitpass ssh generate Personal --title "GitHub deploy key" --comment me@example.invalid
gitpass ssh import Personal ~/.ssh/id_ed25519 --title "Laptop key"
gitpass ssh list
gitpass ssh list Personal
gitpass ssh public Personal "GitHub deploy key"
gitpass ssh config
gitpass ssh agent                                  # foreground; the desktop app runs its own
gitpass ssh git-signing Personal "GitHub deploy key"
```

`ssh generate` creates an Ed25519 key in a new SSH Key item. `ssh import` reads an
OpenSSH private key (Ed25519, ECDSA P-256/P-384, or RSA 2048+ bits; RSA under 2048 bits and DSA keys
are rejected; a passphrase comes from `GITPASS_SSH_PASSPHRASE` or a prompt). The SSH agent signs
RSA requests with `rsa-sha2-256` or `rsa-sha2-512` only (plain SHA-1 `ssh-rsa` is refused). `ssh public` prints the
`authorized_keys` line; `ssh config` prints the `~/.ssh/config` `IdentityAgent` snippet
(both `config` and `agent` accept `--socket PATH`).

### Git commit signing

```sh
gitpass ssh git-signing Personal "GitHub deploy key"               # print Git signing config (default)
gitpass ssh git-signing Personal "GitHub deploy key" --apply --yes # apply to ~/.gitconfig and allowed_signers
gitpass ssh git-signing Personal "Deploy key" --local --apply --yes # apply to local repo (.git/config)
```

`ssh git-signing` configures Git to sign commits and tags with an SSH key stored in gitpass (Ed25519 or RSA):
- Sets `gpg.format=ssh`
- Sets `user.signingkey="key::<public key>"`
- Sets `commit.gpgsign=true` and `tag.gpgsign=true`
- Appends `<email> <public key>` to `allowed_signers` (default `~/.ssh/allowed_signers`)
- Points `gpg.ssh.allowedSignersFile` at the allowed signers file
- Includes `export SSH_AUTH_SOCK="<path>"` so your shell can communicate with the background gitpass SSH agent

Because Git commit signing invokes `ssh-keygen -Y sign` (which communicates with an agent via `$SSH_AUTH_SOCK` rather than `ssh`'s `IdentityAgent`), `SSH_AUTH_SOCK` in your shell profile (`~/.zshrc` or `~/.bashrc`) must point to the gitpass agent socket. `gitpass ssh git-signing` accepts `--socket <PATH>`, and `--apply` warns if `SSH_AUTH_SOCK` is currently pointing elsewhere.

`--print` (the default) only prints the configuration lines without writing. Applying to git config (`--apply` or `--write`) requires confirmation unless `--yes` is passed. Signing requests go through the gitpass SSH agent, preserving per-use approval and logging.

## Devices and identity

```sh
gitpass identity show                              # public key + fingerprint (creates it on first use)
gitpass device list
gitpass device rename <id-prefix> "MacBook"
gitpass device revoke <id-prefix>
gitpass device revoke <id-prefix> --marker-only
gitpass device revoke <id-prefix> --rotate-credentials --save-secret-key
gitpass device unrevoke <id-prefix>
```

`device revoke` rotates the Account Key and every Vault Key by default, so the revoked
device cannot read future changes; `--marker-only` keeps the old marker-only behaviour.
Add `--rotate-credentials` if the device knew the master password and Secret Key. It asks
for confirmation unless `--yes` is passed. Data the device already copied cannot be
recalled. See the README "Device management".

## Security audit

```sh
gitpass audit
gitpass audit Personal --max-age-days 180
gitpass audit --hibp --json
gitpass audit --include-ignored
```

`audit` checks live items: weak, reused and old passwords, missing TOTP, insecure URLs
and duplicates on login items, plus expired or expiring credentials on cards and API
credentials; it also reports offline 2FA/passkey availability. `--hibp` opts in to Have I
Been Pwned k-anonymity checks (only five hex characters of a SHA-1 leave the machine).
`--max-age-days` defaults to 365 and `--expiring-days` to 30. The scoring and checks are
documented in [WATCHTOWER.md](WATCHTOWER.md).

## Shell completions

```sh
gitpass completions zsh > ~/.zfunc/_gitpass
gitpass completions bash > ~/.local/share/bash-completion/completions/gitpass
gitpass completions fish > ~/.config/fish/completions/gitpass.fish
```

`completions` is hidden from `gitpass --help` but fully supported; `completion` is an
alias. Setup details for each shell are in the README "Shell completions" section.

## Environment variables

| Variable | Used for |
|---|---|
| `GITPASS_PATH` | Vault directory (same as `--path`) |
| `GITPASS_DEVICE_NAME` | Device name (same as `--device-name`) |
| `GITPASS_PASSWORD` | Master password prompts (unlock, and the new password for `init` / `rotate-secret-key`) |
| `GITPASS_NEW_PASSWORD` | New master password for `passwd` and `device revoke --rotate-credentials` |
| `GITPASS_ITEM_PASSWORD` | Item password for `add` and `edit --password` |
| `GITPASS_ITEM_CREDENTIAL` | API credential secret for `add --category api-credential` |
| `GITPASS_CARD_NUMBER` | Card number for `add --category credit-card` |
| `GITPASS_CARD_CVV` | Verification code (CVV) for `add --category credit-card` |
| `GITPASS_BANK_ACCOUNT_NUMBER` | Account number for `add --category bank-account` |
| `GITPASS_BANK_PIN` | PIN for `add --category bank-account` |
| `GITPASS_SECRET_KEY` | Secret Key, read before `<path>/secret.key` |
| `GITPASS_SSH_PASSPHRASE` | Passphrase for `ssh import` |
| `GITPASS_EXPORT_PASSPHRASE` | Passphrase for encrypted `import` / `export` |
| `GITPASS_IMPORT_PASSWORD` | Export password for password-protected Bitwarden JSON imports |

The password variables are meant for scripts and tests only: environment variables can
leak. Interactive prompts are used when they are unset.
