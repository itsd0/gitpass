use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::store;
use anyhow::bail;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Vault { cmd: VaultCmd::Create { name, description } } => {
            let s = unlock(cli, root)?; println!("created vault {name} ({})", s.create_vault_with(name, description.clone())?);
        }
        Cmd::Vault { cmd: VaultCmd::Rename { vault, new_name } } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let mut m = s.vault_meta(&vid)?; m.name = new_name.clone();
            s.update_vault_meta(&vid, m)?;
            println!("renamed vault {vault} -> {} ({vid})", new_name.trim());
        }
        Cmd::Vault { cmd: VaultCmd::Rm { vault, yes } } => {
            let s = unlock(cli, root)?;
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
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            s.rotate_vault(&vid)?;
            println!("rotated vault key for {vault} ({vid})");
        }
        Cmd::Vault { cmd: VaultCmd::Share { vault, to } } => {
            let s = unlock(cli, root)?;
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
            let s = unlock(cli, root)?;
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
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let f = s.unshare_vault(&vid, member)?;
            println!("removed member {f} from {vault}; vault key rotated (they keep anything they already copied)");
        }
        Cmd::Vault { cmd: VaultCmd::Members { vault } } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            for (f, p) in s.members(&vid)? { println!("{f}  {}{p}", gitpass_core::keys::PUB_PREFIX); }
        }
        Cmd::Vault { cmd: VaultCmd::List } => {
            let s = unlock(cli, root)?;
            for (id, m) in s.list_vaults()? {
                match &m.description { Some(d) => println!("{id}  {}  ({d})", m.name), None => println!("{id}  {}", m.name) }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
