use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use gitpass::crypto::{generate_password, KdfParams, SecretKey};
use gitpass::store::{self, Item, Session};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "gitpass", version, about = "Zero-knowledge password manager synced via a private git repo (MVP)")]
struct Cli {
    /// Vault directory (default ~/.gitpass, or $GITPASS_PATH)
    #[arg(long, global = true, env = "GITPASS_PATH")]
    path: Option<PathBuf>,
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
    /// git add/commit/pull --rebase/push in the vault directory
    Sync,
}

#[derive(Subcommand)]
enum VaultCmd { Create { name: String }, List }

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

fn unlock(root: &Path) -> Result<Session> {
    let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
    let sk = load_secret_key(root)?;
    Session::unlock(root, &pw, &sk)
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
            if *save_secret_key {
                std::fs::write(root.join("secret.key"), format!("{}\n", *enc))?;
                #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(root.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
            }
            println!("==================== GITPASS EMERGENCY KIT ====================");
            println!("Vault path : {}", root.display());
            println!("Secret Key : {}", *enc);
            println!("Master pw  : (write it down by hand; never stored)");
            println!("Without BOTH your master password and this Secret Key, your");
            println!("data cannot be recovered. Print this and store it offline.");
            println!("===============================================================");
        }
        Cmd::Unlock => { unlock(&root)?; println!("unlocked OK"); }
        Cmd::Vault { cmd: VaultCmd::Create { name } } => {
            let s = unlock(&root)?; println!("created vault {name} ({})", s.create_vault(name)?);
        }
        Cmd::Vault { cmd: VaultCmd::List } => {
            let s = unlock(&root)?;
            for (id, m) in s.list_vaults()? { println!("{id}  {}", m.name); }
        }
        Cmd::Add { vault, title, username, url, notes, totp, generate } => {
            let s = unlock(&root)?;
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
            let s = unlock(&root)?;
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
            let s = unlock(&root)?;
            let vid = s.resolve_vault(vault)?;
            for (id, it) in s.list_items(&vid)? {
                println!("{id}  {}  {}", it.title, it.username.clone().unwrap_or_default());
            }
        }
        Cmd::Edit { vault, id, title, username, url, notes, totp, password, generate } => {
            let s = unlock(&root)?;
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
            let s = unlock(&root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, id)?;
            s.remove_item(&vid, &iid)?;
            println!("removed {iid}");
        }
        Cmd::Gen { len } => println!("{}", generate_password(*len)),
        Cmd::Sync => print!("{}", gitpass::sync::sync(&root)?),
    }
    Ok(())
}
