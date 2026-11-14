//! Bridge for pairing v2. Private keys stay in this process and are dropped
//! when the session succeeds, is cancelled, or expires.
//!
//! The phone shows [`pairing_joiner_code`]. The desktop host view has no code.
//! The person types the phone's code into the desktop. Platform code only
//! moves the returned bytes; it does not derive keys or open the grant.
//! [`PairedSync::secret_key`] and [`PairedSync::sync_token`] are explicit reads.
//! Never log them.
use crate::{err, GitpassError};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, JoinerSession, PairingError, PairingRequest, PairingStep};
use std::path::Path;
use std::sync::{Arc, Mutex};
use zeroize::{Zeroize, Zeroizing};

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

/// What the desktop shows. There is no code field.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingHostView {
    pub qr: String,
    pub expires_at: i64,
    pub port: u16,
    pub addresses: Vec<String>,
}

/// Where the phone connects. Addresses are the IPs from the QR, not names.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingEndpoint {
    pub port: u16,
    pub addresses: Vec<String>,
    pub expires_at: i64,
}

/// One step of the byte stream. `error` is empty on success and is otherwise a fixed string.
/// `outbound` is what the platform writes next, including a fail frame.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingDrive {
    pub outbound: Vec<u8>,
    pub phase: String,
    pub error: String,
}

/// Arguments for the host role. Secrets are wiped after they are copied into the session.
/// Do not log this value.
#[derive(uniffi::Object)]
pub struct PairingHostInput {
    state_dir: String,
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: Mutex<Zeroizing<String>>,
    sync_token: Mutex<Zeroizing<String>>,
    addresses_json: String,
    port: u16,
}

impl std::fmt::Debug for PairingHostInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingHostInput")
            .field("state_dir", &self.state_dir)
            .field("account_id", &self.account_id)
            .field("repo_owner", &self.repo_owner)
            .field("port", &self.port)
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
    /// Foreign constructor. Secrets are wiped after the host session is built.
    #[allow(clippy::too_many_arguments)]
    #[uniffi::constructor]
    pub fn new(
        state_dir: String,
        account_id: String,
        repo_owner: String,
        repo_name: String,
        branch: String,
        secret_key: String,
        sync_token: String,
        addresses_json: String,
        port: u16,
    ) -> Arc<Self> {
        Arc::new(Self {
            state_dir,
            account_id,
            repo_owner,
            repo_name,
            branch,
            secret_key: Mutex::new(Zeroizing::new(secret_key)),
            sync_token: Mutex::new(Zeroizing::new(sync_token)),
            addresses_json,
            port,
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

fn paired(mut payload: gitpass_core::pairing::PairingPayload) -> Arc<PairedSync> {
    let secret_key = payload.secret_key.encode();
    payload.secret_key.0.zeroize();
    Arc::new(PairedSync {
        account_id: std::mem::take(&mut payload.account_id),
        repo_owner: std::mem::take(&mut payload.repo_owner),
        repo_name: std::mem::take(&mut payload.repo_name),
        branch: std::mem::take(&mut payload.branch),
        secret_key,
        sync_token: std::mem::take(&mut *payload.sync_token),
        token_kind: pairing::TOKEN_KIND_FINE_GRAINED_PAT.to_string(),
    })
}

fn drive(step: PairingStep, phase: &str) -> PairingDrive {
    let failed = step.error.is_some();
    PairingDrive {
        outbound: step.outbound,
        phase: if failed { "failed".to_string() } else { phase.to_string() },
        error: step.error.map(|e| e.to_string()).unwrap_or_default(),
    }
}

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

fn parse_addresses(json: &str) -> Result<Vec<String>, PairingError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| PairingError::BadMessage)?;
    let arr = value.as_array().ok_or(PairingError::BadMessage)?;
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let s = v.as_str().ok_or(PairingError::BadMessage)?;
        if s.is_empty() || s.len() > 64 {
            return Err(PairingError::BadMessage);
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// Scan result: keep the session and return the first frame to write.
/// `state_dir` is the phone's replay record (session ids only).
#[uniffi::export]
pub fn pairing_joiner_start(qr: String, state_dir: String) -> Result<Vec<u8>, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&state_dir)?;
    let mut consumed = err(pairing::load_consumed(dir))?;
    let mut session = err(JoinerSession::from_qr(&qr, now, &mut consumed))?;
    let step = session.push(&[], now);
    if let Some(e) = step.error {
        return Err(refused(e));
    }
    let outbound = step.outbound;
    lock().joiner = Some(session);
    Ok(outbound)
}

/// IPs and port from the scanned QR. No secrets.
#[uniffi::export]
pub fn pairing_joiner_endpoint() -> Result<PairingEndpoint, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let g = lock();
    let joiner = g.joiner.as_ref().ok_or_else(not_ready)?;
    Ok(PairingEndpoint {
        port: joiner.port(),
        addresses: joiner.addresses().to_vec(),
        expires_at: joiner.expires_at(),
    })
}

/// Feed bytes read from the socket. Returns the next bytes to write.
#[uniffi::export]
pub fn pairing_joiner_push(chunk: Vec<u8>) -> Result<PairingDrive, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let mut g = lock();
    let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
    let step = joiner.push(&chunk, now);
    let phase = joiner.phase().to_string();
    Ok(drive(step, &phase))
}

/// Code for the phone to display. The desktop does not have a matching call.
#[uniffi::export]
pub fn pairing_joiner_code() -> Result<String, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let g = lock();
    let joiner = g.joiner.as_ref().ok_or_else(not_ready)?;
    err(joiner.code().map(|s| s.to_string()))
}

