use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use gitpass_core::crypto::{generate_password, KdfParams, SecretKey};
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_pasteboard::PasteboardMarkers;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "gitpass", version, about = "Zero-knowledge password manager synced via a private git repo")]
struct Cli {
    /// Vault directory (default ~/.gitpass, or $GITPASS_PATH)
    #[arg(long, global = true, env = "GITPASS_PATH")]
    path: Option<PathBuf>,
    /// Accept a failed integrity/rollback check and re-seal the manifest from what is on disk (dangerous)
    #[arg(long, global = true)]
    accept_rollback: bool,
    /// Answer yes to confirmation prompts (v1 migration, share, revoke, plaintext export)
    #[arg(long, global = true)]
    yes: bool,
    /// Name to register this device under (default: $GITPASS_DEVICE_NAME, else hostname)
    #[arg(long, global = true, env = "GITPASS_DEVICE_NAME")]
    device_name: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new account: master password + generated Secret Key
    Init {
        /// Also save the Secret Key to <path>/secret.key (convenience; git-ignored)
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Verify master password + Secret Key
    Unlock,
    /// Vault management
    Vault { #[command(subcommand)] cmd: VaultCmd },
    /// Add an item
    Add {
        vault: String,
        #[arg(long)] title: String,
        #[arg(long)] username: Option<String>,
        #[arg(long)] url: Option<String>,
        #[arg(long)] notes: Option<String>,
        #[arg(long)] totp: Option<String>,
        /// Generate a password (optional length, default 24) instead of prompting
        #[arg(long, num_args = 0..=1, default_missing_value = "24")]
        generate: Option<usize>,
    },
    /// Decrypt and print an item (query = id, id prefix, or fuzzy match on title/username/url)
    Get { vault: String, query: String, /// Print only the password
        #[arg(long)] password_only: bool,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)] pick: Option<usize>,
        /// Copy the password (or --field) to the clipboard instead of printing the item
        #[arg(long)] copy: bool,
        #[arg(long, value_enum, default_value_t = Field::Password)] field: Field,
        #[arg(long, default_value_t = 30)] clear_secs: u64,
        #[arg(long)] quiet: bool },
    /// Copy an item field (default password) to the clipboard; blocks up to --clear-secs, then clears it
    Copy { vault: String, query: String,
        #[arg(long, value_enum, default_value_t = Field::Password)] field: Field,
        #[arg(long)] pick: Option<usize>,
        /// Seconds before the clipboard is cleared (0 = don't wait/clear)
        #[arg(long, default_value_t = 30)] clear_secs: u64,
        /// Never echo the value to stdout, even if the clipboard is unavailable
        #[arg(long)] quiet: bool },
    /// Print the current TOTP code and seconds remaining
    Totp { vault: String, query: String, #[arg(long)] pick: Option<usize>,
        /// Also copy the code to the clipboard (cleared after --clear-secs)
        #[arg(long)] copy: bool,
        #[arg(long, default_value_t = 30)] clear_secs: u64 },
    /// Ranked fuzzy search over title/username/url
    Find { vault: String, query: String },
    /// List items in a vault
    List { vault: String, /// Fuzzy filter (ranked)
        #[arg(long)] query: Option<String> },
    /// Edit an item
    Edit {
        vault: String, id: String,
        #[arg(long)] title: Option<String>,
        #[arg(long)] username: Option<String>,
        #[arg(long)] url: Option<String>,
        #[arg(long)] notes: Option<String>,
        #[arg(long)] totp: Option<String>,
        /// Prompt for a new password
        #[arg(long)] password: bool,
        #[arg(long, num_args = 0..=1, default_missing_value = "24")]
        generate: Option<usize>,
    },
    /// Remove an item
    Rm { vault: String, id: String },
    /// Generate a random password
    Gen { #[arg(default_value_t = 24)] len: usize },
    /// Run a command with secrets from gp://<vault>/<item>/<field> references injected into its
    /// environment only (never written to disk); resolved values are masked in its stdout/stderr
    Run {
        /// dotenv-style file whose values are (or contain) gp:// references; repeatable
        #[arg(long = "env-file", required = true)] env_file: Vec<PathBuf>,
        /// Do not mask secrets in the child's output (child then writes straight to the terminal)
        #[arg(long)] no_masking: bool,
        /// Command and arguments (after --)
        #[arg(last = true, required = true)] cmd: Vec<String>,
    },
    /// Render a template, replacing {{ gp://<vault>/<item>/<field> }} placeholders (stdout by default)
    Inject {
        /// Template file (default: stdin)
        #[arg(long, short = 'i')] input: Option<PathBuf>,
        /// Write to this file (mode 0600) instead of stdout. NOTE: this puts plaintext secrets on disk.
        #[arg(long, short = 'o')] output: Option<PathBuf>,
    },
    /// Connect the vault folder to a private git remote (GitHub or any git host)
    Remote { #[command(subcommand)] cmd: RemoteCmd },
    /// git add/commit/pull --rebase/push in the vault directory, then verify the manifest
    Sync {
        /// On divergence, field-merge items (per-field clocks) instead of failing on rebase conflicts
        #[arg(long)] merge: bool,
    },
    /// Change the master password (re-wraps keyset.enc only)
    Passwd,
    /// Generate a new Secret Key (re-wraps keyset.enc) and print a new Emergency Kit
    RotateSecretKey {
        /// Overwrite <path>/secret.key with the new key
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Generate a new Account Key and re-wrap all vault keys (recovery from a leaked password+Secret Key)
    RotateAccountKey {
        /// Also rotate every vault key (re-encrypts all items) -- recommended after a real compromise
        #[arg(long)]
        with_vault_keys: bool,
    },
    /// Migrate a format v1 vault directory to the current format
    Migrate,
    /// Device registry: list, rename, revoke (v4: revoke rotates keys)
    Device { #[command(subcommand)] cmd: DeviceCmd },
    /// Account identity (X25519) used for vault sharing
    Identity { #[command(subcommand)] cmd: IdentityCmd },
    /// Import items from another password manager's export file
    Import {
        vault: String,
        /// bitwarden-json | bitwarden-csv | 1password-csv | 1password-1pux | keepass-csv | chrome-csv | gitpass-json
        #[arg(long = "from")] from: String,
        file: PathBuf,
        /// Parse and count only; write nothing
        #[arg(long)] dry_run: bool,
        /// Import items even if title+username+url already exist
        #[arg(long)] allow_duplicates: bool,
    },
    /// Export a vault (PLAINTEXT unless --encrypted; requires --yes for plaintext)
    Export {
        vault: String,
        #[arg(long, value_enum, default_value_t = ExportFmt::GitpassJson)] format: ExportFmt,
        #[arg(long)] out: PathBuf,
        /// Passphrase-protected gitpass-json (Argon2id + XChaCha20-Poly1305); passphrase from $GITPASS_EXPORT_PASSPHRASE or prompt
        #[arg(long)] encrypted: bool,
    },
    /// SSH keys: generate / import / list / public key, `~/.ssh/config` line, and a standalone agent
    Ssh { #[command(subcommand)] cmd: SshCmd },
    /// Security audit: weak / reused / old passwords, missing TOTP, offline 2FA/passkey availability, optional HIBP check
    Audit {
        /// Vault (default: all vaults)
        vault: Option<String>,
        /// Check passwords against Have I Been Pwned (k-anonymity; only 5 hex chars of SHA-1 are sent)
        #[arg(long)] hibp: bool,
        #[arg(long, default_value_t = 365)] max_age_days: u64,
        #[arg(long, default_value_t = 30)] expiring_days: u64,
        #[arg(long)] json: bool,
    },
    /// Generate shell completion scripts for zsh, bash, or fish
    #[command(hide = true, alias = "completion")]
    Completions {
        /// Shell to generate completions for (zsh, bash, fish)
        shell: ShellTarget,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lower")]
enum ShellTarget {
    Bash,
    Zsh,
    Fish,
}

impl From<ShellTarget> for clap_complete::Shell {
    fn from(s: ShellTarget) -> Self {
        match s {
            ShellTarget::Bash => clap_complete::Shell::Bash,
            ShellTarget::Zsh => clap_complete::Shell::Zsh,
            ShellTarget::Fish => clap_complete::Shell::Fish,
        }
    }
}

fn print_completions(shell: ShellTarget) {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(
        clap_complete::Shell::from(shell),
        &mut cmd,
        "gitpass",
        &mut std::io::stdout(),
    );
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum ExportFmt { GitpassJson, Csv }

struct UreqHibp(ureq::Agent);
impl gitpass_core::audit::PwnedRangeClient for UreqHibp {
    fn range(&self, prefix: &str) -> Result<String> {
        let r = self.0.get(&format!("{}{prefix}", gitpass_core::audit::HIBP_RANGE_URL))
            .set("Add-Padding", "true").set("User-Agent", concat!("gitpass/", env!("CARGO_PKG_VERSION"))).call()
            .map_err(|e| anyhow::anyhow!("HIBP request failed: {e}"))?;
        Ok(r.into_string()?)
    }
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// List devices that have unlocked this account
    List,
    /// Rename a device (id or unique id prefix)
    Rename { id: String, name: String },
    /// Revoke a device: marks it revoked AND rotates the Account Key + every Vault Key (re-encrypts items),
    /// so it cannot read FUTURE changes. Data it already copied cannot be recalled.
    Revoke {
        id: String,
        /// Only set the v3 revocation marker (no key rotation)
        #[arg(long)]
        marker_only: bool,
        /// Also change the master password and generate a new Secret Key (do this if the device knew them)
        #[arg(long)]
        rotate_credentials: bool,
        /// With --rotate-credentials: overwrite <path>/secret.key with the new key
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Clear a revocation marker (run from another, non-revoked device)
    Unrevoke { id: String },
}

#[derive(Subcommand)]
enum RemoteCmd {
    /// git init (if needed), ensure .gitignore, commit the encrypted files and push -u origin main.
    /// URL: https://github.com/YOU/REPO.git or git@github.com:YOU/REPO.git (create an EMPTY private repo first).
    /// Uses your existing git credentials (SSH key or credential helper); never force-pushes.
    Add {
        url: String,
        /// Also accept a local path / file:// URL (testing, local mirrors)
        #[arg(long, hide = true)]
        allow_local: bool,
    },
    /// Show the remote URL, branch and ahead/behind counts
    Show,
    /// Disconnect: remove the 'origin' remote (local files and history are kept; nothing is deleted remotely)
    Remove,
}

#[derive(Subcommand)]
enum SshCmd {
    /// Generate an Ed25519 key into a new SSH Key item
    Generate { vault: String, #[arg(long)] title: String, /// Key comment (default: the title)
        #[arg(long)] comment: Option<String> },
    /// Import an OpenSSH private key file (passphrase from $GITPASS_SSH_PASSPHRASE or a prompt if encrypted)
    Import { vault: String, file: PathBuf, #[arg(long)] title: Option<String> },
    /// List SSH keys (all vaults, or one)
    List { vault: Option<String> },
    /// Print an SSH key's public key line (for authorized_keys / GitHub)
    Public { vault: String, query: String },
    /// Print the `~/.ssh/config` snippet that points ssh at the gitpass agent
    Config { #[arg(long)] socket: Option<PathBuf> },
    /// Run a foreground SSH agent serving this vault's keys (until Ctrl-C). The desktop app runs its own.
    Agent { #[arg(long)] socket: Option<PathBuf> },
}

#[derive(Subcommand)]
enum IdentityCmd {
    /// Print this account's public key + fingerprint (creates the identity on first use; upgrades to format v4)
    Show,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Field { Password, Username, Url, Totp, Notes, Title }

#[derive(Subcommand)]
enum VaultCmd {
    Create { name: String, #[arg(long)] description: Option<String> },
    List,
    /// New Vault Key; re-encrypt every item in the vault
    Rotate { vault: String },
    /// Seal the vault key to another account's identity (public key file such as identity.pub, or base64)
    Share { vault: String, #[arg(long)] to: String },
    /// Import a vault shared with you (path to vaults/<id>/members/<your-fpr>.enc in the sharer's checkout)
    Accept { share: PathBuf },
    /// Remove a member (fingerprint or prefix) and rotate the vault key
    Unshare { vault: String, member: String },
    /// List members a vault is shared with
    Members { vault: String },
    /// Rename a vault (name or id)
    Rename { vault: String, new_name: String },
    /// Permanently delete a vault and all its items (asks to type the vault name unless --yes)
    #[command(alias = "delete")]
    Rm { vault: String, #[arg(long)] yes: bool },
}

fn root(cli: &Cli) -> Result<PathBuf> {
    match &cli.path {
        Some(p) => Ok(p.clone()),
        None => Ok(dirs::home_dir().context("no home dir")?.join(".gitpass")),
    }
}

fn prompt_secret(env: &str, prompt: &str) -> Result<Zeroizing<String>> {
    if let Ok(v) = std::env::var(env) { return Ok(Zeroizing::new(v)); }
    Ok(Zeroizing::new(rpassword::prompt_password(prompt)?))
}

fn load_secret_key(root: &Path) -> Result<SecretKey> {
    if let Ok(v) = std::env::var("GITPASS_SECRET_KEY") { return SecretKey::parse(&v); }
    let f = root.join("secret.key");
    if f.exists() { return SecretKey::parse(&Zeroizing::new(std::fs::read_to_string(f)?)); }
    SecretKey::parse(&prompt_secret("__none__", "Secret Key: ")?)
}

fn confirm(prompt: &str) -> Result<bool> {
    use std::io::Write;
    print!("{prompt} [y/N] ");
    std::io::stdout().flush()?;
    let mut l = String::new();
    std::io::stdin().read_line(&mut l)?;
    Ok(matches!(l.trim(), "y" | "Y" | "yes"))
}

fn do_migrate(root: &Path, pw: &str, sk: &SecretKey) -> Result<()> {
    let b = gitpass_core::migrate::migrate(root, pw, sk, Default::default())?;
    eprintln!("migrated format v1 vault to the current format (v1 backup: {})", b.display());
    Ok(())
}

fn unlock_full(cli: &Cli, root: &Path) -> Result<(Session, Zeroizing<String>, SecretKey)> {
    let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
    let sk = load_secret_key(root)?;
    if store::read_format(root)?.version < store::MIN_READ_FORMAT {
        if cli.yes || confirm("This vault uses format v1. Migrate to the current format now (a backup is made first)?")? {
            do_migrate(root, &pw, &sk)?;
        } else {
            bail!("format v1 vault: run `gitpass migrate`");
        }
    }
    let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
    let s = Session::unlock_device(root, &pw, &sk, &opts, cli.device_name.as_deref())?;
    if s.device_revoked { eprintln!("WARNING: this device ({}) is REVOKED; only `device list` and `rotate-account-key` are allowed", s.device_id); }
    for p in &s.accepted_problems { eprintln!("WARNING (accepted via --accept-rollback): {p}"); }
    if s.kdf_upgraded { eprintln!("note: KDF parameters upgraded to m={} KiB t={} p={}", s.fmt.kdf.m_kib, s.fmt.kdf.t, s.fmt.kdf.p); }
    Ok((s, pw, sk))
}

fn unlock(cli: &Cli, root: &Path) -> Result<Session> { Ok(unlock_full(cli, root)?.0) }

fn new_password() -> Result<Zeroizing<String>> {
    let pw = prompt_secret("GITPASS_NEW_PASSWORD", "New master password: ")?;
    if std::env::var("GITPASS_NEW_PASSWORD").is_err() {
        let pw2 = prompt_secret("__none__", "Confirm new master password: ")?;
        if *pw != *pw2 { bail!("passwords do not match"); }
    }
    if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
    Ok(pw)
}

fn save_sk(root: &Path, enc: &str) -> Result<()> {
    gitpass_core::fsutil::atomic_write(&root.join("secret.key"), format!("{enc}\n").as_bytes())?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
    Ok(())
}

fn print_kit(root: &Path, enc: &str) {
    println!("==================== GITPASS EMERGENCY KIT ====================");
    println!("Vault path : {}", root.display());
    println!("Secret Key : {enc}");
    println!("Master pw  : (write it down by hand; never stored)");
    println!("Without BOTH your master password and this Secret Key, your");
    println!("data cannot be recovered. Print this and store it offline.");
    println!("===============================================================");
}

fn print_item(id: &str, it: &Item) {
    println!("id:       {id}\ntitle:    {}", it.title);
    if let Some(v) = &it.username { println!("username: {v}"); }
    if let Some(v) = &it.password { println!("password: {v}"); }
    if let Some(v) = &it.url { println!("url:      {v}"); }
    if let Some(v) = &it.totp { println!("totp:     {v}"); }
    if let Some(v) = &it.notes { println!("notes:    {v}"); }
}

fn field_value(it: &Item, f: Field) -> Result<Zeroizing<String>> {
    let v = match f {
        Field::Password => it.password.clone(), Field::Username => it.username.clone(), Field::Url => it.url.clone(),
        Field::Notes => it.notes.clone(), Field::Title => Some(it.title.clone()),
        Field::Totp => match &it.totp { Some(t) => Some(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0), None => None },
    };
    v.map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("item '{}' has no such field", it.title))
}

/// Ranked lookup; errors (listing candidates) unless one clear winner or --pick.
fn pick_item(s: &Session, vid: &str, query: &str, pick: Option<usize>) -> Result<(String, Item)> {
    let r = gitpass_core::search::rank(query, s.list_items(vid)?);
    if r.is_empty() { bail!("no item matches '{query}'"); }
    let idx = match pick {
        Some(n) if n >= 1 && n <= r.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", r.len()),
        None => match gitpass_core::search::clear_winner(&r) {
            Some(i) => i,
            None => {
                for (i, (sc, id, it)) in r.iter().enumerate() {
                    eprintln!("{:>3}. {id}  {}  {}  (score {sc})", i + 1, it.title, it.username.clone().unwrap_or_default());
                }
                bail!("{} items match '{query}'; use --pick <n> or the item id", r.len());
            }
        },
    };
    let (_, id, it) = r.into_iter().nth(idx).unwrap();
    Ok((id, it))
}

/// Clipboard markers for a copied field: passwords and TOTP codes are secrets; the rest are plain.
fn field_markers(f: Field) -> Option<PasteboardMarkers> {
    match f { Field::Password | Field::Totp => Some(PasteboardMarkers::SECRET), Field::Username | Field::Url | Field::Notes | Field::Title => None }
}

fn do_copy(val: &str, secs: u64, quiet: bool, label: &str, markers: Option<PasteboardMarkers>) -> Result<()> {
    match gitpass_cli::clip::system() {
        Ok(mut cb) => {
            if secs == 0 { gitpass_cli::clip::set_with_markers(cb.as_mut(), val, markers)?; eprintln!("copied {label} to clipboard (not auto-cleared)"); return Ok(()); }
            eprintln!("copied {label} to clipboard; clearing in {secs}s (keep this running, Ctrl-C leaves it)");
            let cleared = gitpass_cli::clip::copy_then_clear(cb.as_mut(), val, markers, std::time::Duration::from_secs(secs), std::thread::sleep)?;
            eprintln!("{}", if cleared { "clipboard cleared" } else { "clipboard changed since copy; left untouched" });
        }
        Err(e) => {
            eprintln!("warning: clipboard unavailable ({e}); {}", if quiet { "value not shown (--quiet)" } else { "printing to stdout" });
            if !quiet { println!("{val}"); }
        }
    }
    Ok(())
}

struct SessionResolver<'a>(&'a Session);
impl gitpass_cli::inject::Resolver for SessionResolver<'_> {
    fn resolve(&self, r: &gitpass_cli::inject::SecretRef) -> Result<Zeroizing<String>> {
        let s = self.0;
        let vid = s.resolve_vault(&r.vault).with_context(|| format!("gp://{}/...: vault", r.vault))?;
        let items = s.list_items(&vid)?;
        let exact: Vec<&(String, Item)> = items.iter().filter(|(id, it)| *id == r.item || it.title.eq_ignore_ascii_case(&r.item)).collect();
        let iid = match exact.len() {
            1 => exact[0].0.clone(),
            0 => s.resolve_item(&vid, &r.item).with_context(|| format!("gp://{}/{}: item", r.vault, r.item))?,
            n => bail!("gp://{}/{}: {n} items have that title; use the item id", r.vault, r.item),
        };
        let (_, rec) = s.read_record(&vid, &iid)?;
        let f = r.field.to_ascii_lowercase();
        let std = match f.as_str() {
            "password" => Some(Field::Password), "username" => Some(Field::Username), "url" => Some(Field::Url),
            "notes" => Some(Field::Notes), "title" => Some(Field::Title), "totp" | "otp" => Some(Field::Totp), _ => None,
        };
        if let Some(sf) = std { return field_value(&rec.item, sf); }
        let hits: Vec<&gitpass_core::item::Field> = rec.sections.iter().flat_map(|sec| sec.fields.iter())
            .filter(|x| !x.removed && (x.label.eq_ignore_ascii_case(&r.field) || x.id == r.field)).collect();
        match hits.as_slice() {
            [one] => one.value.clone().map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("field '{}' is empty", r.field)),
            [] => bail!("gp://{}/{}/{}: no such field", r.vault, r.item, r.field),
            _ => bail!("gp://{}/{}/{}: several fields have that label; use the field id", r.vault, r.item, r.field),
        }
    }
}

fn main() {
    if let Err(e) = run() { eprintln!("error: {e:#}"); std::process::exit(1); }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Cmd::Completions { shell } = &cli.cmd {
        print_completions(*shell);
        return Ok(());
    }
    let root = root(&cli)?;
    match &cli.cmd {
        Cmd::Init { save_secret_key } => {
            let pw = prompt_secret("GITPASS_PASSWORD", "New master password: ")?;
            if std::env::var("GITPASS_PASSWORD").is_err() {
                let pw2 = prompt_secret("__none__", "Confirm master password: ")?;
                if *pw != *pw2 { bail!("passwords do not match"); }
            }
            if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
            let sk = store::init(&root, &pw, KdfParams::default())?;
            let enc = Zeroizing::new(sk.encode());
            if *save_secret_key { save_sk(&root, &enc)?; }
            print_kit(&root, &enc);
        }
        Cmd::Unlock => {
            let s = unlock(&cli, &root)?;
            { let m = s.load_manifest()?; println!("unlocked OK (keyset format v{}, manifest format v{}, epoch {}, integrity verified)", s.fmt.version, m.format, m.epoch); }
        }
        Cmd::Vault { cmd: VaultCmd::Create { name, description } } => {
            let s = unlock(&cli, &root)?; println!("created vault {name} ({})", s.create_vault_with(name, description.clone())?);
        }
        Cmd::Vault { cmd: VaultCmd::Rename { vault, new_name } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let mut m = s.vault_meta(&vid)?; m.name = new_name.clone();
            s.update_vault_meta(&vid, m)?;
            println!("renamed vault {vault} -> {} ({vid})", new_name.trim());
        }
        Cmd::Vault { cmd: VaultCmd::Rm { vault, yes } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let name = s.vault_meta(&vid)?.name.clone();
            let n = s.list_records(&vid)?.len();
            if !yes {
                eprintln!("This permanently deletes vault '{name}' and its {n} item(s), attachments, history and shares.");
                eprint!("Type the vault name to confirm: ");
                let mut l = String::new(); std::io::stdin().read_line(&mut l)?;
                if l.trim() != name { bail!("name did not match; nothing deleted"); }
            }
            s.delete_vault(&vid)?;
            println!("deleted vault {name} ({vid}, {n} item(s))");
        }
        Cmd::Vault { cmd: VaultCmd::Rotate { vault } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            s.rotate_vault(&vid)?;
            println!("rotated vault key for {vault} ({vid})");
        }
        Cmd::Vault { cmd: VaultCmd::Share { vault, to } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let pk = gitpass_core::keys::load_pub(to)?;
            let fpr = gitpass_core::keys::fingerprint(&pk);
            if !cli.yes && !confirm(&format!("Share vault '{vault}' with fingerprint {fpr}? Verify this fingerprint with the recipient out of band."))? { bail!("aborted"); }
            let p = s.share_vault(&vid, &pk)?;
            println!("shared {vault} ({vid}) with {fpr}");
            println!("member file: {}", p.display());
            eprintln!("Commit + push, then the recipient runs: gitpass vault accept <checkout>/vaults/{vid}/members/{fpr}.enc");
        }
        Cmd::Vault { cmd: VaultCmd::Accept { share } } => {
            let s = unlock(&cli, &root)?;
            if let Ok(b) = std::fs::read(share) {
                if let Ok(mf) = serde_json::from_slice::<store::MemberFile>(&b) {
                    // The signature itself is verified by accept_share below.
                    // The signing key travels inside the file, so on first
                    // contact with a sender, verify this signing fingerprint
                    // out of band before trusting the sender's identity.
                    eprintln!("share claims to be from fingerprint {} (signing fingerprint {}; accept verifies the share signature)", mf.sender_fpr, store::sender_sign_fingerprint(&mf));
                }
            }
            let vid = s.accept_share(share)?;
            println!("imported shared vault {} ({vid}) as a snapshot", s.vault_meta(&vid)?.name);
        }
        Cmd::Vault { cmd: VaultCmd::Unshare { vault, member } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let f = s.unshare_vault(&vid, member)?;
            println!("removed member {f} from {vault}; vault key rotated (they keep anything they already copied)");
        }
        Cmd::Vault { cmd: VaultCmd::Members { vault } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            for (f, p) in s.members(&vid)? { println!("{f}  {}{p}", gitpass_core::keys::PUB_PREFIX); }
        }
        Cmd::Ssh { cmd } => ssh_cmd(&cli, &root, cmd)?,
        Cmd::Identity { cmd: IdentityCmd::Show } => {
            let s = unlock(&cli, &root)?;
            let (pk, fpr) = s.identity()?;
            println!("public key:  {}", gitpass_core::keys::encode_pub(&pk));
            println!("fingerprint: {fpr}");
            println!("file:        {}", root.join(store::IDENTITY_PUB_FILE).display());
        }
        Cmd::Vault { cmd: VaultCmd::List } => {
            let s = unlock(&cli, &root)?;
            for (id, m) in s.list_vaults()? {
                match &m.description { Some(d) => println!("{id}  {}  ({d})", m.name), None => println!("{id}  {}", m.name) }
            }
        }
        Cmd::Add { vault, title, username, url, notes, totp, generate } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let password = match generate {
                Some(n) => Some(generate_password(*n)),
                None => {
                    let p = prompt_secret("GITPASS_ITEM_PASSWORD", "Item password (empty for none): ")?;
                    if p.is_empty() { None } else { Some(p.to_string()) }
                }
            };
            let it = Item { title: title.clone(), username: username.clone(), password, url: url.clone(),
                notes: notes.clone(), totp: totp.clone(), updated_at: store::now() };
            println!("added {}", s.add_item(&vid, &it)?);
        }
        Cmd::Get { vault, query, password_only, pick, copy, field, clear_secs, quiet } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            if *copy { do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, "field", field_markers(*field))?; }
            else if *password_only { println!("{}", it.password.clone().unwrap_or_default()); }
            else { print_item(&id, &it); }
        }
        Cmd::Copy { vault, query, field, pick, clear_secs, quiet } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (_, it) = pick_item(&s, &vid, query, *pick)?;
            do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, &format!("'{}'", it.title), field_markers(*field))?;
        }
        Cmd::Totp { vault, query, pick, copy, clear_secs } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (_, it) = pick_item(&s, &vid, query, *pick)?;
            let t = it.totp.as_deref().ok_or_else(|| anyhow::anyhow!("item '{}' has no TOTP secret", it.title))?;
            let (code, rem) = gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?);
            println!("{code}  ({rem}s remaining)");
            if *copy { do_copy(&code, *clear_secs, true, "TOTP code", Some(PasteboardMarkers::SECRET))?; }
        }
        Cmd::Find { vault, query } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            for (sc, id, it) in gitpass_core::search::rank(query, s.list_items(&vid)?) {
                println!("{id}  {}  {}  {}  (score {sc})", it.title, it.username.clone().unwrap_or_default(), it.url.clone().unwrap_or_default());
            }
        }
        Cmd::List { vault, query } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let items: Vec<(String, Item)> = match query {
                Some(q) => gitpass_core::search::rank(q, s.list_items(&vid)?).into_iter().map(|(_, id, it)| (id, it)).collect(),
                None => s.list_items(&vid)?,
            };
            for (id, it) in items {
                println!("{id}  {}  {}", it.title, it.username.clone().unwrap_or_default());
            }
        }
        Cmd::Edit { vault, id, title, username, url, notes, totp, password, generate } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, id)?;
            let (_, mut it) = s.read_item(&vid, &iid)?;
            if let Some(v) = title { it.title = v.clone(); }
            if let Some(v) = username { it.username = Some(v.clone()); }
            if let Some(v) = url { it.url = Some(v.clone()); }
            if let Some(v) = notes { it.notes = Some(v.clone()); }
            if let Some(v) = totp { it.totp = Some(v.clone()); }
            if let Some(n) = generate { it.password = Some(generate_password(*n)); }
            else if *password { it.password = Some(prompt_secret("GITPASS_ITEM_PASSWORD", "New item password: ")?.to_string()); }
            it.updated_at = store::now();
            s.update_item(&vid, &iid, &it)?;
            println!("updated {iid}");
        }
        Cmd::Rm { vault, id } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, id)?;
            s.remove_item(&vid, &iid)?;
            println!("removed {iid}");
        }
        Cmd::Gen { len } => println!("{}", generate_password(*len)),
        Cmd::Run { env_file, no_masking, cmd } => {
            use gitpass_cli::inject;
            let s = unlock(&cli, &root)?;
            let sr = SessionResolver(&s);
            let mut res = inject::Resolved::new(&sr);
            let mut env = vec![];
            for f in env_file {
                let text = Zeroizing::new(std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?);
                env.extend(inject::parse_env_file(&text, &mut res)?);
            }
            let secrets = std::mem::take(&mut res.secrets);
            drop(s);
            let code = inject::run_child(cmd, &env, &secrets, !*no_masking)?;
            drop(env); drop(secrets);
            std::process::exit(code);
        }
        Cmd::Inject { input, output } => {
            use gitpass_cli::inject;
            use std::io::Read;
            let text = Zeroizing::new(match input {
                Some(p) => std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
                None => { let mut b = String::new(); std::io::stdin().read_to_string(&mut b)?; b }
            });
            let s = unlock(&cli, &root)?;
            let sr = SessionResolver(&s);
            let out = inject::render_template(&text, &mut inject::Resolved::new(&sr))?;
            match output {
                Some(p) => {
                    gitpass_core::fsutil::atomic_write(p, out.as_bytes())?;
                    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?; }
                    eprintln!("wrote {} (mode 0600; contains plaintext secrets)", p.display());
                }
                None => { use std::io::Write; std::io::stdout().write_all(out.as_bytes())?; }
            }
        }
        Cmd::Remote { cmd } => match cmd {
            RemoteCmd::Add { url, allow_local } => {
                print!("{}", gitpass_core::remote::add(&root, url, *allow_local)?);
                println!("connected: origin = {}", url.trim());
            }
            RemoteCmd::Show => {
                let i = gitpass_core::remote::show(&root)?;
                match &i.url {
                    Some(u) => println!("origin: {u}\nbranch: {}\nahead: {}  behind: {}", i.branch.as_deref().unwrap_or("?"), i.ahead, i.behind),
                    None => println!("not connected (run: gitpass remote add git@github.com:YOU/REPO.git)"),
                }
            }
            RemoteCmd::Remove => { gitpass_core::remote::remove(&root)?; println!("disconnected: removed 'origin' (local files and history kept)"); }
        },
        Cmd::Sync { merge } => {
            let (_s, pw, sk) = unlock_full(&cli, &root)?;
            if *merge {
                drop(_s);
                let opts = UnlockOpts { accept_rollback: cli.accept_rollback, target_kdf: gitpass_core::store::read_format(&root)?.kdf };
                let r = gitpass_core::sync::sync_merge(&root, &pw, &sk, &opts)?;
                print!("{}", r.log);
                for (vid, iid, m) in &r.merged {
                    println!("merged {vid}/{iid}: remote={:?} local={:?}", m.from_remote, m.from_local);
                    if m.has_conflicts() { println!("  CONFLICT (equal clocks, kept local): {:?}", m.conflicts); }
                }
                for (vid, iid) in &r.reapplied { println!("re-applied local-only item {vid}/{iid}"); }
            } else {
                print!("{}", gitpass_core::sync::sync(&root)?);
            }
            // verify-only: no device/KDF writes after the push (keeps the working tree clean)
            let s = Session::reopen_verify(&root, &pw, &sk, cli.accept_rollback)
                .context("post-pull integrity check failed; the pre-pull backup is in .gitpass-backup-*/")?;
            println!("post-pull integrity verified (manifest epoch {})", s.epoch()?);
        }
        Cmd::Passwd => {
            let (mut s, _pw, sk) = unlock_full(&cli, &root)?;
            let npw = new_password()?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            s.change_password(&npw, &sk, k)?;
            println!("master password changed (manifest epoch {})", s.epoch()?);
        }
        Cmd::RotateSecretKey { save_secret_key } => {
            let (mut s, pw, _sk) = unlock_full(&cli, &root)?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            let nsk = s.rotate_secret_key(&pw, k)?;
            let enc = Zeroizing::new(nsk.encode());
            if *save_secret_key || root.join("secret.key").exists() { save_sk(&root, &enc)?; }
            print_kit(&root, &enc);
            println!("Secret Key rotated. Your OLD Secret Key no longer opens the current keyset.");
        }
        Cmd::RotateAccountKey { with_vault_keys } => {
            let (mut s, pw, sk) = unlock_full(&cli, &root)?;
            let b = s.rotate_account_key(&pw, &sk)?;
            println!("account key rotated (backup of previous state: {})", b.display());
            if *with_vault_keys {
                for (vid, m) in s.list_vaults()? { s.rotate_vault(&vid)?; println!("rotated vault key for {}", m.name); }
            }
        }
        Cmd::Device { cmd } => {
            let (mut s, pw, sk) = unlock_full(&cli, &root)?;
            match cmd {
                DeviceCmd::List => {
                    let reg = s.devices()?;
                    println!("{:<36}  {:<20}  {:<20}  STATUS", "ID", "NAME", "LAST SEEN (UTC)");
                    for d in &reg.devices {
                        let mut st = match &d.revoked_at { Some(t) if d.revoked => format!("REVOKED {t}"), _ if d.revoked => "REVOKED".into(), _ => "active".into() };
                        if d.id == s.device_id { st.push_str(" (this device)"); }
                        println!("{:<36}  {:<20}  {:<20}  {st}", d.id, d.name, d.last_seen_at);
                    }
                }
                DeviceCmd::Rename { id, name } => { let id = s.rename_device(id, name)?; println!("renamed {id} -> {name}"); }
                DeviceCmd::Revoke { id, marker_only, rotate_credentials, save_secret_key } => {
                    let reg = s.devices()?;
                    let full = reg.resolve(id)?;
                    let d = reg.get(&full).unwrap();
                    let this = if full == s.device_id { " (THIS device)" } else { "" };
                    if !cli.yes && !confirm(&format!("Revoke device {} '{}'{this}?", d.id, d.name))? { bail!("aborted"); }
                    if *marker_only {
                        s.revoke_device(&full)?;
                        println!("revoked {full} (marker only; no keys rotated)");
                    } else {
                        let rep = s.revoke_device_and_rotate(&full, &pw, &sk)?;
                        println!("revoked {full}; rotated account key and {} vault key(s) (backup: {})", rep.vaults_rotated.len(), rep.account_key_backup.display());
                        eprintln!("NOTE: the revoked device cannot decrypt changes made from now on, but it KEEPS anything it already");
                        eprintln!("copied (old repo clone, items, old keys). Push now so other devices pick up the new keys.");
                    }
                    if *rotate_credentials {
                        let npw = new_password()?;
                        let k = s.fmt.kdf.max_with(&KdfParams::default());
                        let nsk = s.rotate_credentials(&npw, k)?;
                        let enc = Zeroizing::new(nsk.encode());
                        if *save_secret_key || root.join("secret.key").exists() { save_sk(&root, &enc)?; }
                        print_kit(&root, &enc);
                        println!("master password and Secret Key rotated.");
                    } else {
                        eprintln!("If the revoked device knew your master password + Secret Key it can still derive the new keys:");
                        eprintln!("rerun with --rotate-credentials, or run `gitpass passwd` and `gitpass rotate-secret-key`.");
                    }
                }
                DeviceCmd::Unrevoke { id } => { let id = s.unrevoke_device(id)?; println!("unrevoked {id}"); }
            }
        }
        Cmd::Import { vault, from, file, dry_run, allow_duplicates } => {
            use gitpass_core::import;
            let fmt: import::ImportFormat = from.parse()?;
            let data = Zeroizing::new(std::fs::read(file).with_context(|| format!("reading {}", file.display()))?);
            let pass = if fmt == import::ImportFormat::GitpassJson && import::is_encrypted_export(&String::from_utf8_lossy(&data)) {
                Some(prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?) } else { None };
            let parsed = import::parse_bytes(fmt, &data, pass.as_deref().map(|s| s.as_str()))?;
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let r = import::import_items(&s, &vid, parsed, import::ImportOpts { dry_run: *dry_run, allow_duplicates: *allow_duplicates })?;
            for w in &r.warnings { eprintln!("warning: {w}"); }
            for d in &r.duplicates { eprintln!("duplicate skipped: {d}"); }
            println!("{}parsed {}, {} {}, {} duplicate(s) skipped", if *dry_run { "[dry run] " } else { "" },
                r.parsed, if *dry_run { "would add" } else { "added" }, r.added, r.duplicates.len());
        }
        Cmd::Export { vault, format, out, encrypted } => {
            use gitpass_core::import;
            let out = &std::path::absolute(out)?;
            if *encrypted && *format == ExportFmt::Csv { bail!("--encrypted only supports --format gitpass-json"); }
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let recs: Vec<_> = s.list_records(&vid)?.into_iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| r).collect();
            let items: Vec<Item> = recs.iter().map(|r| r.item.clone()).collect();
            let body = if *encrypted {
                let p = prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?;
                if std::env::var("GITPASS_EXPORT_PASSPHRASE").is_err() {
                    let p2 = prompt_secret("__none__", "Confirm export passphrase: ")?;
                    if *p != *p2 { bail!("passphrases do not match"); }
                }
                Zeroizing::new(import::export_records_encrypted(&recs, &p, &import::export_kdf_default())?)
            } else {
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                eprintln!("!! WARNING: PLAINTEXT EXPORT. {} will contain ALL passwords,", out.display());
                eprintln!("!! TOTP secrets and notes UNENCRYPTED. Anyone who reads the file owns");
                eprintln!("!! these accounts. Prefer --encrypted. Delete the file when done.");
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                if !cli.yes && !confirm("Write plaintext export?")? { bail!("aborted (pass --yes to confirm)"); }
                Zeroizing::new(match format { ExportFmt::GitpassJson => import::export_records_json(&recs)?, ExportFmt::Csv => import::export_csv(&items)? })
            };
            gitpass_core::fsutil::atomic_write(out, body.as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o600))?; }
            println!("exported {} item(s) to {}{}", items.len(), out.display(), if *encrypted { " (encrypted)" } else { " (PLAINTEXT)" });
        }
        Cmd::Audit { vault, hibp, max_age_days, expiring_days, json } => {
            use gitpass_core::{audit, passkey, Category};
            let s = unlock(&cli, &root)?;
            let vids: Vec<(String, String)> = match vault {
                Some(v) => { let id = s.resolve_vault(v)?; vec![(id.clone(), s.vault_meta(&id)?.name.clone())] }
                None => s.list_vaults()?.into_iter().map(|(id, m)| (id, m.name.clone())).collect(),
            };
            let mut all = vec![];
            let mut passkeys = std::collections::BTreeSet::new();
            for (vid, name) in &vids {
                let mut rows = vec![];
                for (iid, _, rec) in s.list_records(vid)? {
                    if rec.deleted_at.is_some() || rec.archived { continue; }
                    if rec.category == Category::Passkey {
                        if let Some(v) = rec.find_field(passkey::F_RP_ID).and_then(|f| f.value) { passkeys.insert(v); }
                    } else {
                        rows.push((name.clone(), iid, rec));
                    }
                }
                rows.sort_by(|a, b| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()));
                all.extend(rows);
            }
            let client = UreqHibp(ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build());
            let opts = audit::AuditOpts { max_age_days: *max_age_days, expiring_days: *expiring_days, ..Default::default() };
            let r = audit::audit_with_passkeys(&all, &passkeys, &opts, if *hibp { Some(&client) } else { None })?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                return Ok(());
            }
            println!("audited {} item(s) in {} vault(s){}", r.items_checked, vids.len(), if *hibp { " (with HIBP)" } else { "" });
            println!("  weak: {}  reused: {} ({} group(s))  old (>{max_age_days}d): {}  expired: {}  expiring soon (<={expiring_days}d): {}  insecure URLs: {}  missing TOTP: {}  2FA available: {}  passkeys available: {}{}",
                r.weak, r.reused, r.reused_groups, r.old, r.expired, r.expiring_soon, r.insecure_urls, r.missing_totp, r.totp_available, r.passkey_available,
                if *hibp { format!("  pwned: {}", r.pwned) } else { String::new() });
            for f in &r.per_item {
                println!("\n[{}] {} ({})", f.vault, f.title, &f.id[..f.id.len().min(8)]);
                for x in &f.findings { println!("  - {x}"); }
            }
        }
        Cmd::Migrate => {
            let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
            let sk = load_secret_key(&root)?;
            do_migrate(&root, &pw, &sk)?;
        }
        Cmd::Completions { .. } => unreachable!(),
    }
    Ok(())
}

/// Default agent socket (same path the desktop app uses).
fn default_ssh_socket() -> PathBuf {
    #[cfg(target_os = "macos")]
    { dirs::home_dir().unwrap_or_default().join("Library/Application Support/gitpass/ssh-agent.sock") }
    #[cfg(not(target_os = "macos"))]
    { std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("gitpass/ssh-agent.sock") }
}

fn ssh_cmd(cli: &Cli, root: &Path, cmd: &SshCmd) -> Result<()> {
    use gitpass_core::ssh::{self, SshKey};
    match cmd {
        SshCmd::Config { socket } => println!("{}", ssh::identity_agent_line(&socket.clone().unwrap_or_else(default_ssh_socket))),
        SshCmd::Generate { vault, title, comment } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let k = SshKey::generate_ed25519(comment.as_deref().unwrap_or(title))?;
            let iid = s.add_record(&vid, k.to_record(title)?)?;
            println!("id:          {iid}\npublic key:  {}\nfingerprint: {}", k.info.public_key, k.info.fingerprint);
        }
        SshCmd::Import { vault, file, title } => {
            let pem = Zeroizing::new(std::fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?);
            let k = match SshKey::import_openssh(&pem, None) {
                Ok(k) => k,
                Err(e) if e.to_string().contains("encrypted") => {
                    let p = prompt_secret("GITPASS_SSH_PASSPHRASE", "Key passphrase: ")?;
                    SshKey::import_openssh(&pem, Some(&p))?
                }
                Err(e) => return Err(e),
            };
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let t = title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| if k.info.comment.is_empty() { file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default() } else { k.info.comment.clone() });
            let iid = s.add_record(&vid, k.to_record(&t)?)?;
            println!("id:          {iid}\npublic key:  {}\nfingerprint: {}", k.info.public_key, k.info.fingerprint);
        }
        SshCmd::List { vault } => {
            let s = unlock(cli, root)?;
            let only = match vault { Some(v) => Some(s.resolve_vault(v)?), None => None };
            for i in ssh::session_identities(&s)? {
                if only.as_ref().is_some_and(|v| *v != i.vault_id) { continue; }
                println!("{}  {}  {}  {}", i.item_id, i.key.info.key_type, i.key.info.fingerprint, i.title);
            }
        }
        SshCmd::Public { vault, query } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, query)?;
            let (_, r) = s.read_record(&vid, &iid)?;
            let k = SshKey::from_record(&r)?;
            println!("{}", k.info.public_key);
        }
        SshCmd::Agent { socket } => {
            #[cfg(not(unix))]
            { let _ = socket; bail!("`gitpass ssh agent` is not supported on Windows yet"); }
            #[cfg(unix)]
            {
            use std::os::unix::fs::PermissionsExt;
            use std::os::unix::net::{UnixListener, UnixStream};
            let sp = socket.clone().unwrap_or_else(default_ssh_socket);
            let s = unlock(cli, root)?;
            let ids = ssh::session_identities(&s)?;
            drop(s);
            if let Some(d) = sp.parent() { std::fs::create_dir_all(d)?; std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?; }
            if sp.exists() {
                if UnixStream::connect(&sp).is_ok() { bail!("an agent is already listening on {}", sp.display()); }
                std::fs::remove_file(&sp)?;
            }
            let l = UnixListener::bind(&sp)?;
            std::fs::set_permissions(&sp, std::fs::Permissions::from_mode(0o600))?;
            eprintln!("gitpass ssh agent: {} key(s) on {}", ids.len(), sp.display());
            eprintln!("export SSH_AUTH_SOCK=\"{}\"", sp.display());
            let keys = std::sync::Arc::new(ssh::StaticKeys { ids, approve: Box::new(|_, _| true) });
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                let k = keys.clone();
                std::thread::spawn(move || { let _ = ssh::serve_stream(&mut c, &*k); });
            }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_fields_are_classified_as_secret_or_plain() {
        assert_eq!(field_markers(Field::Password), Some(PasteboardMarkers::SECRET));
        assert_eq!(field_markers(Field::Totp), Some(PasteboardMarkers::SECRET));
        for f in [Field::Username, Field::Url, Field::Notes, Field::Title] {
            assert_eq!(field_markers(f), None);
        }
    }
}
