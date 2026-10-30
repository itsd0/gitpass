//! v0.7 SSH agent: serves the vault's `ssh_key` items over the ssh-agent protocol on
//! `~/Library/Application Support/gitpass/ssh-agent.sock` (dir 0700, socket 0600, peer uid checked).
//!
//! * Locked vault: REQUEST_IDENTITIES answers an empty list and SIGN_REQUEST fails.
//! * Agent use does NOT refresh the idle auto-lock timer.
//! * Settings `sshApproveEachUse`: every signature asks [`AgentUi::approve`] first (denied = failure).
//! * Every sign request is recorded (metadata only) in [`App::ssh_agent_activity`]; the log is
//!   cleared when the vault locks and never touches disk or the vault.
#[cfg(unix)]
use crate::agent::peer_uid;
use crate::agent::AgentUi;
use crate::api::App;
use crate::ssh_activity::{now_iso, SshActivityEntry, SshActivityResult};
use anyhow::{anyhow, bail, Result};
#[cfg(unix)]
use gitpass_core::ssh::handle_message;
use gitpass_core::ssh::{session_identities, KeyProvider, SSH_AGENT_RSA_SHA2_256, SSH_AGENT_RSA_SHA2_512};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SOCKET_NAME: &str = "ssh-agent.sock";

/// Socket peer that opened the connection (best effort; both fields are blank when unavailable).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PeerClient { pub name: Option<String>, pub pid: Option<u32> }

fn requested_algorithm(key_type: &str, flags: u32) -> String {
    match key_type {
        "ssh-rsa" if flags & SSH_AGENT_RSA_SHA2_512 != 0 => "rsa-sha2-512".into(),
        "ssh-rsa" if flags & SSH_AGENT_RSA_SHA2_256 != 0 => "rsa-sha2-256".into(),
        "ssh-ed25519" => "ed25519".into(),
        k if k.starts_with("ecdsa-sha2-") => "ecdsa".into(),
        k => k.to_string(),
    }
}

/// Algorithm name from the SSH public-key blob (`string alg || ...`) when the key is unknown to the vault.
fn blob_key_type(blob: &[u8]) -> Option<String> {
    let n = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    let end = 4usize.checked_add(n)?;
    std::str::from_utf8(blob.get(4..end)?).ok().filter(|s| !s.is_empty()).map(str::to_string)
}

pub struct AppKeys { pub app: Arc<App>, pub ui: Arc<dyn AgentUi> }

impl AppKeys {
    fn record(&self, id: Option<&gitpass_core::ssh::Identity>, algorithm: &str, result: SshActivityResult, peer: &PeerClient) {
        self.app.ssh_activity.record(SshActivityEntry {
            time: now_iso(),
            key_title: id.map(|i| i.title.clone()).unwrap_or_default(),
            fingerprint: id.map(|i| i.key.info.fingerprint.clone()).unwrap_or_default(),
            key_type: id.map(|i| i.key.info.key_type.clone()).unwrap_or_default(),
            algorithm: algorithm.into(),
            result,
            client: peer.name.clone(),
            pid: peer.pid,
        });
    }

    fn sign_with_peer(&self, blob: &[u8], data: &[u8], flags: u32, peer: &PeerClient) -> Result<Vec<u8>> {
        let id = self.app.state.peek(|u| session_identities(&u.session)).ok().and_then(|ids| ids.into_iter().find(|i| i.blob == blob));
        let Some(id) = id else {
            let algorithm = requested_algorithm(&blob_key_type(blob).unwrap_or_default(), flags);
            self.record(None, &algorithm, SshActivityResult::Refused, peer);
            return Err(anyhow!("unknown key"));
        };
        let algorithm = requested_algorithm(&id.key.info.key_type, flags);
        if self.app.get_settings().ssh_approve_each_use
            && !self.ui.approve("SSH key use", &format!("Allow an SSH signature with \u{201c}{}\u{201d} ({})?", id.title, id.key.info.fingerprint))
        {
            self.record(Some(&id), &algorithm, SshActivityResult::Denied, peer);
            bail!("denied by user");
        }
        match id.key.sign_with_flags(data, flags) {
            Ok(sig) => {
                self.record(Some(&id), &algorithm, SshActivityResult::Signed, peer);
                Ok(sig)
            }
            Err(e) => {
                self.record(Some(&id), &algorithm, SshActivityResult::Refused, peer);
                Err(e)
            }
        }
    }
}