/// Phase: `wait`, `code`, `done` or `failed`.
#[uniffi::export]
pub fn pairing_joiner_phase() -> String {
    let now = pairing::now_unix();
    reap_expired(now);
    lock()
        .joiner
        .as_ref()
        .map(|j| j.phase().to_string())
        .unwrap_or_else(|| "idle".to_string())
}

#[uniffi::export]
pub fn pairing_joiner_cancel() {
    lock().joiner = None;
}

/// Persist the single-use record and return the opened grant. Never log the result.
#[uniffi::export]
pub fn pairing_joiner_finish(state_dir: String) -> Result<Arc<PairedSync>, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&state_dir)?.to_path_buf();
    let mut g = lock();
    let joiner = g.joiner.as_mut().ok_or_else(not_ready)?;
    if joiner.phase() != "done" {
        return Err(not_ready());
    }
    let sid = joiner.session_id().to_string();
    let exp = joiner.expires_at();
    let mut consumed = err(pairing::load_consumed(&dir))?;
    err(consumed.mark_released(&sid, exp, now))?;
    err(pairing::save_consumed(&dir, &consumed))?;
    let payload = err(joiner.take_payload())?;
    joiner.wipe();
    g.joiner = None;
    Ok(paired(payload))
}

/// Start the desktop side. The returned view has the QR and no code.
#[uniffi::export]
pub fn pairing_host_begin(input: Arc<PairingHostInput>) -> Result<PairingHostView, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&input.state_dir)?.to_path_buf();
    let addresses = err(parse_addresses(&input.addresses_json))?;
    let secret = take_secret(&input.secret_key);
    let token = take_secret(&input.sync_token);
    let request = err(PairingRequest::new(
        &input.account_id,
        &input.repo_owner,
        &input.repo_name,
        &input.branch,
        err(SecretKey::parse(&secret).map_err(|_| PairingError::BadField))?,
        &token,
    ))?;
    let mut consumed = err(pairing::load_consumed(&dir))?;
    if let Some(prev) = lock().host.take() {
        consumed.cancel_accept(prev.session.session_id());
    }
    let (session, qr) = match HostSession::begin(&addresses, input.port, request, now, &mut consumed) {
        Ok(v) => v,
        Err(e) => {
            let _ = pairing::save_consumed(&dir, &consumed);
            return Err(refused(e));
        }
    };
    err(pairing::save_consumed(&dir, &consumed))?;
    let view = PairingHostView {
        qr,
        expires_at: session.expires_at(),
        port: session.port(),
        addresses: session.addresses().to_vec(),
    };
    lock().host = Some(HostHold {
        state_dir: input.state_dir.clone(),
        session,
    });
    Ok(view)
}

#[uniffi::export]
pub fn pairing_host_push(state_dir: String, chunk: Vec<u8>) -> Result<PairingDrive, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let mut g = lock();
    let hold = g.host.as_mut().ok_or_else(not_ready)?;
    if hold.state_dir != state_dir {
        return Err(not_ready());
    }
    let step = hold.session.push(&chunk, now);
    let phase = hold.session.phase().to_string();
    let failed = step.error.is_some();
    let out = drive(step, &phase);
    if failed {
        hold.session.wipe();
    }
    Ok(out)
}

#[uniffi::export]
pub fn pairing_host_confirm(state_dir: String, code: String) -> Result<PairingDrive, GitpassError> {
    let now = pairing::now_unix();
    reap_expired(now);
    let dir = check_dir(&state_dir)?.to_path_buf();
    let mut g = lock();
    let hold = g.host.as_mut().ok_or_else(not_ready)?;
    if hold.state_dir != state_dir {
        return Err(not_ready());
    }
    let sid = hold.session.session_id().to_string();
    let exp = hold.session.expires_at();
    let step = hold.session.confirm(&code, now);
    let phase = hold.session.phase().to_string();
    let ended = matches!(
        step.error,
        Some(PairingError::Exhausted | PairingError::Expired | PairingError::Replay)
    );
    let succeeded = step.error.is_none() && phase == "done";
    let out = drive(step, &phase);
    if succeeded || ended {
        let mut consumed = err(pairing::load_consumed(&dir))?;
        if succeeded || matches!(out.error.as_str(), "too many wrong pairing codes" | "pairing session expired") {
            err(consumed.mark_released(&sid, exp, now))?;
            err(pairing::save_consumed(&dir, &consumed))?;
        }
        hold.session.wipe();
        g.host = None;
    }
    Ok(out)
}

/// Drop the host session only when `state_dir` is the one that started it.
#[uniffi::export]
pub fn pairing_host_cancel(state_dir: String) -> Result<(), GitpassError> {
    let dir = check_dir(&state_dir)?.to_path_buf();
    let mut g = lock();
    let Some(hold) = g.host.as_ref() else {
        return Err(not_ready());
    };
    if hold.state_dir != state_dir {
        return Err(not_ready());
    }
    let sid = hold.session.session_id().to_string();
    g.host = None;
    let mut consumed = err(pairing::load_consumed(&dir))?;
    consumed.cancel_accept(&sid);
    err(pairing::save_consumed(&dir, &consumed))?;
    Ok(())
}

#[uniffi::export]
pub fn pairing_test_vector() -> Result<String, GitpassError> {
    err(pairing::test_vector_json())
}

#[uniffi::export]
pub fn pairing_test_vector_open() -> Result<Arc<PairedSync>, GitpassError> {
    Ok(paired(err(pairing::test_vector_open())?))
}
