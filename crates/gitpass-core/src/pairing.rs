//! Device pairing v2 (#491). The desktop shows one QR. Only the phone scans it.
//! The two devices then speak over a caller-provided byte stream (a LAN TCP
//! connection; this module does not open sockets). The phone shows a 6-digit
//! code. The person types it on the desktop. The desktop compares it in
//! constant time and only then sends the encrypted grant.
//!
//! The master password is never an input. The QR contains no secrets.
//!
//! # QR (desktop → phone)
//!
//! `gitpass-pair:2:qr:<sid>.<exp>.<port>.<addrs>.<host_pub>`
//!
//! `<sid>` and `<host_pub>` are unpadded URL-safe base64. `<exp>` is the
//! absolute expiry (Unix seconds), two minutes after the desktop starts.
//! `<port>` is the TCP port. `<addrs>` is unpadded URL-safe base64 of a
//! canonical JSON array of IP addresses (no names, no ports). Version 1
//! frames are rejected.
//!
//! # Commitment
//!
//! An active LAN peer can swap the phone's key. The exchange removes the
//! offline grind that a 6-digit code would otherwise allow:
//!
//! 1. The phone sends `SHA-256(phone_pub || nonce_p)` and does not reveal
//!    either value yet.
//! 2. The desktop then reveals a fresh `nonce_d` and repeats its public key.
//! 3. The phone opens the commitment. A different key does not match the hash.
//! 4. Both sides derive the code from the full transcript. The desktop typed
//!    check therefore gets one guess per session (at most 1 in 10^6).
//!
//! # Transcript and keys
//!
//! The transcript is version, session id, expiry, port, the address JSON,
//! both public keys, the commitment and both nonces. HKDF-SHA256 uses the
//! X25519 shared secret as input, `SHA-256(transcript)` as salt, and the
//! distinct info strings `gitpass/pair/v2/aead` and `gitpass/pair/v2/sas`.
//! The grant is the vault's XChaCha20-Poly1305 envelope with
//! `gitpass/pair/v2 || salt` as additional data.
//!
//! # Frames
//!
//! A frame is `u32be` length (1..=4096, counting the type byte), one type
//! byte, then the body. Types: commit (1), reveal (2), open (3), ready (4),
//! grant (5), fail (6). A declared length above 4096 is rejected before the
//! body is buffered. `finish` rejects a leftover partial frame.
//!
//! The desktop API does not return the code. Three wrong codes end the
//! session. Session ids are single-use via [`ConsumedPairings`].
//!
//! Device registration is not done here. After the person types the master
//! password, `Session::unlock_device` adds the phone to `devices.enc`.

use crate::crypto::{hkdf32, open, random_bytes, seal_with_nonce, Envelope, Key32, SecretKey};
use crate::device;
use crate::fsutil;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64U, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

/// Protocol version carried in the QR, the transcript and the sealed payload.
pub const PAIRING_VERSION: u32 = 2;
/// How long a session stays valid.
pub const PAIRING_TTL_SECS: i64 = 120;
/// Peer clock may be this far ahead when checking `exp`.
pub const PAIRING_CLOCK_SKEW_SECS: i64 = 60;
/// Local replay record. Git-ignored. Contains no secrets.
pub const CONSUMED_FILE: &str = ".gitpass-pairing-consumed.json";

const MAX_MESSAGE_BYTES: usize = 4096;
const MAX_FRAME: usize = 4096;
const MAX_INBOX: usize = 16 * 1024;
const RETAIN_AFTER_EXP_SECS: i64 = 24 * 60 * 60;
const MAX_CONSUMED_ENTRIES: usize = 1024;
const MAX_CONSUMED_BYTES: usize = 256 * 1024;
const MAX_ADDRS: usize = 8;
const PAYLOAD_KEYS: [&str; 8] = [
    "v",
    "account_id",
    "repo_owner",
    "repo_name",
    "branch",
    "secret_key",
    "sync_token",
    "token_kind",
];
/// Only value accepted for `token_kind`.
pub const TOKEN_KIND_FINE_GRAINED_PAT: &str = "fine_grained_pat";
const MAX_WRONG_CODES: u8 = 3;

const T_COMMIT: u8 = 1;
const T_REVEAL: u8 = 2;
const T_OPEN: u8 = 3;
const T_READY: u8 = 4;
const T_GRANT: u8 = 5;
const T_FAIL: u8 = 6;
const FAIL_REFUSED: u8 = 1;
const FAIL_EXPIRED: u8 = 2;

const COMMIT_LEN: usize = 16 + 8 + 32;
const REVEAL_LEN: usize = 16 + 32 + 32;
const OPEN_LEN: usize = 32 + 32;

/// Why a pairing step was refused. Display text never includes secrets, paths or message bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingError {
    Expired,
    Replay,
    BadCode,
    Tampered,
    BadVersion,
    BadMessage,
    LowOrder,
    BadField,
    State,
    NotReady,
    /// Three wrong codes. The session is wiped and must be marked used.
    Exhausted,
    Oversized,
    Truncated,
    WrongSession,
    /// The peer sent a fail frame, or the session can no longer continue.
    Failed,
}

impl PairingError {
    fn as_str(self) -> &'static str {
        match self {
            Self::Expired => "pairing session expired",
            Self::Replay => "pairing session already used",
            Self::BadCode => "pairing verification code does not match",
            Self::Tampered => "pairing payload failed authentication",
            Self::BadVersion => "unsupported pairing protocol version",
            Self::BadMessage => "unrecognized pairing message",
            Self::LowOrder => "pairing key rejected",
            Self::BadField => "pairing payload field rejected",
            Self::State => "pairing state unavailable",
            Self::NotReady => "no pairing in progress",
            Self::Exhausted => "too many wrong pairing codes",
            Self::Oversized => "pairing message too large",
            Self::Truncated => "pairing message truncated",
            Self::WrongSession => "pairing session does not match",
            Self::Failed => "pairing failed",
        }
    }

    fn fail_byte(self) -> u8 {
        if self == Self::Expired {
            FAIL_EXPIRED
        } else {
            FAIL_REFUSED
        }
    }
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl std::error::Error for PairingError {}

/// Bytes to write, and a fixed error when the step did not succeed.
/// `outbound` is still meaningful on failure: it is the fail frame, when one was queued.
#[derive(Debug)]
pub struct PairingStep {
    pub outbound: Vec<u8>,
    pub error: Option<PairingError>,
}

/// Unix time in seconds. `0` if the clock is before the epoch.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Secrets the host seals into the grant. There is no master-password field.
pub struct PairingRequest {
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: SecretKey,
    sync_token: Zeroizing<String>,
}

impl PairingRequest {
    pub fn new(
        account_id: &str,
        repo_owner: &str,
        repo_name: &str,
        branch: &str,
        secret_key: SecretKey,
        sync_token: &str,
    ) -> Result<Self, PairingError> {
        validate_account(account_id)?;
        validate_owner(repo_owner)?;
        validate_repo(repo_name)?;
        validate_branch(branch)?;
        validate_token(sync_token)?;
        Ok(Self {
            account_id: account_id.to_string(),
            repo_owner: repo_owner.to_string(),
            repo_name: repo_name.to_string(),
            branch: branch.to_string(),
            secret_key,
            sync_token: Zeroizing::new(sync_token.to_string()),
        })
    }
}

impl Drop for PairingRequest {
    fn drop(&mut self) {
        self.secret_key.0.zeroize();
    }
}

impl std::fmt::Debug for PairingRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingRequest")
            .field("account_id", &self.account_id)
            .field("repo_owner", &self.repo_owner)
            .field("repo_name", &self.repo_name)
            .field("branch", &self.branch)
            .field("secret_key", &"<redacted>")
            .field("sync_token", &"<redacted>")
            .finish()
    }
}

/// What the joiner learns. Secret fields are redacted in `Debug` and wiped on drop.
pub struct PairingPayload {
    pub account_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub secret_key: SecretKey,
    pub sync_token: Zeroizing<String>,
}

impl Drop for PairingPayload {
    fn drop(&mut self) {
        self.secret_key.0.zeroize();
    }
}

impl std::fmt::Debug for PairingPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingPayload")
            .field("account_id", &self.account_id)
            .field("repo_owner", &self.repo_owner)
            .field("repo_name", &self.repo_name)
            .field("branch", &self.branch)
            .field("secret_key", &"<redacted>")
            .field("sync_token", &"<redacted>")
            .finish()
    }
}

/// Replay record. Public session ids only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsumedPairings {
    accepted: BTreeMap<String, i64>,
    released: BTreeMap<String, i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumedFile {
    v: u32,
    #[serde(default)]
    accepted: BTreeMap<String, i64>,
    #[serde(default)]
    released: BTreeMap<String, i64>,
}