impl KeyProvider for AppKeys {
    fn identities(&self) -> Vec<(Vec<u8>, String)> {
        self.app.state.peek(|u| session_identities(&u.session)).map(|v| v.into_iter().map(|i| (i.blob, i.title)).collect()).unwrap_or_default()
    }
    fn sign(&self, blob: &[u8], data: &[u8], flags: u32) -> Result<Vec<u8>> {
        self.sign_with_peer(blob, data, flags, &PeerClient::default())
    }
}

/// Per-connection provider: same keys, records the socket peer that asked.
struct ConnKeys { inner: Arc<AppKeys>, peer: PeerClient }
impl KeyProvider for ConnKeys {
    fn identities(&self) -> Vec<(Vec<u8>, String)> { self.inner.identities() }
    fn sign(&self, blob: &[u8], data: &[u8], flags: u32) -> Result<Vec<u8>> { self.inner.sign_with_peer(blob, data, flags, &self.peer) }
}

#[cfg(target_os = "macos")]
fn peer_pid(s: &UnixStream) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    let rc = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_LOCAL, libc::LOCAL_PEERPID, &mut pid as *mut _ as *mut libc::c_void, &mut len) };
    (rc == 0 && pid > 0).then_some(pid as u32)
}

#[cfg(target_os = "linux")]
fn peer_pid(s: &UnixStream) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let mut c: libc::ucred = unsafe { std::mem::zeroed() };
    let mut l = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut c as *mut _ as *mut libc::c_void, &mut l) };
    (rc == 0 && c.pid > 0).then_some(c.pid as u32)
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn peer_pid(_: &UnixStream) -> Option<u32> { None }

#[cfg(target_os = "macos")]
fn process_name(pid: u32) -> Option<String> {
    let mut buf = [0u8; 4096]; // PROC_PIDPATHINFO_MAXSIZE
    let n = unsafe { libc::proc_pidpath(pid as libc::c_int, buf.as_mut_ptr() as *mut libc::c_void, buf.len() as u32) };
    if n <= 0 { return None; }
    let path = std::str::from_utf8(&buf[..n as usize]).ok()?;
    path.rsplit('/').next().filter(|s| !s.is_empty()).map(str::to_string)
}

#[cfg(target_os = "linux")]
fn process_name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn process_name(_: u32) -> Option<String> { None }

#[cfg(unix)]
fn peer_client(s: &UnixStream) -> PeerClient {
    let pid = peer_pid(s);
    PeerClient { name: pid.and_then(process_name), pid }
}

pub struct SshAgent { pub socket_path: PathBuf }

#[cfg(unix)]
fn my_uid() -> u32 { unsafe { libc::getuid() } }

/// Windows: the SSH agent (named pipe / OpenSSH agent protocol) is not implemented yet (STUB).
#[cfg(not(unix))]
pub fn start(_app: Arc<App>, _ui: Arc<dyn AgentUi>, _dir: &Path) -> Result<Arc<SshAgent>> {
    bail!("SSH agent is not implemented on Windows yet")
}

/// Bind `<dir>/ssh-agent.sock` and serve until the process exits.
#[cfg(unix)]
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
        let k = Arc::new(ConnKeys { inner: keys.clone(), peer: peer_client(&s) });
        std::thread::spawn(move || { let _ = gitpass_core::ssh::serve_stream(&mut s, &*k); });
    });
    let _ = handle_message; // re-exported for tests
    Ok(Arc::new(SshAgent { socket_path: sp }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_algorithm_maps_rsa_flags_and_ecdsa() {
        assert_eq!(requested_algorithm("ssh-rsa", 0), "ssh-rsa");
        assert_eq!(requested_algorithm("ssh-rsa", SSH_AGENT_RSA_SHA2_256), "rsa-sha2-256");
        assert_eq!(requested_algorithm("ssh-rsa", SSH_AGENT_RSA_SHA2_512), "rsa-sha2-512");
        assert_eq!(requested_algorithm("ecdsa-sha2-nistp256", 0), "ecdsa");
        assert_eq!(requested_algorithm("ssh-ed25519", 0), "ed25519");
    }

    #[test]
    fn blob_key_type_reads_the_wire_algorithm() {
        let mut blob = (11u32).to_be_bytes().to_vec();
        blob.extend_from_slice(b"ssh-ed25519");
        blob.extend_from_slice(b"rest");
        assert_eq!(blob_key_type(&blob).as_deref(), Some("ssh-ed25519"));
        assert_eq!(blob_key_type(b"garbage"), None);
        assert_eq!(blob_key_type(&[]), None);
    }
}
