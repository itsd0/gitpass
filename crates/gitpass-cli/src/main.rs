use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use gitpass_core::crypto::{generate_password, KdfParams, SecretKey};
use gitpass_core::store::{self, Item, Session, UnlockOpts};
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
    /// Migrate a format v1 vault directory to format v2
    Migrate,
    /// Device registry: list, rename, revoke (format v3)
    Device { #[command(subcommand)] cmd: DeviceCmd },
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// List devices that have unlocked this account
    List,
    /// Rename a device (id or unique id prefix)
    Rename { id: String, name: String },
    /// Mark a device revoked. Does NOT re-encrypt keys; run `rotate-account-key --with-vault-keys` afterwards
    Revoke { id: String },
    /// Clear a revocation marker (run from another, non-revoked device)
    Unrevoke { id: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Field { Password, Username, Url, Totp, Notes, Title }

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
    let b = gitpass_core::migrate::migrate(root, pw, sk, Default::default())?;
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

fn do_copy(val: &str, secs: u64, quiet: bool, label: &str) -> Result<()> {
    match gitpass_cli::clip::system() {
        Ok(mut cb) => {
            if secs == 0 { cb.set(val)?; eprintln!("copied {label} to clipboard (not auto-cleared)"); return Ok(()); }
            eprintln!("copied {label} to clipboard; clearing in {secs}s (keep this running, Ctrl-C leaves it)");
            let cleared = gitpass_cli::clip::copy_then_clear(cb.as_mut(), val, std::time::Duration::from_secs(secs), std::thread::sleep)?;
            eprintln!("{}", if cleared { "clipboard cleared" } else { "clipboard changed since copy; left untouched" });
        }
        Err(e) => {
            eprintln!("warning: clipboard unavailable ({e}); {}", if quiet { "value not shown (--quiet)" } else { "printing to stdout" });
            if !quiet { println!("{val}"); }
        }
    }
    Ok(())
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
            { let m = s.load_manifest()?; println!("unlocked OK (keyset format v{}, manifest format v{}, epoch {}, integrity verified)", s.fmt.version, m.format, m.epoch); }
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
        Cmd::Get { vault, query, password_only, pick, copy, field, clear_secs, quiet } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            if *copy { do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, "field")?; }
            else if *password_only { println!("{}", it.password.clone().unwrap_or_default()); }
            else { print_item(&id, &it); }
        }
        Cmd::Copy { vault, query, field, pick, clear_secs, quiet } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (_, it) = pick_item(&s, &vid, query, *pick)?;
            do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, &format!("'{}'", it.title))?;
        }
        Cmd::Totp { vault, query, pick, copy, clear_secs } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (_, it) = pick_item(&s, &vid, query, *pick)?;
            let t = it.totp.as_deref().ok_or_else(|| anyhow::anyhow!("item '{}' has no TOTP secret", it.title))?;
            let (code, rem) = gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?);
            println!("{code}  ({rem}s remaining)");
            if *copy { do_copy(&code, *clear_secs, true, "TOTP code")?; }
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
        Cmd::Sync { merge } => {
            let (_s, pw, sk) = unlock_full(&cli, &root)?;
            if *merge {
                drop(_s);
                let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
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
            let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
            let s = Session::unlock_device(&root, &pw, &sk, &opts, cli.device_name.as_deref())
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
            let (mut s, _pw, _sk) = unlock_full(&cli, &root)?;
            match cmd {
                DeviceCmd::List => {
                    let reg = s.devices()?;
                    println!("{:<36}  {:<20}  {:<20}  {}", "ID", "NAME", "LAST SEEN (UTC)", "STATUS");
                    for d in &reg.devices {
                        let mut st = match &d.revoked_at { Some(t) if d.revoked => format!("REVOKED {t}"), _ if d.revoked => "REVOKED".into(), _ => "active".into() };
                        if d.id == s.device_id { st.push_str(" (this device)"); }
                        println!("{:<36}  {:<20}  {:<20}  {st}", d.id, d.name, d.last_seen_at);
                    }
                }
                DeviceCmd::Rename { id, name } => { let id = s.rename_device(id, name)?; println!("renamed {id} -> {name}"); }
                DeviceCmd::Revoke { id } => {
                    let reg = s.devices()?;
                    let full = reg.resolve(id)?;
                    let d = reg.get(&full).unwrap();
                    let this = if full == s.device_id { " (THIS device)" } else { "" };
                    if !cli.yes && !confirm(&format!("Revoke device {} '{}'{this}?", d.id, d.name))? { bail!("aborted"); }
                    s.revoke_device(&full)?;
                    println!("revoked {full}");
                    eprintln!("NOTE: revoke is a trust marker only; it does NOT re-encrypt vault keys.");
                    eprintln!("If the device may be compromised, run: gitpass rotate-account-key --with-vault-keys");
                    eprintln!("and also change the master password (`gitpass passwd`) and Secret Key (`gitpass rotate-secret-key`).");
                }
                DeviceCmd::Unrevoke { id } => { let id = s.unrevoke_device(id)?; println!("unrevoked {id}"); }
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