fn sane_sid_map(m: &BTreeMap<String, i64>) -> bool {
    m.len() <= MAX_CONSUMED_ENTRIES
        && m.iter().all(|(k, exp)| {
            !k.is_empty()
                && k.len() <= 64
                && *exp > 0
                && k.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

impl ConsumedPairings {
    pub fn prune(&mut self, now: i64) {
        let keep = |exp: &i64| now < exp.saturating_add(RETAIN_AFTER_EXP_SECS);
        self.accepted.retain(|_, exp| keep(exp));
        self.released.retain(|_, exp| keep(exp));
        while self.released.len() > MAX_CONSUMED_ENTRIES {
            let oldest = self
                .released
                .iter()
                .min_by_key(|(_, exp)| *exp)
                .map(|(k, _)| k.clone());
            let Some(k) = oldest else { break };
            self.released.remove(&k);
        }
    }
    pub fn is_used(&mut self, sid: &str, now: i64) -> bool {
        self.prune(now);
        self.accepted.contains_key(sid) || self.released.contains_key(sid)
    }
    pub fn mark_accepted(&mut self, sid: &str, exp: i64, now: i64) -> Result<(), PairingError> {
        self.prune(now);
        if self.accepted.contains_key(sid) || self.released.contains_key(sid) {
            return Err(PairingError::Replay);
        }
        self.accepted.insert(sid.to_string(), exp);
        Ok(())
    }
    pub fn mark_released(&mut self, sid: &str, exp: i64, now: i64) -> Result<(), PairingError> {
        self.prune(now);
        if self.released.contains_key(sid) {
            return Err(PairingError::Replay);
        }
        self.accepted.remove(sid);
        self.released.insert(sid.to_string(), exp);
        Ok(())
    }
    /// Drop an accept that was never released (cancel, lock). A released id stays released.
    pub fn cancel_accept(&mut self, sid: &str) {
        if !self.released.contains_key(sid) {
            self.accepted.remove(sid);
        }
    }
}

pub fn load_consumed(dir: &Path) -> Result<ConsumedPairings, PairingError> {
    let p = dir.join(CONSUMED_FILE);
    if !p.exists() {
        return Ok(ConsumedPairings::default());
    }
    let bytes = std::fs::read(&p).map_err(|_| PairingError::State)?;
    if bytes.len() > MAX_CONSUMED_BYTES {
        return Err(PairingError::State);
    }
    let file: ConsumedFile = serde_json::from_slice(&bytes).map_err(|_| PairingError::State)?;
    if file.v != 1 || !sane_sid_map(&file.accepted) || !sane_sid_map(&file.released) {
        return Err(PairingError::State);
    }
    Ok(ConsumedPairings {
        accepted: file.accepted,
        released: file.released,
    })
}

pub fn save_consumed(dir: &Path, consumed: &ConsumedPairings) -> Result<(), PairingError> {
    if !sane_sid_map(&consumed.accepted) || !sane_sid_map(&consumed.released) {
        return Err(PairingError::State);
    }
    let file = ConsumedFile {
        v: 1,
        accepted: consumed.accepted.clone(),
        released: consumed.released.clone(),
    };
    let bytes = serde_json::to_vec(&file).map_err(|_| PairingError::State)?;
    device::ensure_ignored(dir, CONSUMED_FILE).map_err(|_| PairingError::State)?;
    fsutil::atomic_write(&dir.join(CONSUMED_FILE), &bytes).map_err(|_| PairingError::State)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitCommit,
    WaitReveal,
    WaitOpen,
    WaitReady,
    WaitCode,
    ShowCode,
    Done,
    Dead,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::WaitCommit | Self::WaitReveal | Self::WaitOpen | Self::WaitReady => "wait",
            Self::WaitCode | Self::ShowCode => "code",
            Self::Done => "done",
            Self::Dead => "failed",
        }
    }
}

struct Transcript {
    sid: [u8; 16],
    exp: i64,
    port: u16,
    addrs_json: String,
    host_pub: [u8; 32],
    commitment: [u8; 32],
    nonce_d: [u8; 32],
    phone_pub: [u8; 32],
    nonce_p: [u8; 32],
}

impl Transcript {
    fn bytes(&self) -> Vec<u8> {
        let aj = self.addrs_json.as_bytes();
        let mut t = Vec::with_capacity(4 + 16 + 8 + 2 + 2 + aj.len() + 32 * 4);
        t.extend_from_slice(&PAIRING_VERSION.to_be_bytes());
        t.extend_from_slice(&self.sid);
        t.extend_from_slice(&self.exp.to_be_bytes());
        t.extend_from_slice(&self.port.to_be_bytes());
        t.extend_from_slice(&(aj.len() as u16).to_be_bytes());
        t.extend_from_slice(aj);
        t.extend_from_slice(&self.host_pub);
        t.extend_from_slice(&self.commitment);
        t.extend_from_slice(&self.nonce_d);
        t.extend_from_slice(&self.phone_pub);
        t.extend_from_slice(&self.nonce_p);
        t
    }
}

/// Desktop side. The 6-digit code is never returned; [`HostSession::confirm`] only compares.
pub struct HostSession {
    secret: Option<StaticSecret>,
    public: [u8; 32],
    sid: [u8; 16],
    sid_b64: String,
    exp: i64,
    port: u16,
    addresses: Vec<String>,
    addrs_json: String,
    qr: String,
    request: Option<PairingRequest>,
    commitment: Option<[u8; 32]>,
    nonce_d: Option<[u8; 32]>,
    phone_pub: Option<[u8; 32]>,
    nonce_p: Option<[u8; 32]>,
    reveal_nonce: Option<[u8; 32]>,
    grant_nonce: Option<[u8; 24]>,
    cached_grant: Option<Vec<u8>>,
    wrong_codes: u8,
    phase: Phase,
    inbox: Vec<u8>,
    outbox: Vec<u8>,
}

impl std::fmt::Debug for HostSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostSession")
            .field("session_id", &self.sid_b64)
            .field("exp", &self.exp)
            .field("port", &self.port)
            .field("phase", &self.phase.name())
            .finish_non_exhaustive()
    }
}

