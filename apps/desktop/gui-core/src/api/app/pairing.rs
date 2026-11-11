//! Desktop host role for QR device pairing. The UI that shows the QR is a later issue;
//! these methods only keep the session and release the ciphertext after the code matches.
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

impl App {
    /// Accept a joiner offer. Returns the announce frame and the 6-digit code. The sync token and
    /// this vault's Secret Key stay in the host session until [`App::pairing_host_release`].
    pub fn pairing_host_prepare(
        &self,
        offer: &str,
        repo_owner: &str,
        repo_name: &str,
        branch: &str,
        sync_token: &str,
    ) -> Result<PairingHostPrepared> {
        let (account_id, secret) =
            self.with(|u| Ok((u.session.fmt.account_id.clone(), SecretKey(u.secret.0))))?;
        let request = PairingRequest::new(
            &account_id,
            repo_owner,
            repo_name,
            branch,
            secret,
            sync_token,
        )?;
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
                code: host.code().to_string(),
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

    /// Release the grant QR when `code` matches the code from [`App::pairing_host_prepare`].
    pub fn pairing_host_release(&self, code: &str) -> Result<String> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let now = pairing::now_unix();
            let (grant, sid, exp) = {
                let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
                let grant = host.grant_if_code_matches(code, now)?;
                (grant, host.session_id().to_string(), host.expires_at())
            };
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            consumed.mark_released(&sid, exp, now)?;
            pairing::save_consumed(&self.settings_dir, &consumed)?;
            if let Some(host) = g.host.as_mut() {
                host.wipe();
            }
            g.host = None;
            Ok(grant)
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
