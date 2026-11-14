//! Desktop host role for pairing v2.
//!
//! The desktop shows `qr` and never learns a code to display. The phone shows
//! the 6-digit code; the person types it into [`App::pairing_host_confirm`].
//! Owner, repository, branch and the fine-grained PAT come from the vault's git
//! `origin` and the git-ignored `.gitpass-sync-token` file, not from UI arguments.
//! This module does not open a socket. The LAN listener is a separate step.
use super::super::dto::PairingHostPrepared;
use super::App;
use anyhow::{anyhow, Result};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, PairingError, PairingRequest, PairingStep};
use std::path::Path;
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct PairingHold {
    host: Option<HostSession>,
}

/// Bytes to write on the pairing connection, plus a fixed error string when the step failed.
#[derive(Debug)]
pub struct PairingTurn {
    pub outbound: Vec<u8>,
    pub phase: String,
    pub error: Option<String>,
}

pub(crate) fn clear_host_on_lock(slots: &Mutex<PairingHold>, dir: &Path) {
    let mut g = slots.lock().unwrap_or_else(|e| e.into_inner());
    let Some(host) = g.host.take() else { return };
    let sid = host.session_id().to_string();
    drop(host);
    if let Ok(mut consumed) = pairing::load_consumed(dir) {
        consumed.cancel_accept(&sid);
        let _ = pairing::save_consumed(dir, &consumed);
    }
}

const SYNC_TOKEN_FILE: &str = ".gitpass-sync-token";

/// Owner, repository and branch from the vault's `origin` plus the fine-grained PAT
/// in `.gitpass-sync-token`. Errors are fixed strings and do not include the token or the URL.
fn load_github_sync(root: &Path) -> Result<(String, String, String, String)> {
    let url = super::sync::git(root, &["remote", "get-url", "origin"])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("sync is not configured"))?;
    let branch = super::sync::git(root, &["symbolic-ref", "--short", "HEAD"])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("sync is not configured"))?;
    let (owner, repo) = github_owner_repo(&url).ok_or_else(|| anyhow!("sync is not configured"))?;
    gitpass_core::device::ensure_ignored(root, SYNC_TOKEN_FILE)
        .map_err(|_| anyhow!("sync token is not configured"))?;
    let raw = std::fs::read_to_string(root.join(SYNC_TOKEN_FILE))
        .map_err(|_| anyhow!("sync token is not configured"))?;
    let token = raw.trim();
    if token.is_empty() || token.contains('\n') || token.contains('\0') {
        return Err(anyhow!("sync token is not configured"));
    }
    Ok((owner, repo, branch, token.to_string()))
}

fn github_owner_repo(url: &str) -> Option<(String, String)> {
    let path = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.trim().strip_prefix("git@github.com:"))?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    if parts.next().is_some() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn turn(step: PairingStep, phase: &str) -> PairingTurn {
    let failed = step.error.is_some();
    PairingTurn {
        outbound: step.outbound,
        phase: if failed { "failed".into() } else { phase.into() },
        error: step.error.map(|e| e.to_string()),
    }
}

impl App {
    /// Open a host session for `addresses` and `port`. Returns the QR only.
    pub fn pairing_host_begin(&self, addresses: &[String], port: u16) -> Result<PairingHostPrepared> {
        let (account_id, secret, owner, repo, branch, token) = self.with(|u| {
            let (owner, repo, branch, token) = load_github_sync(&u.session.root)?;
            Ok((
                u.session.fmt.account_id.clone(),
                SecretKey(u.secret.0),
                owner,
                repo,
                branch,
                token,
            ))
        })?;
        let request = {
            let token = zeroize::Zeroizing::new(token);
            PairingRequest::new(&account_id, &owner, &repo, &branch, secret, &token)?
        };
        self.with(|_| {
            let previous = {
                let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                g.host.take()
            };
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            if let Some(prev) = previous.as_ref() {
                consumed.cancel_accept(prev.session_id());
            }
            let now = pairing::now_unix();
            let (host, qr) = match HostSession::begin(addresses, port, request, now, &mut consumed) {
                Ok(v) => v,
                Err(e) => {
                    if let Some(prev) = previous {
                        self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(prev);
                    }
                    return Err(e.into());
                }
            };
            if let Err(e) = pairing::save_consumed(&self.settings_dir, &consumed) {
                if let Some(prev) = previous {
                    self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(prev);
                }
                return Err(e.into());
            }
            let prepared = PairingHostPrepared {
                qr,
                expires_at: host.expires_at(),
                port: host.port(),
                addresses: host.addresses().to_vec(),
            };
            self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(host);
            Ok(prepared)
        })
    }

    /// Feed bytes from the phone. The returned bytes are the desktop's reply.
    pub fn pairing_host_push(&self, bytes: &[u8]) -> Result<PairingTurn> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
            let now = pairing::now_unix();
            let step = host.push(bytes, now);
            let phase = host.phase().to_string();
            Ok(turn(step, &phase))
        })
    }

    /// Release the grant when `code` is the code the phone displayed.
    /// The third wrong code ends the session and marks it used.
    pub fn pairing_host_confirm(&self, code: &str) -> Result<PairingTurn> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
            let now = pairing::now_unix();
            let sid = host.session_id().to_string();
            let exp = host.expires_at();
            let step = host.confirm(code, now);
            let phase = host.phase().to_string();
            let ended = matches!(
                step.error,
                Some(PairingError::Exhausted | PairingError::Expired)
            );
            let succeeded = step.error.is_none() && phase == "done";
            let out = turn(step, &phase);
            if succeeded || ended {
                let mut consumed = pairing::load_consumed(&self.settings_dir)?;
                consumed.mark_released(&sid, exp, now)?;
                pairing::save_consumed(&self.settings_dir, &consumed)?;
                g.host = None;
            }
            Ok(out)
        })
    }

    /// Drop the host session and allow a new QR until the old one would have expired.
    pub fn pairing_host_cancel(&self) -> Result<()> {
        let previous = {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            g.host.take()
        };
        if let Some(prev) = previous {
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            consumed.cancel_accept(prev.session_id());
            pairing::save_consumed(&self.settings_dir, &consumed)?;
        }
        Ok(())
    }
}
