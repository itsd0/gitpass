//! Bridge for the QR pairing protocol. Private keys stay in this process and are dropped
//! when the session succeeds, is cancelled, or expires.
//!
//! #458: call [`pairing_joiner_code`] on the phone and show that string there. The desktop
//! host view has no code. The person types the phone's code into the desktop.
//! [`PairedSync::secret_key`] and [`PairedSync::sync_token`] are explicit reads. Never log them.
use crate::{err, GitpassError};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, JoinerSession, PairingError, PairingRequest};
use std::path::Path;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// Opened grant. `Debug` and `Display` redact the Secret Key and the sync token.
/// The generated foreign object does not put those fields in `toString` / `description`.
#[derive(uniffi::Object)]
pub struct PairedSync {
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: String,
    sync_token: String,
    token_kind: String,
}

impl std::fmt::Debug for PairedSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedSync")
            .field("account_id", &self.account_id)
            .field("repo_owner", &self.repo_owner)
            .field("repo_name", &self.repo_name)
            .field("branch", &self.branch)
            .field("token_kind", &self.token_kind)
            .field("secret_key", &"<redacted>")
            .field("sync_token", &"<redacted>")
            .finish()
    }
}

impl std::fmt::Display for PairedSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairedSync([redacted])")
    }
}

#[uniffi::export]
impl PairedSync {
    pub fn account_id(&self) -> String {
        self.account_id.clone()
    }
    pub fn repo_owner(&self) -> String {
        self.repo_owner.clone()
    }
    pub fn repo_name(&self) -> String {
        self.repo_name.clone()
    }
    pub fn branch(&self) -> String {
        self.branch.clone()
    }
    /// Account Secret Key. Never log the return value.
    pub fn secret_key(&self) -> String {
        self.secret_key.clone()
    }
    /// Fine-grained GitHub PAT. Never log the return value.
    pub fn sync_token(&self) -> String {
        self.sync_token.clone()
    }
    pub fn token_kind(&self) -> String {
        self.token_kind.clone()
    }
}

/// What the desktop shows: the announce QR and when it expires. No code.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingHostView {
    pub announce: String,
    pub expires_at: i64,
}

/// Arguments for the host role. Secrets are wiped after they are copied into the session.
/// Do not log this value.
#[derive(uniffi::Object)]
pub struct PairingHostInput {
    state_dir: String,
    offer: String,
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: Mutex<Zeroizing<String>>,
    sync_token: Mutex<Zeroizing<String>>,
}

impl std::fmt::Debug for PairingHostInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingHostInput")
            .field("state_dir", &self.state_dir)
            .field("account_id", &self.account_id)
            .field("repo_owner", &self.repo_owner)
            .field("secret_key", &"<redacted>")
            .field("sync_token", &"<redacted>")
            .finish()
    }
}

impl std::fmt::Display for PairingHostInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingHostInput([redacted])")
    }
}

#[uniffi::export]
impl PairingHostInput {
    /// Foreign constructor. The argument count is the pairing input; secrets are wiped after accept.
    #[allow(clippy::too_many_arguments)]
    #[uniffi::constructor]
    pub fn new(
        state_dir: String,
        offer: String,
        account_id: String,
        repo_owner: String,
        repo_name: String,
        branch: String,
        secret_key: String,
        sync_token: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            state_dir,
            offer,
            account_id,
            repo_owner,
            repo_name,
            branch,
            secret_key: Mutex::new(Zeroizing::new(secret_key)),
            sync_token: Mutex::new(Zeroizing::new(sync_token)),
        })
    }
}

struct HostHold {
    state_dir: String,
    session: HostSession,
}

struct BridgePairing {
    joiner: Option<JoinerSession>,
    host: Option<HostHold>,
}

static PAIRING: Mutex<BridgePairing> = Mutex::new(BridgePairing { joiner: None, host: None });

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

fn refused(e: PairingError) -> GitpassError {
    GitpassError::Generic(e.to_string())
}

fn take_secret(slot: &Mutex<Zeroizing<String>>) -> Zeroizing<String> {
    let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *guard)
}

fn paired(payload: gitpass_core::pairing::PairingPayload) -> Arc<PairedSync> {
    Arc::new(PairedSync {
        account_id: payload.account_id,
        repo_owner: payload.repo_owner,
        repo_name: payload.repo_name,
        branch: payload.branch,
        secret_key: payload.secret_key.encode(),
        sync_token: payload.sync_token.to_string(),
        token_kind: pairing::TOKEN_KIND_FINE_GRAINED_PAT.to_string(),
    })
}

/// Drop an in-memory private key once its offer has expired.
fn reap_expired(now: i64) {
    let mut g = lock();
    if g.joiner.as_ref().is_some_and(|j| now >= j.expires_at()) {
        g.joiner = None;
    }
    if g.host.as_ref().is_some_and(|h| now >= h.session.expires_at()) {
        if let Some(hold) = g.host.as_mut() {
            hold.session.wipe();
        }
        g.host = None;
    }
}

#[uniffi::export]
pub fn pairing_joiner_begin() -> Result<String, GitpassError> {
    let session = err(JoinerSession::begin(pairing::now_unix()))?;
    let offer = session.offer();
    lock().joiner = Some(session);
    Ok(offer)
}