impl Drop for HostSession {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl HostSession {
    pub fn begin(
        addresses: &[String],
        port: u16,
        request: PairingRequest,
        now: i64,
        consumed: &mut ConsumedPairings,
    ) -> Result<(Self, String), PairingError> {
        Self::begin_parts(addresses, port, request, now, consumed, None, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_parts(
        addresses: &[String],
        port: u16,
        request: PairingRequest,
        now: i64,
        consumed: &mut ConsumedPairings,
        secret_bytes: Option<[u8; 32]>,
        sid_bytes: Option<[u8; 16]>,
        reveal_nonce: Option<[u8; 32]>,
        grant_nonce: Option<[u8; 24]>,
    ) -> Result<(Self, String), PairingError> {
        if now <= 0 {
            return Err(PairingError::BadMessage);
        }
        check_port(port)?;
        let addresses = canonical_addresses(addresses)?;
        let addrs_json = serde_json::to_string(&addresses).map_err(|_| PairingError::BadMessage)?;
        let exp = expiry_from(now)?;
        let secret_bytes = secret_bytes.unwrap_or_else(random_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret).to_bytes();
        if public == [0u8; 32] {
            return Err(PairingError::LowOrder);
        }
        let sid = sid_bytes.unwrap_or_else(random_bytes);
        let sid_b64 = b64(&sid);
        if consumed.is_used(&sid_b64, now) {
            return Err(PairingError::Replay);
        }
        let qr = build_qr(&sid, exp, port, &addrs_json, &public)?;
        consumed.mark_accepted(&sid_b64, exp, now)?;
        let session = Self {
            secret: Some(secret),
            public,
            sid,
            sid_b64,
            exp,
            port,
            addresses,
            addrs_json,
            qr: qr.clone(),
            request: Some(request),
            commitment: None,
            nonce_d: None,
            phone_pub: None,
            nonce_p: None,
            reveal_nonce,
            grant_nonce,
            cached_grant: None,
            wrong_codes: 0,
            phase: Phase::WaitCommit,
            inbox: Vec::new(),
            outbox: Vec::new(),
        };
        Ok((session, qr))
    }

    pub fn session_id(&self) -> &str {
        &self.sid_b64
    }
    pub fn expires_at(&self) -> i64 {
        self.exp
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn addresses(&self) -> &[String] {
        &self.addresses
    }
    pub fn qr(&self) -> &str {
        &self.qr
    }
    pub fn phase(&self) -> &'static str {
        self.phase.name()
    }
    /// Still willing to take the first peer frame. False after success, failure or expiry.
    pub fn accepts_peer(&self, now: i64) -> bool {
        self.phase == Phase::WaitCommit && now < self.exp && self.secret.is_some()
    }

    pub fn push(&mut self, chunk: &[u8], now: i64) -> PairingStep {
        if self.phase == Phase::Dead {
            return err_step(PairingError::Replay);
        }
        if self.phase == Phase::Done {
            return err_step(PairingError::Replay);
        }
        if now >= self.exp {
            return self.fail(PairingError::Expired);
        }
        if let Err(e) = self.absorb(chunk) {
            return self.fail(e);
        }
        let mut outbound = Vec::new();
        loop {
            match next_frame(&mut self.inbox) {
                Ok(None) => break,
                Ok(Some((ty, body))) => match self.on_frame(ty, &body) {
                    Ok(()) => outbound.append(&mut self.drain()),
                    Err(e) => {
                        outbound.append(&mut self.drain());
                        let failed = self.fail(e);
                        outbound.extend_from_slice(&failed.outbound);
                        return PairingStep {
                            outbound,
                            error: Some(e),
                        };
                    }
                },
                Err(e) => return self.fail(e),
            }
        }
        outbound.append(&mut self.drain());
        PairingStep {
            outbound,
            error: None,
        }
    }

    /// Compare `code` with the phone's code. Does not return the expected code.
    /// A matching call returns the grant frame. The same frame is returned again
    /// until [`HostSession::wipe`], so a failed save does not mint a second nonce.
    pub fn confirm(&mut self, code: &str, now: i64) -> PairingStep {
        if self.phase == Phase::Done {
            if let Some(frame) = &self.cached_grant {
                return PairingStep {
                    outbound: frame.clone(),
                    error: None,
                };
            }
            return err_step(PairingError::Replay);
        }
        if self.phase != Phase::WaitCode || self.secret.is_none() {
            return err_step(PairingError::NotReady);
        }
        if now >= self.exp {
            return self.fail(PairingError::Expired);
        }
        let entered = match normalize_code(code) {
            Ok(c) => c.to_string(),
            Err(_) => return self.reject_code(),
        };
        let (aead, mut expected) = match self.derive() {
            Ok(v) => v,
            Err(e) => return self.fail(e),
        };
        let matches = ct_eq(expected.as_bytes(), entered.as_bytes());
        expected.zeroize();
        if !matches {
            return self.reject_code();
        }
        if self.cached_grant.is_none() {
            let nonce = self.grant_nonce.take().unwrap_or_else(random_bytes);
            let req = match self.request.as_ref() {
                Some(r) => r,
                None => return self.fail(PairingError::Replay),
            };
            let pt = match payload_bytes(req) {
                Ok(p) => p,
                Err(e) => return self.fail(e),
            };
            let salt = transcript_hash(&self.transcript_bytes());
            let frame = match seal_frame(&aead, &pt, &aad(&salt), nonce) {
                Ok(f) => f,
                Err(e) => {
                    let mut aead = aead;
                    aead.0.zeroize();
                    return self.fail(e);
                }
            };
            let mut aead = aead;
            aead.0.zeroize();
            self.cached_grant = Some(frame);
        }
        // The grant is cached. Drop the private key now; a retry resends the cache.
        self.secret = None;
        self.request = None;
        self.phase = Phase::Done;
        let frame = self.cached_grant.clone().unwrap_or_default();
        PairingStep {
            outbound: frame,
            error: None,
        }
    }

    pub fn wipe(&mut self) {
        self.secret = None;
        self.request = None;
        self.commitment = None;
        self.nonce_d = None;
        self.phone_pub = None;
        self.nonce_p = None;
        self.reveal_nonce = None;
        if let Some(mut g) = self.cached_grant.take() {
            g.zeroize();
        }
        self.inbox.zeroize();
        self.outbox.zeroize();
    }

    /// A finished stream with an incomplete header or body is a truncated frame.
    pub fn finish(&mut self) -> Result<(), PairingError> {
        if self.inbox.is_empty() {
            Ok(())
        } else {
            self.inbox.clear();
            self.wipe();
            self.phase = Phase::Dead;
            Err(PairingError::Truncated)
        }
    }

    fn on_frame(&mut self, ty: u8, body: &[u8]) -> Result<(), PairingError> {
        match (self.phase, ty) {
            (Phase::WaitCommit, T_COMMIT) => self.on_commit(body),
            (Phase::WaitOpen, T_OPEN) => self.on_open(body),
            _ => Err(PairingError::BadMessage),
        }
    }

    fn on_commit(&mut self, body: &[u8]) -> Result<(), PairingError> {
        if body.len() != COMMIT_LEN {
            return Err(PairingError::BadMessage);
        }
        let mut sid = [0u8; 16];
        sid.copy_from_slice(&body[..16]);
        if sid != self.sid {
            return Err(PairingError::WrongSession);
        }
        let exp = i64::from_be_bytes(copy8(&body[16..24]));
        if exp != self.exp {
            return Err(PairingError::BadMessage);
        }
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&body[24..56]);
        self.commitment = Some(commitment);
        let nonce_d = self.reveal_nonce.take().unwrap_or_else(random_bytes);
        self.nonce_d = Some(nonce_d);
        let mut reveal = Vec::with_capacity(REVEAL_LEN);
        reveal.extend_from_slice(&self.sid);
        reveal.extend_from_slice(&nonce_d);
        reveal.extend_from_slice(&self.public);
        self.enqueue(T_REVEAL, &reveal)?;
        self.phase = Phase::WaitOpen;
        Ok(())
    }

    fn on_open(&mut self, body: &[u8]) -> Result<(), PairingError> {
        if body.len() != OPEN_LEN {
            return Err(PairingError::BadMessage);
        }
        let mut phone_pub = [0u8; 32];
        let mut nonce_p = [0u8; 32];
        phone_pub.copy_from_slice(&body[..32]);
        nonce_p.copy_from_slice(&body[32..64]);
        let commitment = self.commitment.ok_or(PairingError::NotReady)?;
        if sha256_2(&phone_pub, &nonce_p) != commitment {
            return Err(PairingError::BadMessage);
        }
        let secret = self.secret.as_ref().ok_or(PairingError::Replay)?;
        let mut shared = dh(secret, &phone_pub)?;
        shared.zeroize();
        self.phone_pub = Some(phone_pub);
        self.nonce_p = Some(nonce_p);
        self.enqueue(T_READY, &[])?;
        self.phase = Phase::WaitCode;
        Ok(())
    }

    fn reject_code(&mut self) -> PairingStep {
        self.wrong_codes = self.wrong_codes.saturating_add(1);
        if self.wrong_codes >= MAX_WRONG_CODES {
            self.fail(PairingError::Exhausted)
        } else {
            err_step(PairingError::BadCode)
        }
    }

    fn derive(&self) -> Result<(Key32, String), PairingError> {
        let secret = self.secret.as_ref().ok_or(PairingError::Replay)?;
        let phone_pub = self.phone_pub.ok_or(PairingError::NotReady)?;
        let mut shared = dh(secret, &phone_pub)?;
        let salt = transcript_hash(&self.transcript_bytes());
        let aead = hkdf32(&shared, Some(&salt), b"gitpass/pair/v2/aead");
        let mut sas = hkdf32(&shared, Some(&salt), b"gitpass/pair/v2/sas");
        shared.zeroize();
        let code = code_from_key(&sas);
        sas.0.zeroize();
        Ok((aead, code))
    }

    fn transcript_bytes(&self) -> Vec<u8> {
        Transcript {
            sid: self.sid,
            exp: self.exp,
            port: self.port,
            addrs_json: self.addrs_json.clone(),
            host_pub: self.public,
            commitment: self.commitment.unwrap_or([0u8; 32]),
            nonce_d: self.nonce_d.unwrap_or([0u8; 32]),
            phone_pub: self.phone_pub.unwrap_or([0u8; 32]),
            nonce_p: self.nonce_p.unwrap_or([0u8; 32]),
        }
        .bytes()
    }

    fn absorb(&mut self, chunk: &[u8]) -> Result<(), PairingError> {
        if chunk.len() > MAX_INBOX || self.inbox.len() + chunk.len() > MAX_INBOX {
            self.inbox.clear();
            return Err(PairingError::Oversized);
        }
        self.inbox.extend_from_slice(chunk);
        Ok(())
    }

    fn enqueue(&mut self, ty: u8, body: &[u8]) -> Result<(), PairingError> {
        self.outbox.extend_from_slice(&encode(ty, body)?);
        Ok(())
    }

    fn drain(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox)
    }

