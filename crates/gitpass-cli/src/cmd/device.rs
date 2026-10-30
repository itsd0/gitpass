use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::crypto::KdfParams;
use zeroize::Zeroizing;
use anyhow::bail;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Device { cmd } => {
            let (mut s, pw, sk) = unlock_full(cli, root)?;
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
                        if *save_secret_key || root.join("secret.key").exists() { save_sk(root, &enc)?; }
                        print_kit(root, &enc);
                        println!("master password and Secret Key rotated.");
                    } else {
                        eprintln!("If the revoked device knew your master password + Secret Key it can still derive the new keys:");
                        eprintln!("rerun with --rotate-credentials, or run `gitpass passwd` and `gitpass rotate-secret-key`.");
                    }
                }
                DeviceCmd::Unrevoke { id } => { let id = s.unrevoke_device(id)?; println!("unrevoked {id}"); }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