/// Code for the phone to display. The desktop does not have a matching call.
#[uniffi::export]
pub fn pairing_joiner_code(announce: String) -> Result<String, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let mut g = lock();
    let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
    err(joiner.read_announce(&announce, now))
}

/// Forget the joiner's ephemeral private key.
#[uniffi::export]
pub fn pairing_joiner_cancel() {
    lock().joiner = None;
}

#[uniffi::export]
pub fn pairing_joiner_open(state_dir: String, grant: String, code: String) -> Result<Arc<PairedSync>, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&state_dir)?;
    let mut consumed = err(pairing::load_consumed(dir))?;
    let outcome = {
        let mut g = lock();
        let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
        match joiner.open(&grant, &code, now, &mut consumed) {
            Ok(payload) => {
                let sid = joiner.session_id().to_string();
                let exp = joiner.expires_at();
                Ok((payload, sid, exp))
            }
            Err(PairingError::Exhausted) => {
                let sid = joiner.session_id().to_string();
                let exp = joiner.expires_at();
                Err((true, sid, exp))
            }
            Err(e) => Err((false, e.to_string(), 0)),
        }
    };
    match outcome {
        Ok((payload, sid, exp)) => {
            err(consumed.mark_released(&sid, exp, now))?;
            err(pairing::save_consumed(dir, &consumed))?;
            if let Some(joiner) = lock().joiner.as_mut() {
                joiner.consume();
            }
            lock().joiner = None;
            Ok(paired(payload))
        }
        Err((true, sid, exp)) => {
            lock().joiner = None;
            err(consumed.mark_released(&sid, exp, now))?;
            err(pairing::save_consumed(dir, &consumed))?;
            Err(refused(PairingError::Exhausted))
        }
        Err((false, message, _)) => Err(GitpassError::Generic(message)),
    }
}

fn restore_host(previous: Option<HostHold>) {
    if let Some(prev) = previous {
        lock().host = Some(prev);
    }
}

#[uniffi::export]
pub fn pairing_host_accept(input: Arc<PairingHostInput>) -> Result<PairingHostView, GitpassError> {
    let secret_key = take_secret(&input.secret_key);
    let sync_token = take_secret(&input.sync_token);
    let dir = check_dir(&input.state_dir)?;
    let secret = err(SecretKey::parse(&secret_key))?;
    let request = err(PairingRequest::new(
        &input.account_id,
        &input.repo_owner,
        &input.repo_name,
        &input.branch,
        secret,
        &sync_token,
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
    let view = PairingHostView { announce: host.announce(), expires_at: host.expires_at() };
    lock().host = Some(HostHold { state_dir: input.state_dir.clone(), session: host });
    Ok(view)
}

#[uniffi::export]
pub fn pairing_host_release(state_dir: String, code: String) -> Result<String, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&state_dir)?;
    let mut g = lock();
    enum Step {
        Grant(String, String, i64),
        Burned(String, i64),
        Refuse(PairingError),
    }
    let step = {
        let hold = g.host.as_mut().ok_or_else(not_ready)?;
        if hold.state_dir != state_dir {
            return Err(not_ready());
        }
        let sid = hold.session.session_id().to_string();
        let exp = hold.session.expires_at();
        match hold.session.grant_if_code_matches(&code, now) {
            Ok(grant) => Step::Grant(grant, sid, exp),
            Err(PairingError::Exhausted) => Step::Burned(sid, exp),
            Err(e) => Step::Refuse(e),
        }
    };
    match step {
        Step::Grant(grant, sid, exp) => {
            let mut consumed = err(pairing::load_consumed(dir))?;
            err(consumed.mark_released(&sid, exp, now))?;
            err(pairing::save_consumed(dir, &consumed))?;
            if let Some(hold) = g.host.as_mut() {
                hold.session.wipe();
            }
            g.host = None;
            Ok(grant)
        }
        Step::Burned(sid, exp) => {
            let mut consumed = err(pairing::load_consumed(dir))?;
            err(consumed.mark_released(&sid, exp, now))?;
            err(pairing::save_consumed(dir, &consumed))?;
            g.host = None;
            Err(refused(PairingError::Exhausted))
        }
        Step::Refuse(e) => Err(refused(e)),
    }
}

/// Drop the host session when `state_dir` is the directory that accepted it.
#[uniffi::export]
pub fn pairing_host_cancel(state_dir: String) -> Result<(), GitpassError> {
    let _ = check_dir(&state_dir)?;
    let mut g = lock();
    let Some(hold) = g.host.as_ref() else { return Ok(()) };
    if hold.state_dir != state_dir {
        return Err(not_ready());
    }
    let hold = g.host.take().expect("host present");
    drop(g);
    let dir = check_dir(&hold.state_dir)?;
    let mut consumed = err(pairing::load_consumed(dir))?;
    consumed.cancel_accept(hold.session.session_id());
    err(pairing::save_consumed(dir, &consumed))?;
    Ok(())
}

/// Synthetic vector JSON. Fake token and Secret Key only. `code` is what the phone would show.
#[uniffi::export]
pub fn pairing_test_vector() -> Result<String, GitpassError> {
    err(pairing::test_vector_json())
}

/// Open the synthetic vector inside core and return the payload fields.
#[uniffi::export]
pub fn pairing_test_vector_open() -> Result<Arc<PairedSync>, GitpassError> {
    Ok(paired(err(pairing::test_vector_open())?))
}
