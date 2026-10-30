use anyhow::{bail, Context, Result};
use crate::cli::{AddCategory, Cli, Field};
use gitpass_core::crypto::SecretKey;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_pasteboard::PasteboardMarkers;
use serde::Serialize;
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

pub(crate) fn validate_date_iso(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        bail!("invalid expires date format '{val}', expected YYYY-MM-DD");
    }
    let (_y, m, d): (i32, u32, u32) = match (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => bail!("invalid expires date format '{val}', expected YYYY-MM-DD"),
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        bail!("invalid expires date '{val}': month must be 01-12, day 01-31");
    }
    Ok(())
}

pub(crate) fn validate_card_expiry(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('/').collect();
    if parts.len() != 2 {
        bail!("invalid card expiry '{val}', expected MM/YY");
    }
    let (m, _y): (u32, u32) = match (parts[0].parse(), parts[1].parse()) {
        (Ok(m), Ok(y)) => (m, y),
        _ => bail!("invalid card expiry '{val}', expected MM/YY"),
    };
    if !(1..=12).contains(&m) {
        bail!("invalid card expiry month in '{val}', expected 01-12");
    }
    if !(parts[1].len() == 2 || parts[1].len() == 4) {
        bail!("invalid card expiry year in '{val}', expected 2 or 4 digits (MM/YY)");
    }
    Ok(())
}

pub(crate) struct AddFlags<'a> {
    pub(crate) category: AddCategory,
    pub(crate) username: &'a Option<String>,
    pub(crate) url: &'a Option<String>,
    pub(crate) totp: &'a Option<String>,
    pub(crate) generate: &'a Option<String>,
    pub(crate) password_stdin: bool,
    pub(crate) credential_stdin: bool,
    pub(crate) expires: &'a Option<String>,
    pub(crate) notes: &'a Option<String>,
    pub(crate) notes_stdin: bool,
    pub(crate) holder: &'a Option<String>,
    pub(crate) expiry: &'a Option<String>,
    pub(crate) bank: &'a Option<String>,
    pub(crate) account_type: &'a Option<String>,
    pub(crate) routing: &'a Option<String>,
    pub(crate) branch_phone: &'a Option<String>,
    pub(crate) branch_address: &'a Option<String>,
    pub(crate) account_number_stdin: bool,
    pub(crate) pin_stdin: bool,
    pub(crate) admin_url: &'a Option<String>,
    pub(crate) provider: &'a Option<String>,
    pub(crate) first_name: &'a Option<String>,
    pub(crate) last_name: &'a Option<String>,
    pub(crate) email: &'a Option<String>,
    pub(crate) phone: &'a Option<String>,
    pub(crate) address: &'a Option<String>,
    pub(crate) ssid: &'a Option<String>,
    pub(crate) security: &'a Option<String>,
    pub(crate) private_key_file: &'a Option<PathBuf>,
}

pub(crate) fn check_no_identity(flags: &AddFlags) -> Result<()> {
    if flags.first_name.is_some()
        || flags.last_name.is_some()
        || flags.email.is_some()
        || flags.phone.is_some()
        || flags.address.is_some()
    {
        bail!("identity flags are only valid for identity items");
    }
    Ok(())
}

pub(crate) fn check_no_wifi(flags: &AddFlags) -> Result<()> {
    if flags.ssid.is_some() || flags.security.is_some() {
        bail!("wifi flags are only valid for wifi items");
    }
    Ok(())
}

pub(crate) fn check_no_ssh(flags: &AddFlags) -> Result<()> {
    if flags.private_key_file.is_some() {
        bail!("--private-key-file is only valid for ssh-key items");
    }
    Ok(())
}