    fn fail(&mut self, e: PairingError) -> PairingStep {
        let _ = self.enqueue(T_FAIL, &[e.fail_byte()]);
        let outbound = self.drain();
        self.wipe();
        self.phase = Phase::Dead;
        PairingStep {
            outbound,
            error: Some(e),
        }
    }
}

/// Phone side. [`JoinerSession::code`] is the string the phone shows.
pub struct JoinerSession {
    secret: Option<StaticSecret>,
    public: [u8; 32],
    nonce_p: [u8; 32],
    commitment: [u8; 32],
    sid: [u8; 16],
    sid_b64: String,
    exp: i64,
    port: u16,
    addresses: Vec<String>,
    addrs_json: String,
    host_pub: [u8; 32],
    nonce_d: Option<[u8; 32]>,
    code: Option<String>,
    payload: Option<PairingPayload>,
    phase: Phase,
    inbox: Vec<u8>,
    outbox: Vec<u8>,
}

impl std::fmt::Debug for JoinerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinerSession")
            .field("session_id", &self.sid_b64)
            .field("exp", &self.exp)
            .field("port", &self.port)
            .field("phase", &self.phase.name())
            .finish_non_exhaustive()
    }
}

impl Drop for JoinerSession {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl JoinerSession {
    pub fn from_qr(
        qr: &str,
        now: i64,
        consumed: &mut ConsumedPairings,
    ) -> Result<Self, PairingError> {
        Self::from_qr_parts(qr, now, consumed, None, None)
    }

    fn from_qr_parts(
        qr: &str,
        now: i64,
        consumed: &mut ConsumedPairings,
        secret_bytes: Option<[u8; 32]>,
        nonce_p: Option<[u8; 32]>,
    ) -> Result<Self, PairingError> {
        if now <= 0 {
            return Err(PairingError::BadMessage);
        }
        let parsed = parse_qr(qr)?;
        check_window(now, parsed.exp)?;
        if consumed.is_used(&parsed.sid_b64, now) {
            return Err(PairingError::Replay);
        }
        let secret_bytes = secret_bytes.unwrap_or_else(random_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret).to_bytes();
        if public == [0u8; 32] {
            return Err(PairingError::LowOrder);
        }
        let nonce_p = nonce_p.unwrap_or_else(random_bytes);
        let commitment = sha256_2(&public, &nonce_p);
        let mut body = Vec::with_capacity(COMMIT_LEN);
        body.extend_from_slice(&parsed.sid);
        body.extend_from_slice(&parsed.exp.to_be_bytes());
        body.extend_from_slice(&commitment);
        let mut session = Self {
            secret: Some(secret),
            public,
            nonce_p,
            commitment,
            sid: parsed.sid,
            sid_b64: parsed.sid_b64,
            exp: parsed.exp,
            port: parsed.port,
            addresses: parsed.addresses,
            addrs_json: parsed.addrs_json,
            host_pub: parsed.host_pub,
            nonce_d: None,
            code: None,
            payload: None,
            phase: Phase::WaitReveal,
            inbox: Vec::new(),
            outbox: Vec::new(),
        };
        session.enqueue(T_COMMIT, &body)?;
        Ok(session)
    }

    pub fn session_id(&self) -> &str {
        &self.sid_b64
    }
    pub fn expires_at(&self) -> i64 {
        self.exp
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn addresses(&self) -> &[String] {
        &self.addresses
    }
    pub fn phase(&self) -> &'static str {
        self.phase.name()
    }

    /// The 6-digit code, only after the desktop has accepted the commitment.
    pub fn code(&self) -> Result<&str, PairingError> {
        self.code.as_deref().ok_or(PairingError::NotReady)
    }

    pub fn push(&mut self, chunk: &[u8], now: i64) -> PairingStep {
        if self.phase == Phase::Dead {
            return err_step(PairingError::Replay);
        }
        if self.phase == Phase::Done {
            return err_step(PairingError::Replay);
        }
        if now >= self.exp {
            self.wipe();
            self.phase = Phase::Dead;
            return err_step(PairingError::Expired);
        }
        if let Err(e) = self.absorb(chunk) {
            self.wipe();
            self.phase = Phase::Dead;
            return err_step(e);
        }
        let mut outbound = Vec::new();
        loop {
            match next_frame(&mut self.inbox) {
                Ok(None) => break,
                Ok(Some((ty, body))) => match self.on_frame(ty, &body) {
                    Ok(()) => outbound.append(&mut self.drain()),
                    Err(e) => {
                        self.wipe();
                        self.phase = Phase::Dead;
                        return PairingStep {
                            outbound,
                            error: Some(e),
                        };
                    }
                },
                Err(e) => {
                    self.wipe();
                    self.phase = Phase::Dead;
                    return err_step(e);
                }
            }
        }
        outbound.append(&mut self.drain());
        PairingStep {
            outbound,
            error: None,
        }
    }

    pub fn take_payload(&mut self) -> Result<PairingPayload, PairingError> {
        self.payload.take().ok_or(PairingError::NotReady)
    }

    pub fn finish(&mut self) -> Result<(), PairingError> {
        if self.inbox.is_empty() {
            Ok(())
        } else {
            self.inbox.clear();
            self.wipe();
            self.phase = Phase::Dead;
            Err(PairingError::Truncated)
        }
    }

    pub fn wipe(&mut self) {
        self.secret = None;
        self.nonce_p.zeroize();
        self.nonce_d = None;
        if let Some(mut c) = self.code.take() {
            c.zeroize();
        }
        self.payload = None;
        self.inbox.zeroize();
        self.outbox.zeroize();
    }

    fn on_frame(&mut self, ty: u8, body: &[u8]) -> Result<(), PairingError> {
        if ty == T_FAIL {
            return self.on_fail(body);
        }
        match (self.phase, ty) {
            (Phase::WaitReveal, T_REVEAL) => self.on_reveal(body),
            (Phase::WaitReady, T_READY) => self.on_ready(body),
            (Phase::ShowCode, T_GRANT) => self.on_grant(body),
            _ => Err(PairingError::BadMessage),
        }
    }

    fn on_reveal(&mut self, body: &[u8]) -> Result<(), PairingError> {
        if body.len() != REVEAL_LEN {
            return Err(PairingError::BadMessage);
        }
        let mut sid = [0u8; 16];
        sid.copy_from_slice(&body[..16]);
        if sid != self.sid {
            return Err(PairingError::WrongSession);
        }
        let mut nonce_d = [0u8; 32];
        let mut host_pub = [0u8; 32];
        nonce_d.copy_from_slice(&body[16..48]);
        host_pub.copy_from_slice(&body[48..80]);
        if host_pub != self.host_pub {
            return Err(PairingError::BadMessage);
        }
        self.nonce_d = Some(nonce_d);
        let mut open_body = Vec::with_capacity(OPEN_LEN);
        open_body.extend_from_slice(&self.public);
        open_body.extend_from_slice(&self.nonce_p);
        self.enqueue(T_OPEN, &open_body)?;
        self.phase = Phase::WaitReady;
        Ok(())
    }

    fn on_ready(&mut self, body: &[u8]) -> Result<(), PairingError> {
        if !body.is_empty() {
            return Err(PairingError::BadMessage);
        }
        let (mut aead, code) = self.derive()?;
        aead.0.zeroize();
        self.code = Some(code);
        self.phase = Phase::ShowCode;
        Ok(())
    }

    fn on_grant(&mut self, body: &[u8]) -> Result<(), PairingError> {
        let (mut aead, _code) = self.derive()?;
        let salt = transcript_hash(&self.transcript_bytes());
        let payload = open_grant(&aead, body, &aad(&salt))?;
        aead.0.zeroize();
        self.payload = Some(payload);
        self.secret = None;
        if let Some(mut c) = self.code.take() {
            c.zeroize();
        }
        self.phase = Phase::Done;
        Ok(())
    }

    fn on_fail(&mut self, body: &[u8]) -> Result<(), PairingError> {
        let expired = body == [FAIL_EXPIRED];
        self.wipe();
        self.phase = Phase::Dead;
        if expired {
            Err(PairingError::Expired)
        } else {
            Err(PairingError::Failed)
        }
    }

