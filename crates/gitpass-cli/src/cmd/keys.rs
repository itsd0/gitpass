use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::store;
use gitpass_core::crypto::KdfParams;
use zeroize::Zeroizing;
use anyhow::bail;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Init { save_secret_key } => {
            let pw = prompt_secret("GITPASS_PASSWORD", "New master password: ")?;
            if std::env::var("GITPASS_PASSWORD").is_err() {
                let pw2 = prompt_secret("__none__", "Confirm master password: ")?;
                if *pw != *pw2 { bail!("passwords do not match"); }
            }
            if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
            let sk = store::init(root, &pw, KdfParams::default())?;
            let enc = Zeroizing::new(sk.encode());
            if *save_secret_key { save_sk(root, &enc)?; }
            print_kit(root, &enc);
        }
        Cmd::Unlock => {
            let s = unlock(cli, root)?;
            { let m = s.load_manifest()?; println!("unlocked OK (keyset format v{}, manifest format v{}, epoch {}, integrity verified)", s.fmt.version, m.format, m.epoch); }
        }
        Cmd::Passwd => {
            let (mut s, _pw, sk) = unlock_full(cli, root)?;
            let npw = new_password()?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            s.change_password(&npw, &sk, k)?;
            println!("master password changed (manifest epoch {})", s.epoch()?);
        }
        Cmd::RotateSecretKey { save_secret_key } => {
            let (mut s, pw, _sk) = unlock_full(cli, root)?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            let nsk = s.rotate_secret_key(&pw, k)?;
            let enc = Zeroizing::new(nsk.encode());
            if *save_secret_key || root.join("secret.key").exists() { save_sk(root, &enc)?; }
            print_kit(root, &enc);
            println!("Secret Key rotated. Your OLD Secret Key no longer opens the current keyset.");
        }
        Cmd::RotateAccountKey { with_vault_keys } => {
            let (mut s, pw, sk) = unlock_full(cli, root)?;
            let b = s.rotate_account_key(&pw, &sk)?;
            println!("account key rotated (backup of previous state: {})", b.display());
            if *with_vault_keys {
                for (vid, m) in s.list_vaults()? { s.rotate_vault(&vid)?; println!("rotated vault key for {}", m.name); }
            }
        }
        Cmd::Migrate => {
            let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
            let sk = load_secret_key(root)?;
            do_migrate(root, &pw, &sk)?;
        }
        _ => unreachable!(),
    }
    Ok(())
}
