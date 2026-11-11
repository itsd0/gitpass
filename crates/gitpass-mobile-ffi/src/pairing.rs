//! Bridge for the QR pairing protocol. Private keys stay in this process.
//! `PairedSync` is returned only to the caller that must store it; nothing here logs those fields.
use crate::{err, GitpassError};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, JoinerSession, PairingRequest};
use std::path::Path;
use std::sync::Mutex;
use zeroize::Zeroize;

#[derive(Clone, uniffi::Record)]
pub struct PairedSync {
    pub account_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub secret_key: String,
    pub sync_token: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingHostView {
    pub announce: String,
    pub code: String,
    pub expires_at: i64,
}

/// One argument so the host role is not a long parameter list. `secret_key` and `sync_token`
/// are copied into the Rust session and wiped from this record before the function returns.
#[derive(Clone, uniffi::Record)]
pub struct PairingHostInput {
    pub state_dir: String,
    pub offer: String,
    pub account_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub secret_key: String,
    pub sync_token: String,
}

struct HostHold {
    state_dir: String,
    session: HostSession,
}

struct BridgePairing {
    joiner: Option<JoinerSession>,
    host: Option<HostHold>,
}

static PAIRING: Mutex<BridgePairing> = Mutex::new(BridgePairing {
    joiner: None,
    host: None,
});

fn lock() -> std::sync::MutexGuard<'static, BridgePairing> {
    PAIRING.lock().unwrap_or_else(|e| e.into_inner())
}

fn check_dir(state_dir: &str) -> Result<&Path, GitpassError> {
    if state_dir.is_empty() || state_dir.len() > 4096 || state_dir.contains('\0') {
        return Err(GitpassError::Generic("pairing state unavailable".into()));
    }
    Ok(Path::new(state_dir))
}

fn not_ready() -> GitpassError {
    GitpassError::Generic("no pairing in progress".into())
}

fn paired(payload: gitpass_core::pairing::PairingPayload) -> PairedSync {
    PairedSync {
        account_id: payload.account_id,
        repo_owner: payload.repo_owner,
        repo_name: payload.repo_name,
        branch: payload.branch,
        secret_key: payload.secret_key.encode(),
        sync_token: payload.sync_token.to_string(),
    }
}

#[uniffi::export]
pub fn pairing_joiner_begin() -> Result<String, GitpassError> {
    let session = err(JoinerSession::begin(pairing::now_unix()))?;
    let offer = session.offer();
    lock().joiner = Some(session);
    Ok(offer)
}

#[uniffi::export]
pub fn pairing_joiner_code(announce: String) -> Result<String, GitpassError> {
    let mut g = lock();
    let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
    err(joiner.read_announce(&announce, pairing::now_unix()))
}

#[uniffi::export]
pub fn pairing_joiner_open(
    state_dir: String,
    grant: String,
    code: String,
) -> Result<PairedSync, GitpassError> {
    let dir = check_dir(&state_dir)?;
    let mut consumed = err(pairing::load_consumed(dir))?;
    let now = pairing::now_unix();
    let payload = {
        let mut g = lock();
        let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
        let payload = err(joiner.open(&grant, &code, now, &mut consumed))?;
        let sid = joiner.session_id().to_string();
        let exp = joiner.expires_at();
        err(consumed.mark_released(&sid, exp, now))?;
        err(pairing::save_consumed(dir, &consumed))?;
        joiner.consume();
        payload
    };
    lock().joiner = None;
    Ok(paired(payload))
}

fn restore_host(previous: Option<HostHold>) {
    if let Some(prev) = previous {
        lock().host = Some(prev);
    }
}

fn refused(e: pairing::PairingError) -> GitpassError {
    GitpassError::Generic(e.to_string())
}

#[uniffi::export]
pub fn pairing_host_accept(input: PairingHostInput) -> Result<PairingHostView, GitpassError> {
    let mut input = input;
    let result = accept_host(&mut input);
    input.secret_key.zeroize();
    input.sync_token.zeroize();
    result
}

fn accept_host(input: &mut PairingHostInput) -> Result<PairingHostView, GitpassError> {
    let dir = check_dir(&input.state_dir)?;
    let secret = err(SecretKey::parse(&input.secret_key))?;
    let request = err(PairingRequest::new(
        &input.account_id,
        &input.repo_owner,
        &input.repo_name,
        &input.branch,
        secret,
        &input.sync_token,
    ))?;
    let previous = lock().host.take();
    let mut consumed = match pairing::load_consumed(dir) {
        Ok(c) => c,
        Err(e) => {
            restore_host(previous);
            return Err(refused(e));
        }
    };
    if let Some(prev) = previous.as_ref().filter(|p| p.state_dir == input.state_dir) {
        consumed.cancel_accept(prev.session.session_id());
    }
    let now = pairing::now_unix();
    let host = match HostSession::accept(&input.offer, request, now, &mut consumed) {
        Ok(host) => host,
        Err(e) => {
            restore_host(previous);
            return Err(refused(e));
        }
    };
    if let Err(e) = pairing::save_consumed(dir, &consumed) {
        restore_host(previous);
        return Err(refused(e));
    }
    if let Some(prev) = previous.as_ref().filter(|p| p.state_dir != input.state_dir) {
        if let Ok(prev_dir) = check_dir(&prev.state_dir) {
            if let Ok(mut prev_consumed) = pairing::load_consumed(prev_dir) {
                prev_consumed.cancel_accept(prev.session.session_id());
                let _ = pairing::save_consumed(prev_dir, &prev_consumed);
            }
        }
    }
    let view = PairingHostView {
        announce: host.announce(),
        code: host.code().to_string(),
        expires_at: host.expires_at(),
    };
    lock().host = Some(HostHold {
        state_dir: input.state_dir.clone(),
        session: host,
    });
    Ok(view)
}

#[uniffi::export]
pub fn pairing_host_release(state_dir: String, code: String) -> Result<String, GitpassError> {
    let dir = check_dir(&state_dir)?;
    let mut g = lock();
    let now = pairing::now_unix();
    let (grant, sid, exp) = {
        let hold = g.host.as_mut().ok_or_else(not_ready)?;
        if hold.state_dir != state_dir {
            return Err(not_ready());
        }
        let grant = err(hold.session.grant_if_code_matches(&code, now))?;
        (
            grant,
            hold.session.session_id().to_string(),
            hold.session.expires_at(),
        )
    };
    let mut consumed = err(pairing::load_consumed(dir))?;
    err(consumed.mark_released(&sid, exp, now))?;
    err(pairing::save_consumed(dir, &consumed))?;
    if let Some(hold) = g.host.as_mut() {
        hold.session.wipe();
    }
    g.host = None;
    Ok(grant)
}

#[uniffi::export]
pub fn pairing_host_cancel(state_dir: String) -> Result<(), GitpassError> {
    let _ = check_dir(&state_dir)?;
    let previous = lock().host.take();
    if let Some(prev) = previous {
        let dir = check_dir(&prev.state_dir)?;
        let mut consumed = err(pairing::load_consumed(dir))?;
        consumed.cancel_accept(prev.session.session_id());
        err(pairing::save_consumed(dir, &consumed))?;
    }
    Ok(())
}

/// Synthetic vector JSON. Fake token and Secret Key only.
#[uniffi::export]
pub fn pairing_test_vector() -> Result<String, GitpassError> {
    err(pairing::test_vector_json())
}

/// Open the synthetic vector inside core and return the payload fields.
#[uniffi::export]
pub fn pairing_test_vector_open() -> Result<PairedSync, GitpassError> {
    Ok(paired(err(pairing::test_vector_open())?))
}