    fn derive(&self) -> Result<(Key32, String), PairingError> {
        let secret = self.secret.as_ref().ok_or(PairingError::Replay)?;
        let nonce_d = self.nonce_d.ok_or(PairingError::NotReady)?;
        let mut shared = dh(secret, &self.host_pub)?;
        let salt = transcript_hash(&Transcript {
            sid: self.sid,
            exp: self.exp,
            port: self.port,
            addrs_json: self.addrs_json.clone(),
            host_pub: self.host_pub,
            commitment: self.commitment,
            nonce_d,
            phone_pub: self.public,
            nonce_p: self.nonce_p,
        }
        .bytes());
        let aead = hkdf32(&shared, Some(&salt), b"gitpass/pair/v2/aead");
        let mut sas = hkdf32(&shared, Some(&salt), b"gitpass/pair/v2/sas");
        shared.zeroize();
        let code = code_from_key(&sas);
        sas.0.zeroize();
        Ok((aead, code))
    }

    fn transcript_bytes(&self) -> Vec<u8> {
        Transcript {
            sid: self.sid,
            exp: self.exp,
            port: self.port,
            addrs_json: self.addrs_json.clone(),
            host_pub: self.host_pub,
            commitment: self.commitment,
            nonce_d: self.nonce_d.unwrap_or([0u8; 32]),
            phone_pub: self.public,
            nonce_p: self.nonce_p,
        }
        .bytes()
    }

    fn absorb(&mut self, chunk: &[u8]) -> Result<(), PairingError> {
        if chunk.len() > MAX_INBOX || self.inbox.len() + chunk.len() > MAX_INBOX {
            self.inbox.clear();
            return Err(PairingError::Oversized);
        }
        self.inbox.extend_from_slice(chunk);
        Ok(())
    }

    fn enqueue(&mut self, ty: u8, body: &[u8]) -> Result<(), PairingError> {
        self.outbox.extend_from_slice(&encode(ty, body)?);
        Ok(())
    }

    fn drain(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox)
    }
}

/// Synthetic vector (fixed keys, fake token). For Kotlin and Swift tests via the bridge.
pub fn test_vector_json() -> Result<String, PairingError> {
    let v = vector_parts()?;
    let body = TestVectorOut {
        v: PAIRING_VERSION,
        now: TV_NOW,
        qr: v.qr,
        code: v.code,
        port: v.port,
        addresses: v.addresses,
        commit: b64(&v.commit),
        reveal: b64(&v.reveal),
        open: b64(&v.open),
        ready: b64(&v.ready),
        grant: b64(&v.grant),
        account_id: v.payload.account_id.clone(),
        repo_owner: v.payload.repo_owner.clone(),
        repo_name: v.payload.repo_name.clone(),
        branch: v.payload.branch.clone(),
        secret_key: v.payload.secret_key.encode(),
        sync_token: v.payload.sync_token.to_string(),
        token_kind: TOKEN_KIND_FINE_GRAINED_PAT.to_string(),
    };
    serde_json::to_string(&body).map_err(|_| PairingError::BadMessage)
}

/// Run the synthetic vector and return the opened payload.
pub fn test_vector_open() -> Result<PairingPayload, PairingError> {
    Ok(vector_parts()?.payload)
}

const TV_NOW: i64 = 1_700_000_000;
const TV_TOKEN: &str = "github_pat_FAKE_pairing_test_vector";
const TV_ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";
const TV_PORT: u16 = 27463;

#[derive(Serialize)]
struct TestVectorOut {
    v: u32,
    now: i64,
    qr: String,
    /// The code the phone shows. The live desktop API does not return a code.
    code: String,
    port: u16,
    addresses: Vec<String>,
    commit: String,
    reveal: String,
    open: String,
    ready: String,
    grant: String,
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: String,
    sync_token: String,
    token_kind: String,
}

struct VectorParts {
    qr: String,
    code: String,
    port: u16,
    addresses: Vec<String>,
    commit: Vec<u8>,
    reveal: Vec<u8>,
    open: Vec<u8>,
    ready: Vec<u8>,
    grant: Vec<u8>,
    payload: PairingPayload,
}

fn tv_request() -> Result<PairingRequest, PairingError> {
    PairingRequest::new(
        TV_ACCOUNT,
        "fake-owner",
        "fake-vault",
        "main",
        SecretKey([0xAB; 16]),
        TV_TOKEN,
    )
}

fn vector_parts() -> Result<VectorParts, PairingError> {
    let mut consumed = ConsumedPairings::default();
    let addrs = vec!["127.0.0.1".to_string()];
    let (mut host, qr) = HostSession::begin_parts(
        &addrs,
        TV_PORT,
        tv_request()?,
        TV_NOW,
        &mut consumed,
        Some([0x11; 32]),
        Some([0x55; 16]),
        Some([0x44; 32]),
        Some([0x66; 24]),
    )?;
    let mut joiner = JoinerSession::from_qr_parts(
        &qr,
        TV_NOW,
        &mut ConsumedPairings::default(),
        Some([0x22; 32]),
        Some([0x33; 32]),
    )?;
    let commit = take_ok(joiner.push(&[], TV_NOW))?;
    let reveal = take_ok(host.push(&commit, TV_NOW))?;
    let open = take_ok(joiner.push(&reveal, TV_NOW))?;
    let ready = take_ok(host.push(&open, TV_NOW))?;
    let none = joiner.push(&ready, TV_NOW);
    if none.error.is_some() || !none.outbound.is_empty() {
        return Err(PairingError::BadMessage);
    }
    let code = joiner.code()?.to_string();
    let grant_step = host.confirm(&code, TV_NOW);
    if grant_step.error.is_some() {
        return Err(grant_step.error.unwrap_or(PairingError::BadMessage));
    }
    let grant = grant_step.outbound;
    let opened = joiner.push(&grant, TV_NOW);
    if opened.error.is_some() {
        return Err(opened.error.unwrap_or(PairingError::BadMessage));
    }
    let payload = joiner.take_payload()?;
    Ok(VectorParts {
        qr,
        code,
        port: TV_PORT,
        addresses: addrs,
        commit,
        reveal,
        open,
        ready,
        grant,
        payload,
    })
}

fn take_ok(step: PairingStep) -> Result<Vec<u8>, PairingError> {
    if let Some(e) = step.error {
        return Err(e);
    }
    if step.outbound.is_empty() {
        return Err(PairingError::BadMessage);
    }
    Ok(step.outbound)
}

fn err_step(e: PairingError) -> PairingStep {
    PairingStep {
        outbound: Vec::new(),
        error: Some(e),
    }
}

fn expiry_from(now: i64) -> Result<i64, PairingError> {
    now.checked_add(PAIRING_TTL_SECS)
        .ok_or(PairingError::BadMessage)
}

fn check_window(now: i64, exp: i64) -> Result<(), PairingError> {
    if now >= exp {
        return Err(PairingError::Expired);
    }
    if exp > now.saturating_add(PAIRING_TTL_SECS + PAIRING_CLOCK_SKEW_SECS) {
        return Err(PairingError::BadMessage);
    }
    Ok(())
}

fn check_port(port: u16) -> Result<(), PairingError> {
    if port == 0 {
        Err(PairingError::BadMessage)
    } else {
        Ok(())
    }
}

fn canonical_ip(s: &str) -> Result<String, PairingError> {
    let ip: IpAddr = s.parse().map_err(|_| PairingError::BadMessage)?;
    if ip.is_unspecified() || ip.is_multicast() {
        return Err(PairingError::BadMessage);
    }
    if let IpAddr::V4(v4) = ip {
        if v4.is_broadcast() {
            return Err(PairingError::BadMessage);
        }
    }
    let canon = ip.to_string();
    if canon != s {
        return Err(PairingError::BadMessage);
    }
    Ok(canon)
}

fn canonical_addresses(addrs: &[String]) -> Result<Vec<String>, PairingError> {
    if addrs.is_empty() || addrs.len() > MAX_ADDRS {
        return Err(PairingError::BadMessage);
    }
    addrs.iter().map(|s| canonical_ip(s)).collect()
}

fn build_qr(
    sid: &[u8; 16],
    exp: i64,
    port: u16,
    addrs_json: &str,
    host_pub: &[u8; 32],
) -> Result<String, PairingError> {
    let s = format!(
        "gitpass-pair:{PAIRING_VERSION}:qr:{}.{}.{}.{}.{}",
        b64(sid),
        exp,
        port,
        b64(addrs_json.as_bytes()),
        b64(host_pub)
    );
    if s.len() > MAX_MESSAGE_BYTES || !s.is_ascii() {
        return Err(PairingError::BadMessage);
    }
    Ok(s)
}

struct QrParts {
    sid: [u8; 16],
    sid_b64: String,
    exp: i64,
    port: u16,
    addresses: Vec<String>,
    addrs_json: String,
    host_pub: [u8; 32],
}

