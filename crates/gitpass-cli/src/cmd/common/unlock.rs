use anyhow::{bail, Context, Result};
use crate::cli::Cli;
use gitpass_core::crypto::SecretKey;
use gitpass_core::store::{self, Session, UnlockOpts};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub(crate) fn root(cli: &Cli) -> Result<PathBuf> {
    match &cli.path {
        Some(p) => Ok(p.clone()),
        None => Ok(dirs::home_dir().context("no home dir")?.join(".gitpass")),
    }
}

pub(crate) fn prompt_secret(env: &str, prompt: &str) -> Result<Zeroizing<String>> {
    if let Ok(v) = std::env::var(env) { return Ok(Zeroizing::new(v)); }
    Ok(Zeroizing::new(rpassword::prompt_password(prompt)?))
}

pub(crate) fn load_secret_key(root: &Path) -> Result<SecretKey> {
    if let Ok(v) = std::env::var("GITPASS_SECRET_KEY") { return SecretKey::parse(&v); }
    let f = root.join("secret.key");
    if f.exists() { return SecretKey::parse(&Zeroizing::new(std::fs::read_to_string(f)?)); }
    SecretKey::parse(&prompt_secret("__none__", "Secret Key: ")?)
}

pub(crate) fn confirm(prompt: &str) -> Result<bool> {
    use std::io::Write;
    print!("{prompt} [y/N] ");
    std::io::stdout().flush()?;
    let mut l = String::new();
    std::io::stdin().read_line(&mut l)?;
    Ok(matches!(l.trim(), "y" | "Y" | "yes"))
}

pub(crate) fn do_migrate(root: &Path, pw: &str, sk: &SecretKey) -> Result<()> {
    let b = gitpass_core::migrate::migrate(root, pw, sk, Default::default())?;
    eprintln!("migrated format v1 vault to the current format (v1 backup: {})", b.display());
    Ok(())
}

pub(crate) fn unlock_full(cli: &Cli, root: &Path) -> Result<(Session, Zeroizing<String>, SecretKey)> {
    gitpass_core::unlock_throttle::check_throttle(root)?;
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

pub(crate) fn unlock_readonly(root: &Path) -> Result<Session> {
    gitpass_core::unlock_throttle::check_throttle(root)?;
    let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
    let sk = load_secret_key(root)?;
    Session::unlock_readonly(root, &pw, &sk)
}

pub(crate) fn unlock(cli: &Cli, root: &Path) -> Result<Session> { Ok(unlock_full(cli, root)?.0) }

pub(crate) fn new_password() -> Result<Zeroizing<String>> {
    let pw = prompt_secret("GITPASS_NEW_PASSWORD", "New master password: ")?;
    if std::env::var("GITPASS_NEW_PASSWORD").is_err() {
        let pw2 = prompt_secret("__none__", "Confirm new master password: ")?;
        if *pw != *pw2 { bail!("passwords do not match"); }
    }
    if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
    Ok(pw)
}

pub(crate) fn save_sk(root: &Path, enc: &str) -> Result<()> {
    gitpass_core::fsutil::atomic_write(&root.join("secret.key"), format!("{enc}\n").as_bytes())?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
    Ok(())
}

pub(crate) fn print_kit(root: &Path, enc: &str) {
    println!("==================== GITPASS EMERGENCY KIT ====================");
    println!("Vault path : {}", root.display());
    println!("Secret Key : {enc}");
    println!("Master pw  : (write it down by hand; never stored)");
    println!("Without BOTH your master password and this Secret Key, your");
    println!("data cannot be recovered. Print this and store it offline.");
    println!("===============================================================");
}
