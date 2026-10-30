use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use std::path::PathBuf;
use zeroize::Zeroizing;
use anyhow::Context;
use anyhow::bail;

pub(crate) fn default_ssh_socket() -> PathBuf {
    gitpass_core::ssh::default_ssh_agent_socket_path()
}

pub(crate) fn ssh_cmd(cli: &Cli, root: &Path, cmd: &SshCmd) -> Result<()> {
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
        SshCmd::GitSigning { vault, query, socket, global: _, local, print: _, apply, yes, email, allowed_signers } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, query)?;
            let (_, r) = s.read_record(&vid, &iid)?;
            let k = SshKey::from_record(&r)?;
            ssh::check_git_signing_support(&k)?;

            let socket_path = socket.clone().unwrap_or_else(ssh::default_ssh_agent_socket_path);
            let allowed_signers_path = allowed_signers.clone().unwrap_or_else(ssh::default_allowed_signers_path);
            let email_val = ssh::resolve_signing_email(email.as_deref(), &k);

            let cfg = ssh::build_git_signing_config(&k, &email_val, &allowed_signers_path, &socket_path)?;

            let should_apply = *apply;
            if !should_apply {
                println!("{}", cfg.export_auth_sock);
                println!();
                println!("{}", cfg.config_lines);
                return Ok(());
            }

            let is_global = !*local;
            let target_desc = if is_global { "global git config (~/.gitconfig)" } else { "local repository git config" };
            let signers_disp = ssh::display_path(&allowed_signers_path);

            if !*yes && !cli.yes {
                let prompt_msg = format!("Apply Git signing configuration to {target_desc} and update {signers_disp}?");
                if !confirm(&prompt_msg)? {
                    bail!("aborted (pass --yes to confirm)");
                }
            }

            ssh::apply_git_signing(&k, &email_val, &allowed_signers_path, &socket_path, is_global)?;
            eprintln!("Applied Git commit signing configuration to {target_desc}.");
            eprintln!("Updated allowed signers: {signers_disp}");
            if !cfg.auth_sock_active {
                eprintln!();
                eprintln!("Warning: SSH_AUTH_SOCK does not point to the gitpass SSH agent.");
                eprintln!("Git uses ssh-keygen to sign commits, which talks to $SSH_AUTH_SOCK.");
                eprintln!("Add this line to your shell profile (~/.zshrc or ~/.bashrc):");
                eprintln!("  {}", cfg.export_auth_sock);
            }
            println!("{}", cfg.export_auth_sock);
            println!();
            println!("{}", cfg.config_lines);
        }
    }
    Ok(())
}