fn parse_qr(qr: &str) -> Result<QrParts, PairingError> {
    if qr.len() > MAX_MESSAGE_BYTES || !qr.is_ascii() {
        return Err(PairingError::BadMessage);
    }
    let rest = qr
        .strip_prefix("gitpass-pair:")
        .ok_or(PairingError::BadMessage)?;
    let (ver, rest) = rest.split_once(':').ok_or(PairingError::BadMessage)?;
    if ver != "2" {
        return Err(PairingError::BadVersion);
    }
    let rest = rest.strip_prefix("qr:").ok_or(PairingError::BadMessage)?;
    let mut parts = rest.split('.');
    let sid_b64 = parts.next().ok_or(PairingError::BadMessage)?;
    let exp_s = parts.next().ok_or(PairingError::BadMessage)?;
    let port_s = parts.next().ok_or(PairingError::BadMessage)?;
    let addrs_b64 = parts.next().ok_or(PairingError::BadMessage)?;
    let host_b64 = parts.next().ok_or(PairingError::BadMessage)?;
    if parts.next().is_some() {
        return Err(PairingError::BadMessage);
    }
    let sid = decode_exact::<16>(sid_b64)?;
    let exp = parse_i64(exp_s)?;
    let port = parse_port(port_s)?;
    let addrs_raw = B64U
        .decode(addrs_b64)
        .map_err(|_| PairingError::BadMessage)?;
    if b64(&addrs_raw) != addrs_b64 {
        return Err(PairingError::BadMessage);
    }
    let addrs_json = String::from_utf8(addrs_raw).map_err(|_| PairingError::BadMessage)?;
    let addresses = parse_addrs(&addrs_json)?;
    let host_pub = decode_exact::<32>(host_b64)?;
    Ok(QrParts {
        sid,
        sid_b64: sid_b64.to_string(),
        exp,
        port,
        addresses,
        addrs_json,
        host_pub,
    })
}

fn parse_addrs(json: &str) -> Result<Vec<String>, PairingError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| PairingError::BadMessage)?;
    let arr = value.as_array().ok_or(PairingError::BadMessage)?;
    if arr.is_empty() || arr.len() > MAX_ADDRS {
        return Err(PairingError::BadMessage);
    }
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let s = v.as_str().ok_or(PairingError::BadMessage)?;
        out.push(canonical_ip(s)?);
    }
    let canon = serde_json::to_string(&out).map_err(|_| PairingError::BadMessage)?;
    if canon != json {
        return Err(PairingError::BadMessage);
    }
    Ok(out)
}

fn parse_i64(s: &str) -> Result<i64, PairingError> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PairingError::BadMessage);
    }
    s.parse().map_err(|_| PairingError::BadMessage)
}

fn parse_port(s: &str) -> Result<u16, PairingError> {
    if s.is_empty() || s.len() > 5 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PairingError::BadMessage);
    }
    let n: u32 = s.parse().map_err(|_| PairingError::BadMessage)?;
    if n == 0 || n > u32::from(u16::MAX) {
        return Err(PairingError::BadMessage);
    }
    let port = n as u16;
    if port.to_string() != s {
        return Err(PairingError::BadMessage);
    }
    Ok(port)
}

fn b64(bytes: &[u8]) -> String {
    B64U.encode(bytes)
}

fn decode_exact<const N: usize>(s: &str) -> Result<[u8; N], PairingError> {
    let v = B64U.decode(s).map_err(|_| PairingError::BadMessage)?;
    let a: [u8; N] = v.try_into().map_err(|_| PairingError::BadMessage)?;
    if b64(&a) != s {
        return Err(PairingError::BadMessage);
    }
    Ok(a)
}

fn encode(ty: u8, body: &[u8]) -> Result<Vec<u8>, PairingError> {
    let n = 1 + body.len();
    if n == 0 || n > MAX_FRAME {
        return Err(PairingError::Oversized);
    }
    let mut v = Vec::with_capacity(4 + n);
    v.extend_from_slice(&(n as u32).to_be_bytes());
    v.push(ty);
    v.extend_from_slice(body);
    Ok(v)
}

fn next_frame(buf: &mut Vec<u8>) -> Result<Option<(u8, Vec<u8>)>, PairingError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let n = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if n == 0 {
        buf.clear();
        return Err(PairingError::BadMessage);
    }
    if n > MAX_FRAME {
        buf.clear();
        return Err(PairingError::Oversized);
    }
    if buf.len() < 4 + n {
        return Ok(None);
    }
    let ty = buf[4];
    let body = buf[5..4 + n].to_vec();
    buf.drain(..4 + n);
    Ok(Some((ty, body)))
}

fn copy8(b: &[u8]) -> [u8; 8] {
    let mut a = [0u8; 8];
    a.copy_from_slice(b);
    a
}

fn sha256_2(a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(a);
    h.update(b);
    h.finalize().into()
}

fn transcript_hash(t: &[u8]) -> [u8; 32] {
    Sha256::digest(t).into()
}

fn aad(salt: &[u8; 32]) -> Vec<u8> {
    let mut a = Vec::with_capacity(16 + 32);
    a.extend_from_slice(b"gitpass/pair/v2");
    a.extend_from_slice(salt);
    a
}

fn dh(secret: &StaticSecret, peer: &[u8; 32]) -> Result<[u8; 32], PairingError> {
    if *peer == [0u8; 32] {
        return Err(PairingError::LowOrder);
    }
    let shared = secret.diffie_hellman(&PublicKey::from(*peer));
    if !shared.was_contributory() {
        return Err(PairingError::LowOrder);
    }
    Ok(*shared.as_bytes())
}

fn code_from_key(k: &Key32) -> String {
    let n = u32::from_be_bytes([k.0[0], k.0[1], k.0[2], k.0[3]]) % 1_000_000;
    format!("{n:06}")
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

fn normalize_code(code: &str) -> Result<&str, PairingError> {
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PairingError::BadCode);
    }
    Ok(code)
}

fn seal_frame(aead: &Key32, plaintext: &[u8], aad: &[u8], nonce: [u8; 24]) -> Result<Vec<u8>, PairingError> {
    let env = seal_with_nonce(aead, plaintext, aad, nonce).map_err(|_| PairingError::BadMessage)?;
    let body = serde_json::to_vec(&env).map_err(|_| PairingError::BadMessage)?;
    encode(T_GRANT, &body)
}