pub(crate) fn validate_add_flags(flags: &AddFlags) -> Result<()> {
    match flags.category {
        AddCategory::Login => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::ApiCredential => {
            if flags.totp.is_some() {
                bail!("--totp is only valid for login items");
            }
            if flags.generate.is_some() {
                bail!("--generate is only valid for login, server, and ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is only valid for login and server items (use --credential-stdin for API credentials)");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items (use --expires for API credentials)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expires {
                validate_date_iso(exp)?;
            }
        }
        AddCategory::SecureNote => {
            if flags.username.is_some() {
                bail!("--username is not supported for secure-note items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for secure-note items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for secure-note items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for secure-note items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for secure-note items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for secure-note items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for secure-note items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            if flags.notes.is_some() && flags.notes_stdin {
                bail!("cannot combine --notes and --notes-stdin");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::CreditCard => {
            if flags.username.is_some() {
                bail!("--username is not supported for credit-card items (use --holder for cardholder name)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for credit-card items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for credit-card items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for credit-card items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for credit-card items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for credit-card items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for credit-card items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items (use --expiry for credit cards)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expiry {
                validate_card_expiry(exp)?;
            }
        }
        AddCategory::BankAccount => {
            if flags.username.is_some() {
                bail!("--username is not supported for bank-account items (use --holder for account holder)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for bank-account items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for bank-account items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for bank-account items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for bank-account items (use --account-number-stdin or --pin-stdin)");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for bank-account items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for bank-account items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for bank-account items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Server => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for server items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for server items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for server items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Identity => {
            if flags.username.is_some() {
                bail!("--username is not supported for identity items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for identity items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for identity items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for identity items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for identity items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for identity items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for identity items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for identity items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for identity items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Wifi => {
            if flags.ssid.as_deref().unwrap_or("").trim().is_empty() {
                bail!("missing required field: --ssid is required for wifi items");
            }
            if let Some(sec) = flags.security.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                match sec.to_ascii_lowercase().as_str() {
                    "wpa2" | "wpa3" | "wep" | "none" | "wpa" | "wpa/wpa2" | "wpa2/wpa3" => {},
                    _ => bail!("invalid wifi security '{sec}', expected WPA2, WPA3, WEP, or none"),
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for wifi items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for wifi items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for wifi items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for wifi items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for wifi items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for wifi items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for wifi items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for wifi items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::SshKey => {
            if flags.generate.is_some() && flags.private_key_file.is_some() {
                bail!("cannot combine --generate and --private-key-file");
            }
            if flags.generate.is_none() && flags.private_key_file.is_none() {
                bail!("missing required option for ssh-key items: specify either --generate ed25519 or --private-key-file <PATH>");
            }
            if let Some(g) = flags.generate {
                let g_norm = g.trim().to_ascii_lowercase();
                if g_norm != "ed25519" && g != "24" {
                    bail!("unsupported key type '{g}' for --generate: gitpass currently supports generating 'ed25519' SSH keys");
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for ssh-key items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for ssh-key items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for ssh-key items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for ssh-key items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for ssh-key items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for ssh-key items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for ssh-key items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
        }
    }
    Ok(())
}
pub(crate) fn print_item(id: &str, r: &ItemRecord, reveal: bool) {
    let it = &r.item;
    println!("id:       {id}");
    println!("title:    {}", it.title);
    if r.category != Category::Login {
        println!("category: {}", r.category.as_str());
    }
    if r.category == Category::Login {
        if let Some(v) = &it.username { println!("username: {v}"); }
        if let Some(v) = &it.password {
            let p = if reveal { v.as_str() } else { "••••••••" };
            println!("password: {p}");
            if let Some(t) = r.password_changed_at() {
                println!("password changed: {}", gitpass_core::device::iso8601(t as i64));
            }
        }
        if let Some(v) = &it.url { println!("url:      {v}"); }
        if let Some(v) = &it.totp {
            let t = if reveal { v.as_str() } else { "••••••••" };
            println!("totp:     {t}");
        }
        if let Some(v) = &it.notes { println!("notes:    {v}"); }
        for s in &r.sections {
            for f in &s.fields {
                if f.removed { continue; }
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
    } else {
        for s in r.view_sections() {
            if let Some(t) = &s.title {
                println!("[{t}]");
            }
            for f in s.fields {
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
        if let Some(v) = &it.url {
            let has_url = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.kind == FieldKind::Url));
            if !has_url {
                println!("url:      {v}");
            }
        }
        if let Some(v) = &it.notes {
            let has_note = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.id == "note" || f.id == "notes"));
            if !has_note {
                println!("notes:    {v}");
            }
        }
    }
}

pub(crate) fn field_value(it: &Item, f: Field) -> Result<Zeroizing<String>> {
    let v = match f {
        Field::Password => it.password.clone(), Field::Username => it.username.clone(), Field::Url => it.url.clone(),
        Field::Notes => it.notes.clone(), Field::Title => Some(it.title.clone()),
        Field::Totp => match &it.totp { Some(t) => Some(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0), None => None },
    };
    v.map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("item '{}' has no such field", it.title))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct CliItemSummary {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) category: Category,
    pub(crate) username: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subtitle: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct CliField {
    pub(crate) label: String,
    pub(crate) kind: FieldKind,
    pub(crate) value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) masked: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct CliItemDetail {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) category: Category,
    pub(crate) username: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) password_changed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<String>,
    pub(crate) fields: Vec<CliField>,
}

pub(crate) fn ts(s: &str) -> String {
    s.trim().parse::<i64>().map(gitpass_core::device::iso8601).unwrap_or_else(|_| s.to_string())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct CliHistoryEntry {
    pub(crate) version: u64,
    pub(crate) updated_at: String,
    pub(crate) current: bool,
}

/// Default output file name for `attach get` without `-o`: only the final path component of the
/// stored name (which comes from synced vault data and must not be trusted), so names like
/// `../../.ssh/authorized_keys` or `/etc/passwd` can never write outside the current directory.
pub(crate) fn safe_attachment_filename(name: &str) -> PathBuf {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    match std::path::Path::new(base).file_name() {
        Some(f) if !f.is_empty() => PathBuf::from(f),
        _ => PathBuf::from("attachment"),
    }
}

pub(crate) fn find_attachment<'a>(attachments: &'a [gitpass_core::item::AttachmentMeta], target: &str) -> Result<&'a gitpass_core::item::AttachmentMeta> {
    if let Some(a) = attachments.iter().find(|a| a.id == target) {
        return Ok(a);
    }
    let by_name: Vec<_> = attachments.iter().filter(|a| a.name == target).collect();
    match by_name.len() {
        1 => Ok(by_name[0]),
        0 => {
            let by_prefix: Vec<_> = attachments.iter().filter(|a| a.id.starts_with(target)).collect();
            match by_prefix.len() {
                1 => Ok(by_prefix[0]),
                0 => bail!("no attachment matches '{target}'"),
                n => bail!("{n} attachments match prefix '{target}'; use the full attachment id"),
            }
        }
        n => bail!("{n} attachments named '{target}'; specify the attachment id"),
    }
}

pub(crate) fn resolve_item_across_vaults(s: &Session, vault: Option<&str>, item: &str) -> Result<(String, String)> {
    if let Some(v) = vault {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, item)?;
        return Ok((vid, iid));
    }
    if let Some((v, q)) = item.split_once('/') {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, q)?;
        return Ok((vid, iid));
    }
    let mut matches = Vec::new();
    for (vid, _) in s.list_vaults()? {
        if let Ok(m) = s.find_items(&vid, item) {
            for (iid, it) in m {
                matches.push((vid.clone(), iid, it.title.clone()));
            }
        }
    }
    match matches.len() {
        0 => bail!("no item matches '{item}'"),
        1 => Ok((matches[0].0.clone(), matches[0].1.clone())),
        _ => bail!("multiple items match '{item}'; specify --vault or <vault>/<item>"),
    }
}

pub(crate) fn item_summary(id: &str, r: &ItemRecord) -> CliItemSummary {
    let hint = if r.category == Category::ApiCredential {
        gitpass_core::mask::api_credential_secret(r).map(gitpass_core::mask::secret_hint)
    } else {
        None
    };
    CliItemSummary {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        updated_at: ts(&r.item.updated_at),
        last_used: None,
        subtitle: hint.clone(),
        hint,
    }
}

pub(crate) fn item_detail(id: &str, r: &ItemRecord, reveal: bool, last_used_at: Option<String>) -> CliItemDetail {
    let mut fields = Vec::new();
    for section in r.view_sections() {
        for f in section.fields {
            let (val, masked) = if f.kind.is_secret() {
                if reveal {
                    (f.value, None)
                } else if r.category == Category::ApiCredential {
                    (None, f.value.as_deref().map(gitpass_core::mask::secret_hint))
                } else {
                    (None, None)
                }
            } else {
                (f.value, None)
            };
            fields.push(CliField {
                label: f.label,
                kind: f.kind,
                value: val,
                masked,
            });
        }
    }
    CliItemDetail {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        archived: r.archived,
        deleted_at: r.deleted_at.map(|d| gitpass_core::device::iso8601(d as i64)),
        created_at: r.created_at.map(|c| gitpass_core::device::iso8601(c as i64)),
        updated_at: ts(&r.item.updated_at),
        password_changed_at: r.password_changed_at().map(|t| gitpass_core::device::iso8601(t as i64)),
        last_used_at,
        notes: r.item.notes.clone().filter(|n| !n.trim().is_empty()),
        fields,
    }
}

/// Ranked lookup; errors (listing candidates) unless one clear winner or --pick.
pub(crate) fn pick_record(s: &Session, vid: &str, query: &str, pick: Option<usize>, include_archived: bool) -> Result<(String, ItemRecord)> {
    let mut recs = s.list_records(vid)?;
    if !include_archived {
        recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
    }
    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_default();
    let ranked = gitpass_core::search::rank_by_candidate(
        query,
        recs.into_iter().map(|(id, _, rec)| (id, rec)),
        |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
    );

    if ranked.is_empty() {
        bail!("no item matches '{query}'");
    }
    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, id, rec)) in ranked.iter().enumerate() {
                    eprintln!("{:>3}. {id}  {}  {}  (score {sc})", i + 1, rec.item.title, rec.item.username.clone().unwrap_or_default());
                }
                bail!("{} items match '{query}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };
    let (_, id, rec) = ranked.into_iter().nth(idx).unwrap();
    Ok((id, rec))
}

pub(crate) fn pick_item(s: &Session, vid: &str, query: &str, pick: Option<usize>) -> Result<(String, Item)> {
    let (id, rec) = pick_record(s, vid, query, pick, false)?;
    Ok((id, rec.item.clone()))
}

pub(crate) fn resolve_target_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
    include_archived: bool,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    if let Some(v) = vault_str {
        let vid = s.resolve_vault(v)?;
        let (iid, rec) = pick_record(s, &vid, query_str, pick, include_archived)?;
        return Ok((vid, iid, rec));
    }

    let mut all_recs = Vec::new();
    for (vid, _) in s.list_vaults()? {
        let mut recs = s.list_records(&vid)?;
        if !include_archived {
            recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
        }
        for (id, _, rec) in recs {
            all_recs.push((vid.clone(), id, rec));
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        all_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        bail!("no item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} items match '{query_str}'; use --pick <n> or specify --vault", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

pub(crate) fn resolve_deleted_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    let search_vaults: Vec<String> = if let Some(v) = vault_str {
        vec![s.resolve_vault(v)?]
    } else {
        s.list_vaults()?.into_iter().map(|(v, _)| v).collect()
    };

    let mut deleted_recs = Vec::new();
    let mut non_deleted_recs = Vec::new();

    for vid in &search_vaults {
        for (id, _, rec) in s.list_records(vid)? {
            if rec.deleted_at.is_some() {
                deleted_recs.push((vid.clone(), id, rec));
            } else {
                non_deleted_recs.push((vid.clone(), id, rec));
            }
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        let active_ranked = gitpass_core::search::rank_by(
            query_str,
            non_deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
            |(_, _, rec)| &rec.item,
        );
        if let Some((_, _, (_, _, active_rec))) = active_ranked.into_iter().next() {
            bail!("item '{}' is not deleted", active_rec.item.title);
        }
        bail!("no deleted item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} deleted items match '{query_str}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

/// Clipboard markers for a copied field: passwords and TOTP codes are secrets; the rest are plain.
pub(crate) fn field_markers(f: Field) -> Option<PasteboardMarkers> {
    match f { Field::Password | Field::Totp => Some(PasteboardMarkers::SECRET), Field::Username | Field::Url | Field::Notes | Field::Title => None }
}

pub(crate) fn do_copy(val: &str, secs: u64, quiet: bool, label: &str, markers: Option<PasteboardMarkers>) -> Result<()> {
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

pub(crate) struct SessionResolver<'a>(pub(crate) &'a Session);
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

#[cfg(test)]
mod tests {
    use super::field_markers;
    use crate::cli::Field;
    use gitpass_pasteboard::PasteboardMarkers;

    #[test]
    fn copied_fields_are_classified_as_secret_or_plain() {
        assert_eq!(field_markers(Field::Password), Some(PasteboardMarkers::SECRET));
        assert_eq!(field_markers(Field::Totp), Some(PasteboardMarkers::SECRET));
        for f in [Field::Username, Field::Url, Field::Notes, Field::Title] {
            assert_eq!(field_markers(f), None);
        }
    }
}
