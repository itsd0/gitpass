//! v0.7 SSH agent: serves the vault's `ssh_key` items over the ssh-agent protocol on
//! `~/Library/Application Support/gitpass/ssh-agent.sock` (dir 0700, socket 0600, peer uid checked).
//!
//! * Locked vault: REQUEST_IDENTITIES answers an empty list and SIGN_REQUEST fails.
//! * Agent use does NOT refresh the idle auto-lock timer.
//! * Settings `sshApproveEachUse`: every signature asks [`AgentUi::approve`] first (denied = failure).
use crate::agent::{peer_uid, AgentUi};
use crate::api::App;
use anyhow::{anyhow, bail, Result};
use gitpass_core::ssh::{handle_message, session_identities, KeyProvider};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SOCKET_NAME: &str = "ssh-agent.sock";

pub struct AppKeys { pub app: Arc<App>, pub ui: Arc<dyn AgentUi> }
impl KeyProvider for AppKeys {
    fn identities(&self) -> Vec<(Vec<u8>, String)> {
        self.app.state.peek(|u| session_identities(&u.session)).map(|v| v.into_iter().map(|i| (i.blob, i.title)).collect()).unwrap_or_default()
    }
    fn sign(&self, blob: &[u8], data: &[u8], _flags: u32) -> Result<Vec<u8>> {
        let id = self.app.state.peek(|u| session_identities(&u.session))?.into_iter().find(|i| i.blob == blob).ok_or_else(|| anyhow!("unknown key"))?;
        if self.app.get_settings().ssh_approve_each_use
            && !self.ui.approve("SSH key use", &format!("Allow an SSH signature with \u{201c}{}\u{201d} ({})?", id.title, id.key.info.fingerprint)) {
            bail!("denied by user");
        }
        id.key.sign(data)
    }
}

pub struct SshAgent { pub socket_path: PathBuf }

fn my_uid() -> u32 { unsafe { libc::getuid() } }

/// Bind `<dir>/ssh-agent.sock` and serve until the process exits.
pub fn start(app: Arc<App>, ui: Arc<dyn AgentUi>, dir: &Path) -> Result<Arc<SshAgent>> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let sp = dir.join(SOCKET_NAME);
    if sp.exists() {
        if UnixStream::connect(&sp).is_ok() { bail!("another SSH agent is already listening on {}", sp.display()); }
        std::fs::remove_file(&sp)?;
    }
    let l = UnixListener::bind(&sp)?;
    std::fs::set_permissions(&sp, std::fs::Permissions::from_mode(0o600))?;
    let keys = Arc::new(AppKeys { app, ui });
    std::thread::spawn(move || for s in l.incoming() {
        let Ok(mut s) = s else { continue };
        if !matches!(peer_uid(&s), Ok(u) if u == my_uid()) { continue; }
        let k = keys.clone();
        std::thread::spawn(move || { let _ = gitpass_core::ssh::serve_stream(&mut s, &*k); });
    });
    let _ = handle_message; // re-exported for tests
    Ok(Arc::new(SshAgent { socket_path: sp }))
}