fn open_grant(aead: &Key32, body: &[u8], aad: &[u8]) -> Result<PairingPayload, PairingError> {
    let env: Envelope = serde_json::from_slice(body).map_err(|_| PairingError::Tampered)?;
    let pt = open(aead, &env, aad).map_err(|_| PairingError::Tampered)?;
    let pt = Zeroizing::new(pt);
    decode_payload(&pt)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadPlain {
    v: u32,
    account_id: String,
    repo_owner: String,
    repo_name: String,
    branch: String,
    secret_key: String,
    sync_token: String,
    token_kind: String,
}

fn payload_bytes(req: &PairingRequest) -> Result<Zeroizing<Vec<u8>>, PairingError> {
    let mut plain = PayloadPlain {
        v: PAIRING_VERSION,
        account_id: req.account_id.clone(),
        repo_owner: req.repo_owner.clone(),
        repo_name: req.repo_name.clone(),
        branch: req.branch.clone(),
        secret_key: req.secret_key.encode(),
        sync_token: req.sync_token.to_string(),
        token_kind: TOKEN_KIND_FINE_GRAINED_PAT.to_string(),
    };
    let bytes = serde_json::to_vec(&plain).map_err(|_| PairingError::BadField)?;
    plain.secret_key.zeroize();
    plain.sync_token.zeroize();
    Ok(Zeroizing::new(bytes))
}

fn decode_payload(pt: &[u8]) -> Result<PairingPayload, PairingError> {
    let value: serde_json::Value = serde_json::from_slice(pt).map_err(|_| PairingError::Tampered)?;
    let obj = value.as_object().ok_or(PairingError::Tampered)?;
    match obj.get("v") {
        Some(serde_json::Value::Number(n)) if n.as_u64() == Some(u64::from(PAIRING_VERSION)) => {}
        Some(serde_json::Value::Number(_)) => return Err(PairingError::BadVersion),
        _ => return Err(PairingError::BadField),
    }
    if obj.contains_key("password") || obj.contains_key("master_password") {
        return Err(PairingError::BadField);
    }
    if obj.len() != PAYLOAD_KEYS.len() || PAYLOAD_KEYS.iter().any(|k| !obj.contains_key(*k)) {
        return Err(PairingError::BadField);
    }
    let mut plain: PayloadPlain = serde_json::from_value(value).map_err(|_| PairingError::BadField)?;
    let parsed = SecretKey::parse(&plain.secret_key);
    plain.secret_key.zeroize();
    let secret_key = parsed.map_err(|_| PairingError::BadField)?;
    validate_account(&plain.account_id)?;
    validate_owner(&plain.repo_owner)?;
    validate_repo(&plain.repo_name)?;
    validate_branch(&plain.branch)?;
    validate_token(&plain.sync_token)?;
    if plain.token_kind != TOKEN_KIND_FINE_GRAINED_PAT {
        return Err(PairingError::BadField);
    }
    let sync_token = Zeroizing::new(std::mem::take(&mut plain.sync_token));
    Ok(PairingPayload {
        account_id: plain.account_id,
        repo_owner: plain.repo_owner,
        repo_name: plain.repo_name,
        branch: plain.branch,
        secret_key,
        sync_token,
    })
}

fn validate_account(s: &str) -> Result<(), PairingError> {
    let id = uuid::Uuid::parse_str(s).map_err(|_| PairingError::BadField)?;
    if id.to_string() != s {
        return Err(PairingError::BadField);
    }
    Ok(())
}
fn validate_owner(s: &str) -> Result<(), PairingError> {
    let b = s.as_bytes();
    if !(1..=39).contains(&b.len())
        || b[0] == b'-'
        || b[b.len() - 1] == b'-'
        || s.contains("--")
        || !b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-')
    {
        return Err(PairingError::BadField);
    }
    Ok(())
}
fn validate_repo(s: &str) -> Result<(), PairingError> {
    let b = s.as_bytes();
    if !(1..=100).contains(&b.len())
        || s.starts_with('.')
        || s.ends_with('.')
        || s == "."
        || s == ".."
        || !b
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(*c, b'.' | b'_' | b'-'))
    {
        return Err(PairingError::BadField);
    }
    Ok(())
}
fn validate_branch(s: &str) -> Result<(), PairingError> {
    let b = s.as_bytes();
    if !(1..=255).contains(&b.len())
        || s.starts_with('/')
        || s.ends_with('/')
        || s.contains("//")
        || s.contains("..")
        || s.ends_with(".lock")
        || !b
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(*c, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(PairingError::BadField);
    }
    Ok(())
}
fn validate_token(s: &str) -> Result<(), PairingError> {
    if crate::github_api::is_fine_grained_pat(s) {
        Ok(())
    } else {
        Err(PairingError::BadField)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine};

    const PASSWORD: &str = "FAKE-master-password-must-not-appear";
    const TOKEN: &str = "github_pat_FAKE_from_desktop";

    fn req(token: &str) -> PairingRequest {
        PairingRequest::new(
            TV_ACCOUNT,
            "fake-owner",
            "fake-vault",
            "main",
            SecretKey([0xAB; 16]),
            token,
        )
        .unwrap()
    }
    fn wrong(code: &str) -> String {
        if code == "000000" {
            "000001".into()
        } else {
            "000000".into()
        }
    }
    fn assert_clean(text: &str) {
        assert!(!text.contains(PASSWORD), "{text}");
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains(TV_TOKEN));
        assert!(!text.contains("github_pat_"), "{text}");
    }
    fn pair_at(now: i64) -> (HostSession, JoinerSession, String) {
        let mut consumed = ConsumedPairings::default();
        let (host, qr) = HostSession::begin(
            &["127.0.0.1".into()],
            27500,
            req(TOKEN),
            now,
            &mut consumed,
        )
        .unwrap();
        let joiner = JoinerSession::from_qr(&qr, now, &mut ConsumedPairings::default()).unwrap();
        (host, joiner, qr)
    }
    fn handshake(
        host: &mut HostSession,
        joiner: &mut JoinerSession,
        now: i64,
    ) -> String {
        let commit = take_ok(joiner.push(&[], now)).unwrap();
        let reveal = take_ok(host.push(&commit, now)).unwrap();
        let open = take_ok(joiner.push(&reveal, now)).unwrap();
        let ready = take_ok(host.push(&open, now)).unwrap();
        let done = joiner.push(&ready, now);
        assert!(done.error.is_none());
        assert!(done.outbound.is_empty());
        assert_eq!(joiner.phase(), "code");
        assert_eq!(host.phase(), "code");
        joiner.code().unwrap().to_string()
    }

    #[test]
    fn happy_path_and_password_stays_out_of_every_message() {
        let now = 1_700_000_100;
        let (mut host, mut joiner, qr) = pair_at(now);
        assert!(qr.starts_with("gitpass-pair:2:qr:"));
        assert_clean(&qr);
        assert!(!format!("{host:?}").contains("code"));
        let code = handshake(&mut host, &mut joiner, now);
        assert_eq!(code.len(), 6);
        let bad = host.confirm(wrong(&code).as_str(), now);
        assert_eq!(bad.error, Some(PairingError::BadCode));
        assert!(bad.outbound.is_empty());
        let grant = host.confirm(&code, now);
        assert!(grant.error.is_none());
        let frame = String::from_utf8_lossy(&grant.outbound);
        assert_clean(&frame);
        assert!(!frame.contains(&code));
        let opened = joiner.push(&grant.outbound, now);
        assert!(opened.error.is_none());
        let payload = joiner.take_payload().unwrap();
        assert_eq!(payload.repo_owner, "fake-owner");
        assert_eq!(&payload.sync_token[..], TOKEN);
        assert_eq!(payload.secret_key.0, [0xAB; 16]);
        assert!(!format!("{payload:?}").contains(TOKEN));
        assert_eq!(joiner.phase(), "done");
        assert!(joiner.code().is_err());
        let again = host.confirm(&code, now);
        assert!(again.error.is_none());
        assert_eq!(again.outbound, grant.outbound);
    }

    #[test]
    fn three_wrong_codes_end_the_session() {
        let now = 1_700_000_100;
        let (mut host, mut joiner, qr) = pair_at(now);
        let code = handshake(&mut host, &mut joiner, now);
        let wrong = wrong(&code);
        assert_eq!(host.confirm(&wrong, now).error, Some(PairingError::BadCode));
        assert_eq!(host.confirm(&wrong, now).error, Some(PairingError::BadCode));
        let burned = host.confirm(&wrong, now);
        assert_eq!(burned.error, Some(PairingError::Exhausted));
        assert!(!burned.outbound.is_empty());
        assert!(!host.accepts_peer(now));
        let seen = joiner.push(&burned.outbound, now);
        assert_eq!(seen.error, Some(PairingError::Failed));
        let mut consumed = ConsumedPairings::default();
        consumed
            .mark_released(host.session_id(), host.expires_at(), now)
            .unwrap();
        let again = JoinerSession::from_qr(&qr, now, &mut consumed);
        assert_eq!(again.unwrap_err(), PairingError::Replay);
    }

    #[test]
    fn expiry_skew_and_version() {
        let now = 1_700_000_100;
        let (host, _joiner, qr) = pair_at(now);
        let exp = host.expires_at();
        assert_eq!(exp, now + PAIRING_TTL_SECS);
        assert!(check_window(exp - 1, exp).is_ok());
        assert_eq!(check_window(exp, exp), Err(PairingError::Expired));
        let mut consumed = ConsumedPairings::default();
        let early = now + PAIRING_TTL_SECS + PAIRING_CLOCK_SKEW_SECS;
        assert!(check_window(now, early).is_ok());
        assert_eq!(
            check_window(now, early + 1),
            Err(PairingError::BadMessage)
        );
        let mut old = qr.clone();
        old = old.replacen("gitpass-pair:2:", "gitpass-pair:1:", 1);
        assert_eq!(
            JoinerSession::from_qr(&old, now, &mut consumed).unwrap_err(),
            PairingError::BadVersion
        );
        assert_eq!(
            JoinerSession::from_qr("not-a-frame", now, &mut consumed).unwrap_err(),
            PairingError::BadMessage
        );
        drop(host);
    }

    #[test]
    fn replay_record_prunes_and_blocks_the_sid() {
        let now = 1_700_000_100;
        let (host, _joiner, qr) = pair_at(now);
        let mut consumed = ConsumedPairings::default();
        consumed
            .mark_released(host.session_id(), host.expires_at(), now)
            .unwrap();
        assert_eq!(
            consumed.mark_accepted(host.session_id(), host.expires_at(), now),
            Err(PairingError::Replay)
        );
        assert_eq!(
            JoinerSession::from_qr(&qr, now, &mut consumed).unwrap_err(),
            PairingError::Replay
        );
        let dir = tempfile::tempdir().unwrap();
        save_consumed(dir.path(), &consumed).unwrap();
        let mut loaded = load_consumed(dir.path()).unwrap();
        assert!(loaded.is_used(host.session_id(), now));
        let mut stale = ConsumedPairings::default();
        stale
            .mark_released("old-session", now - RETAIN_AFTER_EXP_SECS - 5, now - RETAIN_AFTER_EXP_SECS - 5)
            .unwrap();
        // mark_released prunes using `now`, so plant an already-expired row directly.
        stale.released.insert("older".into(), now - RETAIN_AFTER_EXP_SECS - 1);
        stale.prune(now);
        assert!(!stale.released.contains_key("older"));
    }

    #[test]
    fn tamper_flip_ct_nonce_and_wrong_key() {
        let now = 1_700_000_100;
        let (mut host, mut joiner, _) = pair_at(now);
        let code = handshake(&mut host, &mut joiner, now);
        let grant = host.confirm(&code, now).outbound;
        let mut flipped = grant.clone();
        flip_ct(&mut flipped);
        let err = joiner.push(&flipped, now);
        assert_eq!(err.error, Some(PairingError::Tampered));

        let (mut host, mut joiner, _) = pair_at(now + 1);
        let code = handshake(&mut host, &mut joiner, now + 1);
        let grant = host.confirm(&code, now + 1).outbound;
        let mut nonce_flipped = grant.clone();
        flip_nonce(&mut nonce_flipped);
        assert_eq!(
            joiner.push(&nonce_flipped, now + 1).error,
            Some(PairingError::Tampered)
        );

        let (mut host, mut joiner, _) = pair_at(now + 2);
        let code = handshake(&mut host, &mut joiner, now + 2);
        let grant = host.confirm(&code, now + 2).outbound;
        let mut wrong_key = grant.clone();
        reseal_with_other_key(&mut wrong_key);
        assert_eq!(
            joiner.push(&wrong_key, now + 2).error,
            Some(PairingError::Tampered)
        );
    }

    #[test]
    fn commitment_rejects_a_different_key_and_code_tracks_both_keys() {
        let now = 1_700_000_100;
        let (mut host, mut joiner, _) = pair_at(now);
        let commit = take_ok(joiner.push(&[], now)).unwrap();
        let reveal = take_ok(host.push(&commit, now)).unwrap();
        let mut open = take_ok(joiner.push(&reveal, now)).unwrap();
        // OPEN body starts at byte 5. Flip the first public-key byte.
        open[5] ^= 0x01;
        let rejected = host.push(&open, now);
        assert_eq!(rejected.error, Some(PairingError::BadMessage));
        assert!(!rejected.outbound.is_empty());

        let mut phone = [0x22u8; 32];
        let host_pub = [0x11u8; 32];
        let a = sample_code(&phone, &host_pub);
        phone[0] ^= 0x01;
        let b = sample_code(&phone, &host_pub);
        assert_ne!(a, b);
        let mut host_pub2 = host_pub;
        host_pub2[0] ^= 0x01;
        let c = sample_code(&[0x22u8; 32], &host_pub2);
        assert_ne!(a, c);
        let committed = sha256_2(&[0x22u8; 32], &[0x33u8; 32]);
        let mut other = [0x22u8; 32];
        other[1] ^= 0x01;
        assert_ne!(committed, sha256_2(&other, &[0x33u8; 32]));
    }

    #[test]
    fn wrong_session_oversized_and_truncated_frames() {
        let now = 1_700_000_100;
        let (mut host, joiner, _) = pair_at(now);
        let (host2, mut joiner2, _) = pair_at(now);
        drop(host2);
        let commit = take_ok(joiner2.push(&[], now)).unwrap();
        let wrong = host.push(&commit, now);
        assert_eq!(wrong.error, Some(PairingError::WrongSession));
        drop(joiner);

        let (mut host, _, _) = pair_at(now);
        let huge = 100_000u32.to_be_bytes();
        let big = host.push(&huge, now);
        assert_eq!(big.error, Some(PairingError::Oversized));

        let (mut host, _, _) = pair_at(now);
        let partial = 40u32.to_be_bytes();
        let wait = host.push(&partial, now);
        assert!(wait.error.is_none());
        assert!(wait.outbound.is_empty());
        assert_eq!(host.finish(), Err(PairingError::Truncated));
    }

    #[test]
    fn payload_rejects_a_password_field_and_a_classic_token() {
        assert_eq!(
            PairingRequest::new(
                TV_ACCOUNT,
                "fake-owner",
                "fake-vault",
                "main",
                SecretKey([0xAB; 16]),
                "ghp_FAKE0123456789",
            )
            .unwrap_err(),
            PairingError::BadField
        );
        let mut extra = serde_json::json!({
            "v": PAIRING_VERSION,
            "account_id": TV_ACCOUNT,
            "repo_owner": "fake-owner",
            "repo_name": "fake-vault",
            "branch": "main",
            "secret_key": SecretKey([0xAB; 16]).encode(),
            "sync_token": TOKEN,
            "token_kind": TOKEN_KIND_FINE_GRAINED_PAT,
            "password": PASSWORD,
        });
        let err = decode_payload(&serde_json::to_vec(&extra).unwrap()).unwrap_err();
        assert_eq!(err, PairingError::BadField);
        assert!(!err.to_string().contains(PASSWORD));
        extra.as_object_mut().unwrap().remove("password");
        extra["v"] = serde_json::json!(1);
        assert_eq!(
            decode_payload(&serde_json::to_vec(&extra).unwrap()).unwrap_err(),
            PairingError::BadVersion
        );
        let vector = test_vector_json().unwrap();
        assert!(vector.contains("gitpass-pair:2:qr:"));
        assert!(!vector.contains(PASSWORD));
        assert!(!vector.contains("master_password"));
        let opened = test_vector_open().unwrap();
        assert_eq!(&opened.sync_token[..], TV_TOKEN);
        assert_eq!(opened.repo_owner, "fake-owner");
    }

    #[test]
    fn open_before_commit_is_rejected() {
        let now = 1_700_000_100;
        let (mut host, _, _) = pair_at(now);
        let early = encode(T_OPEN, &[0u8; OPEN_LEN]).unwrap();
        assert_eq!(host.push(&early, now).error, Some(PairingError::BadMessage));
    }

    fn sample_code(phone: &[u8; 32], host_pub: &[u8; 32]) -> String {
        let ikm = [0x07u8; 32];
        let t = Transcript {
            sid: [0x55; 16],
            exp: TV_NOW + PAIRING_TTL_SECS,
            port: TV_PORT,
            addrs_json: "[\"127.0.0.1\"]".into(),
            host_pub: *host_pub,
            commitment: sha256_2(phone, &[0x33; 32]),
            nonce_d: [0x44; 32],
            phone_pub: *phone,
            nonce_p: [0x33; 32],
        };
        let salt = transcript_hash(&t.bytes());
        let mut sas = hkdf32(&ikm, Some(&salt), b"gitpass/pair/v2/sas");
        let code = code_from_key(&sas);
        sas.0.zeroize();
        code
    }

    fn grant_body(frame: &mut [u8]) -> (usize, Envelope) {
        let n = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(frame[4], T_GRANT);
        let env: Envelope = serde_json::from_slice(&frame[5..4 + n]).unwrap();
        (n, env)
    }

    fn write_grant(frame: &mut Vec<u8>, env: &Envelope) {
        let body = serde_json::to_vec(env).unwrap();
        let encoded = encode(T_GRANT, &body).unwrap();
        frame.clear();
        frame.extend_from_slice(&encoded);
    }

    fn flip_ct(frame: &mut Vec<u8>) {
        let (_n, mut env) = grant_body(frame);
        let mut ct = B64.decode(&env.ct).unwrap();
        let i = ct.len() / 2;
        ct[i] ^= 0x01;
        env.ct = B64.encode(ct);
        write_grant(frame, &env);
    }

    fn flip_nonce(frame: &mut Vec<u8>) {
        let (_n, mut env) = grant_body(frame);
        let mut nonce = B64.decode(&env.nonce).unwrap();
        nonce[0] ^= 0x01;
        env.nonce = B64.encode(nonce);
        write_grant(frame, &env);
    }

    fn reseal_with_other_key(frame: &mut Vec<u8>) {
        let (_n, env) = grant_body(frame);
        let other = Key32([0x5A; 32]);
        let nonce = B64.decode(&env.nonce).unwrap();
        let mut n = [0u8; 24];
        n.copy_from_slice(&nonce);
        let sealed = seal_with_nonce(&other, b"not-the-payload", b"gitpass/pair/v2", n).unwrap();
        write_grant(frame, &sealed);
    }
}
