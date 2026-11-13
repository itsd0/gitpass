//! Desktop host role for QR device pairing.
//!
//! #458 shows `announce` as a QR on the desktop and the 6-digit code on the phone only.
//! The person types that code here. This API never returns the desktop's code.
//! Owner, repository, branch and the fine-grained PAT come from the vault's git `origin`
//! (#454) and the git-ignored `.gitpass-sync-token` file, not from UI arguments.
use super::super::dto::PairingHostPrepared;
use super::App;
use anyhow::{anyhow, Result};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, PairingError, PairingRequest};
use std::path::Path;
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct PairingHold {
    host: Option<HostSession>,
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

/// Owner, repository and branch from the vault's `origin` (#454) plus the fine-grained PAT
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

impl App {
    /// Accept a joiner offer. Returns the announce QR only. The phone shows the code; the person
    /// types it into [`App::pairing_host_release`]. Repo and token come from the stored sync config.
    pub fn pairing_host_prepare(&self, offer: &str) -> Result<PairingHostPrepared> {
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
        // Save the replay record and store the session while the vault lock is held, so a lock
        // cannot clear the session and then lose the race to the replay file. A failed accept
        // puts the previous session back; the replay file is written only after success.
        self.with(|_| {
            let previous = {
                let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                g.host.take()
            };
            let restore = |previous: Option<HostSession>| -> Result<()> {
                if let Some(prev) = previous {
                    self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(prev);
                }
                Ok(())
            };
            let mut consumed = match pairing::load_consumed(&self.settings_dir) {
                Ok(c) => c,
                Err(e) => {
                    restore(previous)?;
                    return Err(e.into());
                }
            };
            if let Some(prev) = previous.as_ref() {
                consumed.cancel_accept(prev.session_id());
            }
            let now = pairing::now_unix();
            let host = match HostSession::accept(offer, request, now, &mut consumed) {
                Ok(host) => host,
                Err(e) => {
                    restore(previous)?;
                    return Err(e.into());
                }
            };
            let prepared = PairingHostPrepared {
                announce: host.announce(),
                expires_at: host.expires_at(),
            };
            if let Err(e) = pairing::save_consumed(&self.settings_dir, &consumed) {
                restore(previous)?;
                return Err(e.into());
            }
            self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(host);
            Ok(prepared)
        })
    }

    /// Release the grant when `code` is the code the phone displayed. The third wrong code
    /// ends the session and marks it used.
    pub fn pairing_host_release(&self, code: &str) -> Result<String> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let now = pairing::now_unix();
            enum Step {
                Grant(String, String, i64),
                Burned(String, i64),
                Refuse(PairingError),
            }
            let step = {
                let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
                let sid = host.session_id().to_string();
                let exp = host.expires_at();
                match host.grant_if_code_matches(code, now) {
                    Ok(grant) => Step::Grant(grant, sid, exp),
                    Err(PairingError::Exhausted) => Step::Burned(sid, exp),
                    Err(e) => Step::Refuse(e),
                }
            };
            match step {
                Step::Grant(grant, sid, exp) => {
                    let mut consumed = pairing::load_consumed(&self.settings_dir)?;
                    consumed.mark_released(&sid, exp, now)?;
                    pairing::save_consumed(&self.settings_dir, &consumed)?;
                    if let Some(host) = g.host.as_mut() {
                        host.wipe();
                    }
                    g.host = None;
                    Ok(grant)
                }
                Step::Burned(sid, exp) => {
                    let mut consumed = pairing::load_consumed(&self.settings_dir)?;
                    consumed.mark_released(&sid, exp, now)?;
                    pairing::save_consumed(&self.settings_dir, &consumed)?;
                    g.host = None;
                    Err(PairingError::Exhausted.into())
                }
                Step::Refuse(e) => Err(e.into()),
            }
        })
    }

    /// Drop the host session and allow the same offer to be accepted again until it expires.
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
