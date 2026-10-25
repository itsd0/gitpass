use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use gitpass::crypto::{generate_password, KdfParams, SecretKey};
use gitpass::store::{self, Item, Session, UnlockOpts};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "gitpass", version, about = "Zero-knowledge password manager synced via a private git repo (MVP)")]
struct Cli {
    /// Vault directory (default ~/.gitpass, or $GITPASS_PATH)
    #[arg(long, global = true, env = "GITPASS_PATH")]
    path: Option<PathBuf>,
    /// Accept a failed integrity/rollback check and re-seal the manifest from what is on disk (dangerous)
    #[arg(long, global = true)]
    accept_rollback: bool,
    /// Answer yes to the v1 -> v2 migration prompt
    #[arg(long, global = true)]
    yes: bool,
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
    /// Decrypt and print an item (query = id, id prefix, or title substring)
    Get { vault: String, query: String, /// Print only the password
        #[arg(long)] password_only: bool },
    /// List items in a vault
    List { vault: String },
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
    /// git add/commit/pull --rebase/push in the vault directory, then verify the manifest
    Sync,
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
    /// Migrate a format v1 vault directory to format v2
    Migrate,
}

#[derive(Subcommand)]
enum VaultCmd {
    Create { name: String, #[arg(long)] description: Option<String> },
    List,
    /// New Vault Key; re-encrypt every item in the vault
    Rotate { vault: String },
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
    let b = gitpass::migrate::migrate(root, pw, sk, Default::default())?;
    eprintln!("migrated to format v2 (v1 backup: {})", b.display());
    Ok(())
}

fn unlock_full(cli: &Cli, root: &Path) -> Result<(Session, Zeroizing<String>, SecretKey)> {
    let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
    let sk = load_secret_key(root)?;
    if store::read_format(root)?.version < store::FORMAT_VERSION {
        if cli.yes || confirm("This vault uses format v1. Migrate to format v2 now (a backup is made first)?")? {
            do_migrate(root, &pw, &sk)?;
        } else {
            bail!("format v1 vault: run `gitpass migrate`");
        }
    }
    let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
    let s = Session::unlock_with(root, &pw, &sk, &opts)?;
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
    gitpass::fsutil::atomic_write(&root.join("secret.key"), format!("{enc}\n").as_bytes())?;
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

fn main() {
    if let Err(e) = run() { eprintln!("error: {e:#}"); std::process::exit(1); }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
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
            println!("unlocked OK (format v{}, manifest epoch {}, integrity verified)", s.fmt.version, s.epoch()?);
        }
        Cmd::Vault { cmd: VaultCmd::Create { name, description } } => {
            let s = unlock(&cli, &root)?; println!("created vault {name} ({})", s.create_vault_with(name, description.clone())?);
        }
        Cmd::Vault { cmd: VaultCmd::Rotate { vault } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            s.rotate_vault(&vid)?;
            println!("rotated vault key for {vault} ({vid})");
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
        Cmd::Get { vault, query, password_only } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let m = s.find_items(&vid, query)?;
            if m.is_empty() { bail!("no item matches '{query}'"); }
            if *password_only {
                if m.len() > 1 { bail!("{} items match; use the id", m.len()); }
                println!("{}", m[0].1.password.clone().unwrap_or_default());
            } else {
                for (i, (id, it)) in m.iter().enumerate() { if i > 0 { println!("---"); } print_item(id, it); }
            }
        }
        Cmd::List { vault } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            for (id, it) in s.list_items(&vid)? {
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
        Cmd::Sync => {
            let (_s, pw, sk) = unlock_full(&cli, &root)?;
            print!("{}", gitpass::sync::sync(&root)?);
            let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
            let s = Session::unlock_with(&root, &pw, &sk, &opts)
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
        Cmd::Migrate => {
            let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
            let sk = load_secret_key(&root)?;
            do_migrate(&root, &pw, &sk)?;
        }
    }
    Ok(())
}
