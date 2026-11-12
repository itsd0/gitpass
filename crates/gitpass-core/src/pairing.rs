//! QR device pairing (phase 4 of cross-platform sync).
//!
//! The new device needs a sync token and the account Secret Key. The master password is never
//! part of this protocol: the person types it on the new device. There is no gitpass server and
//! the new device cannot read the git remote until it has the token, so the ciphertext moves
//! only inside QR frames (any other local copy of the same text is the same message).
//!
//! Direction: the joiner (new device) generates an ephemeral X25519 key and reveals only the
//! public key (`offer`). The host (desktop) generates its own ephemeral key after that and shows
//! `announce` (host public key + session id, no secrets). Only the phone displays the 6-digit
//! code derived from the transcript. The person types that code on the desktop. The desktop API
//! does not return its own code: a screen that confirms a code it just computed is not a check.
//! `grant` (the ciphertext) is released only after the typed code matches, and the joiner decrypts
//! only after the typed code matches. A photographed grant is unreadable without the joiner's
//! ephemeral private key, which never leaves the process and is wiped on success.
//!
//! UI for #458: show `announce` as the desktop QR and the joiner's code on the phone only.
//! Three wrong codes end the session. The sync token must be a fine-grained GitHub PAT.
//!
//! Frames, all ASCII, one line:
//! - `gitpass-pair:1:offer:<sid>.<joiner_pub>.<exp>`
//! - `gitpass-pair:1:announce:<sid>.<joiner_pub>.<host_pub>.<exp>`
//! - `gitpass-pair:1:grant:<sid>.<joiner_pub>.<host_pub>.<exp>.<envelope>`
//!
//! `<exp>` is the absolute expiry (Unix seconds), two minutes after the offer is created.
//! Session ids are single-use: callers persist [`ConsumedPairings`] (public ids and expiries
//! only) and reject a repeat. Device registration is not done here. After the person types the
//! master password, a normal unlock with device registration (`Session::unlock_device`) adds the
//! new device to `devices.enc`.

use crate::crypto::{hkdf32, open, random_bytes, seal_with_nonce, Key32, SecretKey};
use crate::device;
use crate::fsutil;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64U, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

/// Protocol version carried in every frame and inside the sealed payload.
pub const PAIRING_VERSION: u32 = 1;
/// How long an offer stays valid.
pub const PAIRING_TTL_SECS: i64 = 120;
/// Host clock may be this far behind the joiner when checking `exp`.
pub const PAIRING_CLOCK_SKEW_SECS: i64 = 60;
/// Local replay record. Git-ignored. Contains no secrets.
pub const CONSUMED_FILE: &str = ".gitpass-pairing-consumed.json";

const MAX_MESSAGE_BYTES: usize = 4096;
const RETAIN_AFTER_EXP_SECS: i64 = 24 * 60 * 60;
const MAX_CONSUMED_ENTRIES: usize = 1024;
const MAX_CONSUMED_BYTES: usize = 256 * 1024;
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
/// Only value accepted for `token_kind`. A later token type needs a new payload version.
pub const TOKEN_KIND_FINE_GRAINED_PAT: &str = "fine_grained_pat";
/// Wrong codes before the session is wiped and must be marked used.
const MAX_WRONG_CODES: u8 = 3;

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
        }
    }
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl std::error::Error for PairingError {}

/// Which frame a scanned string is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingKind {
    Offer,
    Announce,
    Grant,
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
    pub fn is_released(&mut self, sid: &str, now: i64) -> bool {
        self.prune(now);
        self.released.contains_key(sid)
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

/// New device. The private key stays here until [`JoinerSession::consume`].
pub struct JoinerSession {
    secret: Option<StaticSecret>,
    public: [u8; 32],
    session_id: [u8; 16],
    sid_b64: String,
    exp: i64,
    announced_host: Option<[u8; 32]>,
    wrong_codes: u8,
}

impl std::fmt::Debug for JoinerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinerSession")
            .field("session_id", &self.sid_b64)
            .field("exp", &self.exp)
            .finish_non_exhaustive()
    }
}

impl JoinerSession {
    pub fn begin(now: i64) -> Result<Self, PairingError> {
        let exp = expiry_from(now)?;
        Self::from_parts(random_bytes(), random_bytes(), exp)
    }
    fn from_parts(
        secret_bytes: [u8; 32],
        session_id: [u8; 16],
        exp: i64,
    ) -> Result<Self, PairingError> {
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret).to_bytes();
        if public == [0u8; 32] {
            return Err(PairingError::LowOrder);
        }
        Ok(Self {
            secret: Some(secret),
            public,
            sid_b64: b64(&session_id),
            session_id,
            exp,
            announced_host: None,
            wrong_codes: 0,
        })
    }
    pub fn offer(&self) -> String {
        format!(
            "gitpass-pair:{PAIRING_VERSION}:offer:{}.{}.{}",
            self.sid_b64,
            b64(&self.public),
            self.exp
        )
    }
    pub fn session_id(&self) -> &str {
        &self.sid_b64
    }
    pub fn expires_at(&self) -> i64 {
        self.exp
    }
    /// Read the host's announce frame and return the 6-digit code the phone shows.
    /// Does not decrypt anything. The desktop must not display this value.
    pub fn read_announce(&mut self, announce: &str, now: i64) -> Result<String, PairingError> {
        if self.secret.is_none() {
            return Err(PairingError::Replay);
        }
        if now >= self.exp {
            self.consume();
            return Err(PairingError::Expired);
        }
        let a = parse_announce(announce)?;
        if a.sid != self.session_id || a.joiner_pub != self.public || a.exp != self.exp {
            return Err(PairingError::BadMessage);
        }
        let secret = self.secret.as_ref().ok_or(PairingError::Replay)?;
        let mut shared = dh(secret, &a.host_pub)?;
        let code = sas_code(
            &shared,
            &self.public,
            &a.host_pub,
            &self.session_id,
            self.exp,
        );
        shared.zeroize();
        self.announced_host = Some(a.host_pub);
        Ok(code)
    }
    /// Decrypt `grant` when `code` matches the announce. Does not mark the session used; call
    /// [`ConsumedPairings::mark_released`] and [`JoinerSession::consume`] after that record is saved.
    /// The third wrong code wipes the ephemeral key and returns [`PairingError::Exhausted`].
    pub fn open(
        &mut self,
        grant: &str,
        code: &str,
        now: i64,
        consumed: &mut ConsumedPairings,
    ) -> Result<PairingPayload, PairingError> {
        if self.secret.is_none() {
            return Err(PairingError::Replay);
        }
        if now >= self.exp {
            self.consume();
            return Err(PairingError::Expired);
        }
        let host_pub = self.announced_host.ok_or(PairingError::BadMessage)?;
        let g = parse_grant(grant)?;
        if g.sid != self.session_id
            || g.joiner_pub != self.public
            || g.host_pub != host_pub
            || g.exp != self.exp
        {
            return Err(PairingError::BadMessage);
        }
        if consumed.is_released(&self.sid_b64, now) {
            return Err(PairingError::Replay);
        }
        let entered = match normalize_code(code) {
            Ok(c) => c.to_string(),
            Err(_) => return Err(self.note_wrong_code()),
        };
        let opened = {
            let secret = self.secret.as_ref().ok_or(PairingError::Replay)?;
            let mut shared = dh(secret, &g.host_pub)?;
            let expect = sas_code(
                &shared,
                &self.public,
                &g.host_pub,
                &self.session_id,
                self.exp,
            );
            if !ct_eq(expect.as_bytes(), entered.as_bytes()) {
                shared.zeroize();
                None
            } else {
                let aead = derive_aead(
                    &shared,
                    &self.public,
                    &g.host_pub,
                    &self.session_id,
                    self.exp,
                );
                shared.zeroize();
                let aad = transcript_aad(&self.session_id, &self.public, &g.host_pub, self.exp);
                Some(open_grant_blob(&aead, &g.envelope_b64, &aad)?)
            }
        };
        let Some(pt) = opened else {
            return Err(self.note_wrong_code());
        };
        decode_payload(&pt)
    }
    fn note_wrong_code(&mut self) -> PairingError {
        self.wrong_codes = self.wrong_codes.saturating_add(1);
        if self.wrong_codes >= MAX_WRONG_CODES {
            self.consume();
            PairingError::Exhausted
        } else {
            PairingError::BadCode
        }
    }
    pub fn consume(&mut self) {
        self.secret = None;
        self.announced_host = None;
    }
}

/// Desktop side. Holds the payload in memory until [`HostSession::wipe`].
pub struct HostSession {
    aead: Option<Key32>,
    code: String,
    joiner_pub: [u8; 32],
    host_pub: [u8; 32],
    session_id: [u8; 16],
    sid_b64: String,
    exp: i64,
    request: Option<PairingRequest>,
    cached_grant: Option<String>,
    wrong_codes: u8,
}

impl std::fmt::Debug for HostSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostSession")
            .field("session_id", &self.sid_b64)
            .field("exp", &self.exp)
            .field("has_payload", &self.request.is_some())
            .finish_non_exhaustive()
    }
}

impl HostSession {
    pub fn accept(
        offer: &str,
        request: PairingRequest,
        now: i64,
        consumed: &mut ConsumedPairings,
    ) -> Result<Self, PairingError> {
        Self::accept_with(offer, request, now, consumed, None)
    }
    fn accept_with(
        offer: &str,
        request: PairingRequest,
        now: i64,
        consumed: &mut ConsumedPairings,
        host_secret_bytes: Option<[u8; 32]>,
    ) -> Result<Self, PairingError> {
        if now <= 0 {
            return Err(PairingError::BadMessage);
        }
        let offer = parse_offer(offer)?;
        if consumed.is_used(&offer.sid_b64, now) {
            return Err(PairingError::Replay);
        }
        check_window(now, offer.exp)?;
        let host_secret = StaticSecret::from(host_secret_bytes.unwrap_or_else(random_bytes));
        let host_pub = PublicKey::from(&host_secret).to_bytes();
        let mut shared = dh(&host_secret, &offer.joiner_pub)?;
        drop(host_secret);
        let aead = derive_aead(&shared, &offer.joiner_pub, &host_pub, &offer.sid, offer.exp);
        let code = sas_code(&shared, &offer.joiner_pub, &host_pub, &offer.sid, offer.exp);
        shared.zeroize();
        consumed.mark_accepted(&offer.sid_b64, offer.exp, now)?;
        Ok(Self {
            aead: Some(aead),
            code,
            joiner_pub: offer.joiner_pub,
            host_pub,
            session_id: offer.sid,
            sid_b64: offer.sid_b64,
            exp: offer.exp,
            request: Some(request),
            cached_grant: None,
            wrong_codes: 0,
        })
    }
    /// The code the phone will show after it reads [`HostSession::announce`].
    /// Not part of the desktop API: returning it would let a screen confirm itself.
    fn code(&self) -> &str {
        &self.code
    }
    pub fn session_id(&self) -> &str {
        &self.sid_b64
    }
    pub fn expires_at(&self) -> i64 {
        self.exp
    }
    pub fn announce(&self) -> String {
        format!(
            "gitpass-pair:{PAIRING_VERSION}:announce:{}.{}.{}.{}",
            self.sid_b64,
            b64(&self.joiner_pub),
            b64(&self.host_pub),
            self.exp
        )
    }
    /// Build the grant when `code` matches. The same ciphertext is returned on a retry so a failed
    /// save of the replay record does not mint a second grant. Call [`HostSession::wipe`] after the
    /// record is saved.
    pub fn grant_if_code_matches(&mut self, code: &str, now: i64) -> Result<String, PairingError> {
        self.grant_with_nonce(code, now, None)
    }
    fn grant_with_nonce(
        &mut self,
        code: &str,
        now: i64,
        nonce: Option<[u8; 24]>,
    ) -> Result<String, PairingError> {
        if self.aead.is_none() || self.request.is_none() {
            return Err(PairingError::Replay);
        }
        if now >= self.exp {
            self.wipe();
            return Err(PairingError::Expired);
        }
        let entered = match normalize_code(code) {
            Ok(c) => c,
            Err(_) => return self.reject_code(),
        };
        if !ct_eq(self.code.as_bytes(), entered.as_bytes()) {
            return self.reject_code();
        }
        if self.cached_grant.is_none() {
            let nonce = nonce.unwrap_or_else(random_bytes);
            let req = self.request.as_ref().ok_or(PairingError::Replay)?;
            let pt = payload_bytes(req)?;
            let aead = self.aead.as_ref().ok_or(PairingError::Replay)?;
            let aad = transcript_aad(&self.session_id, &self.joiner_pub, &self.host_pub, self.exp);
            let blob = seal_payload(aead, &pt, &aad, nonce)?;
            self.cached_grant = Some(format!(
                "gitpass-pair:{PAIRING_VERSION}:grant:{}.{}.{}.{}.{}",
                self.sid_b64,
                b64(&self.joiner_pub),
                b64(&self.host_pub),
                self.exp,
                blob
            ));
        }
        self.cached_grant.clone().ok_or(PairingError::Replay)
    }
    fn reject_code(&mut self) -> Result<String, PairingError> {
        self.wrong_codes = self.wrong_codes.saturating_add(1);
        if self.wrong_codes >= MAX_WRONG_CODES {
            self.wipe();
            Err(PairingError::Exhausted)
        } else {
            Err(PairingError::BadCode)
        }
    }
    pub fn wipe(&mut self) {
        self.aead = None;
        self.request = None;
        if let Some(mut g) = self.cached_grant.take() {
            g.zeroize();
        }
    }
    #[cfg(test)]
    fn seal_raw(&self, plaintext: &[u8], nonce: [u8; 24]) -> Result<String, PairingError> {
        let aead = self.aead.as_ref().ok_or(PairingError::Replay)?;
        let aad = transcript_aad(&self.session_id, &self.joiner_pub, &self.host_pub, self.exp);
        let blob = seal_payload(aead, plaintext, &aad, nonce)?;
        Ok(format!(
            "gitpass-pair:{PAIRING_VERSION}:grant:{}.{}.{}.{}.{}",
            self.sid_b64,
            b64(&self.joiner_pub),
            b64(&self.host_pub),
            self.exp,
            blob
        ))
    }
}

/// `offer` / `announce` / `grant`, or an error if it is not a pairing frame.
pub fn message_kind(message: &str) -> Result<PairingKind, PairingError> {
    Ok(parse_framed(message)?.0)
}

/// Synthetic vector (fixed keys, fake token). For Kotlin/Swift tests via the bridge.
pub fn test_vector_json() -> Result<String, PairingError> {
    let v = vector_parts()?;
    let body = TestVectorOut {
        v: PAIRING_VERSION,
        now: TV_NOW,
        offer: v.offer,
        announce: v.announce,
        code: v.code,
        grant: v.grant,
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

#[derive(Serialize)]
struct TestVectorOut {
    v: u32,
    now: i64,
    offer: String,
    announce: String,
    /// The code the phone would show. The live desktop API does not return a code.
    code: String,
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
    offer: String,
    announce: String,
    code: String,
    grant: String,
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
    let exp = TV_NOW + PAIRING_TTL_SECS;
    let mut joiner = JoinerSession::from_parts([0x11; 32], [0x33; 16], exp)?;
    let offer = joiner.offer();
    let mut consumed = ConsumedPairings::default();
    let mut host = HostSession::accept_with(
        &offer,
        tv_request()?,
        TV_NOW,
        &mut consumed,
        Some([0x22; 32]),
    )?;
    let announce = host.announce();
    let code = joiner.read_announce(&announce, TV_NOW)?;
    if code != host.code() {
        return Err(PairingError::BadMessage);
    }
    let grant = host.grant_with_nonce(&code, TV_NOW, Some([0x44; 24]))?;
    let payload = joiner.open(&grant, &code, TV_NOW, &mut ConsumedPairings::default())?;
    Ok(VectorParts {
        offer,
        announce,
        code,
        grant,
        payload,
    })
}

fn expiry_from(now: i64) -> Result<i64, PairingError> {
    if now <= 0 {
        return Err(PairingError::BadMessage);
    }
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

fn salt(joiner: &[u8; 32], host: &[u8; 32], sid: &[u8; 16], exp: i64) -> [u8; 88] {
    let mut s = [0u8; 88];
    s[..32].copy_from_slice(joiner);
    s[32..64].copy_from_slice(host);
    s[64..80].copy_from_slice(sid);
    s[80..88].copy_from_slice(&exp.to_be_bytes());
    s
}

fn derive_aead(
    shared: &[u8; 32],
    joiner: &[u8; 32],
    host: &[u8; 32],
    sid: &[u8; 16],
    exp: i64,
) -> Key32 {
    hkdf32(
        shared,
        Some(&salt(joiner, host, sid, exp)),
        b"gitpass/pair/v1/aead",
    )
}

/// Six digits from HKDF. `u32 % 1_000_000` has a tiny bias; this is a comparison string, not a key.
fn sas_code(
    shared: &[u8; 32],
    joiner: &[u8; 32],
    host: &[u8; 32],
    sid: &[u8; 16],
    exp: i64,
) -> String {
    let k = hkdf32(
        shared,
        Some(&salt(joiner, host, sid, exp)),
        b"gitpass/pair/v1/sas",
    );
    let mut b = [0u8; 4];
    b.copy_from_slice(&k.0[..4]);
    let n = u32::from_be_bytes(b) % 1_000_000;
    format!("{n:06}")
}

fn transcript_aad(sid: &[u8; 16], joiner: &[u8; 32], host: &[u8; 32], exp: i64) -> Vec<u8> {
    let mut a = Vec::with_capacity(16 + 16 + 32 + 32 + 8);
    a.extend_from_slice(b"gitpass/pair/v1");
    a.extend_from_slice(sid);
    a.extend_from_slice(joiner);
    a.extend_from_slice(host);
    a.extend_from_slice(&exp.to_be_bytes());
    a
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

fn seal_payload(
    aead: &Key32,
    plaintext: &[u8],
    aad: &[u8],
    nonce: [u8; 24],
) -> Result<String, PairingError> {
    let env = seal_with_nonce(aead, plaintext, aad, nonce).map_err(|_| PairingError::BadMessage)?;
    let bytes = serde_json::to_vec(&env).map_err(|_| PairingError::BadMessage)?;
    Ok(b64(&bytes))
}

fn open_grant_blob(
    aead: &Key32,
    envelope_b64: &str,
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, PairingError> {
    let raw = B64U
        .decode(envelope_b64)
        .map_err(|_| PairingError::Tampered)?;
    let env = serde_json::from_slice(&raw).map_err(|_| PairingError::Tampered)?;
    open(aead, &env, aad)
        .map(Zeroizing::new)
        .map_err(|_| PairingError::Tampered)
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
    let value: serde_json::Value =
        serde_json::from_slice(pt).map_err(|_| PairingError::Tampered)?;
    let obj = value.as_object().ok_or(PairingError::Tampered)?;
    match obj.get("v") {
        Some(serde_json::Value::Number(n)) if n.as_u64() == Some(u64::from(PAIRING_VERSION)) => {}
        Some(serde_json::Value::Number(_)) => return Err(PairingError::BadVersion),
        _ => return Err(PairingError::BadField),
    }
    if obj.len() != PAYLOAD_KEYS.len() || PAYLOAD_KEYS.iter().any(|k| !obj.contains_key(*k)) {
        return Err(PairingError::BadField);
    }
    let mut plain: PayloadPlain =
        serde_json::from_value(value).map_err(|_| PairingError::BadField)?;
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

struct OfferParts {
    sid: [u8; 16],
    sid_b64: String,
    joiner_pub: [u8; 32],
    exp: i64,
}
struct AnnounceParts {
    sid: [u8; 16],
    joiner_pub: [u8; 32],
    host_pub: [u8; 32],
    exp: i64,
}
struct GrantParts {
    sid: [u8; 16],
    joiner_pub: [u8; 32],
    host_pub: [u8; 32],
    exp: i64,
    envelope_b64: String,
}

fn parse_framed(message: &str) -> Result<(PairingKind, Vec<&str>), PairingError> {
    let s = message.trim();
    if s.len() > MAX_MESSAGE_BYTES || !s.is_ascii() || s.chars().any(char::is_whitespace) {
        return Err(PairingError::BadMessage);
    }
    let rest = s
        .strip_prefix("gitpass-pair:")
        .ok_or(PairingError::BadMessage)?;
    let (ver, rest) = rest.split_once(':').ok_or(PairingError::BadMessage)?;
    if !ver.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PairingError::BadMessage);
    }
    if ver.parse::<u32>().ok() != Some(PAIRING_VERSION) {
        return Err(PairingError::BadVersion);
    }
    let (kind, body) = rest.split_once(':').ok_or(PairingError::BadMessage)?;
    let kind = match kind {
        "offer" => PairingKind::Offer,
        "announce" => PairingKind::Announce,
        "grant" => PairingKind::Grant,
        _ => return Err(PairingError::BadMessage),
    };
    if body.is_empty() {
        return Err(PairingError::BadMessage);
    }
    let parts: Vec<&str> = body.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(PairingError::BadMessage);
    }
    Ok((kind, parts))
}

fn parse_exp(s: &str) -> Result<i64, PairingError> {
    if s.is_empty() || s.len() > 18 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PairingError::BadMessage);
    }
    let n: i64 = s.parse().map_err(|_| PairingError::BadMessage)?;
    if n <= 0 {
        return Err(PairingError::BadMessage);
    }
    Ok(n)
}

fn parse_offer(message: &str) -> Result<OfferParts, PairingError> {
    let (kind, parts) = parse_framed(message)?;
    if kind != PairingKind::Offer || parts.len() != 3 {
        return Err(PairingError::BadMessage);
    }
    let sid = decode_exact::<16>(parts[0])?;
    Ok(OfferParts {
        sid,
        sid_b64: parts[0].to_string(),
        joiner_pub: decode_exact(parts[1])?,
        exp: parse_exp(parts[2])?,
    })
}

fn parse_announce(message: &str) -> Result<AnnounceParts, PairingError> {
    let (kind, parts) = parse_framed(message)?;
    if kind != PairingKind::Announce || parts.len() != 4 {
        return Err(PairingError::BadMessage);
    }
    Ok(AnnounceParts {
        sid: decode_exact(parts[0])?,
        joiner_pub: decode_exact(parts[1])?,
        host_pub: decode_exact(parts[2])?,
        exp: parse_exp(parts[3])?,
    })
}

fn parse_grant(message: &str) -> Result<GrantParts, PairingError> {
    let (kind, parts) = parse_framed(message)?;
    if kind != PairingKind::Grant || parts.len() != 5 {
        return Err(PairingError::BadMessage);
    }
    Ok(GrantParts {
        sid: decode_exact(parts[0])?,
        joiner_pub: decode_exact(parts[1])?,
        host_pub: decode_exact(parts[2])?,
        exp: parse_exp(parts[3])?,
        envelope_b64: parts[4].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, Session, UnlockOpts};
    use std::fs;

    const PASSWORD: &str = "FAKE-master-password-must-not-appear";
    const TOKEN: &str = "github_pat_FAKE_from_desktop";

    fn fast() -> crate::crypto::KdfParams {
        crate::crypto::KdfParams {
            alg: "argon2id".into(),
            m_kib: 8192,
            t: 1,
            p: 1,
        }
    }
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
    }

    #[test]
    fn happy_path_codes_match_and_secrets_round_trip() {
        let now = 1_700_000_000;
        let mut joiner = JoinerSession::begin(now).unwrap();
        let offer = joiner.offer();
        assert_eq!(message_kind(&offer).unwrap(), PairingKind::Offer);
        let mut consumed = ConsumedPairings::default();
        let mut host = HostSession::accept(&offer, req(TOKEN), now, &mut consumed).unwrap();
        let announce = host.announce();
        assert_eq!(message_kind(&announce).unwrap(), PairingKind::Announce);
        let code = joiner.read_announce(&announce, now).unwrap();
        assert_eq!(code, host.code());
        assert_eq!(code.len(), 6);
        let bad = host.grant_if_code_matches(&wrong(&code), now).unwrap_err();
        assert_eq!(bad, PairingError::BadCode);
        assert_clean(&bad.to_string());
        let grant = host.grant_if_code_matches(&code, now).unwrap();
        assert_eq!(message_kind(&grant).unwrap(), PairingKind::Grant);
        assert_eq!(host.grant_if_code_matches(&code, now).unwrap(), grant);
        assert_clean(&offer);
        assert_clean(&announce);
        assert_clean(&grant);
        assert_eq!(
            joiner
                .open(&grant, &wrong(&code), now, &mut ConsumedPairings::default())
                .unwrap_err(),
            PairingError::BadCode
        );
        let payload = joiner
            .open(&grant, &code, now, &mut ConsumedPairings::default())
            .unwrap();
        assert_eq!(payload.account_id, TV_ACCOUNT);
        assert_eq!(payload.repo_owner, "fake-owner");
        assert_eq!(payload.repo_name, "fake-vault");
        assert_eq!(payload.branch, "main");
        assert_eq!(&payload.sync_token[..], TOKEN);
        assert_eq!(payload.secret_key.encode(), SecretKey([0xAB; 16]).encode());
        let dbg = format!("{payload:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains(TOKEN));
        assert!(!dbg.contains(&payload.secret_key.encode()));
        assert!(!dbg.contains(PASSWORD));
        host.wipe();
        assert_eq!(
            host.grant_if_code_matches(&code, now).unwrap_err(),
            PairingError::Replay
        );
    }

    #[test]
    fn expired_replay_version_low_order_and_field_rejection() {
        let now = 1_700_000_000;
        let joiner = JoinerSession::begin(now).unwrap();
        let mut consumed = ConsumedPairings::default();
        assert_eq!(
            HostSession::accept(
                &joiner.offer(),
                req(TOKEN),
                now + PAIRING_TTL_SECS,
                &mut consumed
            )
            .unwrap_err(),
            PairingError::Expired
        );
        assert!(consumed.accepted.is_empty());
        let _host = HostSession::accept(&joiner.offer(), req(TOKEN), now, &mut consumed).unwrap();
        assert_eq!(
            HostSession::accept(&joiner.offer(), req(TOKEN), now, &mut consumed).unwrap_err(),
            PairingError::Replay
        );
        let mut late = JoinerSession::begin(now).unwrap();
        let mut host2 = HostSession::accept(
            &late.offer(),
            req(TOKEN),
            now,
            &mut ConsumedPairings::default(),
        )
        .unwrap();
        let announce = host2.announce();
        let code = late.read_announce(&announce, now).unwrap();
        assert_eq!(
            late.read_announce(&announce, now + PAIRING_TTL_SECS)
                .unwrap_err(),
            PairingError::Expired
        );
        assert_eq!(
            host2
                .grant_if_code_matches(&code, now + PAIRING_TTL_SECS)
                .unwrap_err(),
            PairingError::Expired
        );

        let sid = b64(&[0x33; 16]);
        let zpub = b64(&[0u8; 32]);
        let exp = now + PAIRING_TTL_SECS;
        let zero_offer = format!("gitpass-pair:1:offer:{sid}.{zpub}.{exp}");
        assert_eq!(
            HostSession::accept(
                &zero_offer,
                req(TOKEN),
                now,
                &mut ConsumedPairings::default()
            )
            .unwrap_err(),
            PairingError::LowOrder
        );
        let bad_ver = format!("gitpass-pair:9:offer:{sid}.{zpub}.{exp}");
        assert_eq!(
            message_kind(&bad_ver).unwrap_err(),
            PairingError::BadVersion
        );
        assert_eq!(
            HostSession::accept(&bad_ver, req(TOKEN), now, &mut ConsumedPairings::default())
                .unwrap_err(),
            PairingError::BadVersion
        );
        let crafted = format!(
            "gitpass-pair:1:offer:{}.{}.{}",
            b64(&joiner.session_id),
            b64(&joiner.public),
            now + 10_000
        );
        let err = HostSession::accept(&crafted, req(TOKEN), now, &mut ConsumedPairings::default())
            .unwrap_err();
        assert_eq!(err, PairingError::BadMessage);
        assert_clean(&err.to_string());
        assert_eq!(
            PairingRequest::new(
                TV_ACCOUNT,
                "fake-owner",
                "fake-vault",
                "main",
                SecretKey([0xAB; 16]),
                "https://example.invalid/FAKE-token"
            )
            .unwrap_err(),
            PairingError::BadField
        );
        assert_eq!(
            PairingRequest::new(
                TV_ACCOUNT,
                "../etc",
                "fake-vault",
                "main",
                SecretKey([0xAB; 16]),
                TOKEN
            )
            .unwrap_err(),
            PairingError::BadField
        );
        let leaked = PairingError::BadField.to_string();
        assert!(!leaked.contains("https://"));
        assert!(!leaked.contains("../etc"));
    }

    #[test]
    fn tamper_wrong_inner_version_password_field_and_replayed_grant() {
        let now = TV_NOW;
        let mut joiner =
            JoinerSession::from_parts([0x11; 32], [0x55; 16], now + PAIRING_TTL_SECS).unwrap();
        let mut consumed = ConsumedPairings::default();
        let mut host = HostSession::accept_with(
            &joiner.offer(),
            req(TOKEN),
            now,
            &mut consumed,
            Some([0x22; 32]),
        )
        .unwrap();
        assert_eq!(
            HostSession::accept_with(
                &joiner.offer(),
                req(TOKEN),
                now,
                &mut consumed,
                Some([0x22; 32])
            )
            .unwrap_err(),
            PairingError::Replay
        );
        let code = joiner.read_announce(&host.announce(), now).unwrap();
        let grant = host.grant_with_nonce(&code, now, Some([0x44; 24])).unwrap();
        let tampered = flip_ct(&grant);
        assert_eq!(
            joiner
                .open(&tampered, &code, now, &mut ConsumedPairings::default())
                .unwrap_err(),
            PairingError::Tampered
        );
        let payload = joiner
            .open(&grant, &code, now, &mut ConsumedPairings::default())
            .unwrap();
        assert_eq!(&payload.sync_token[..], TOKEN);
        consumed
            .mark_released(joiner.session_id(), joiner.expires_at(), now)
            .unwrap();
        assert_eq!(
            joiner.open(&grant, &code, now, &mut consumed).unwrap_err(),
            PairingError::Replay
        );

        let secret = SecretKey([0xAB; 16]).encode();
        let password_json = serde_json::json!({
            "v": 1, "account_id": TV_ACCOUNT, "repo_owner": "fake-owner", "repo_name": "fake-vault",
            "branch": "main", "secret_key": secret, "sync_token": TOKEN,
            "token_kind": TOKEN_KIND_FINE_GRAINED_PAT, "password": PASSWORD,
        });
        let injected = host
            .seal_raw(&serde_json::to_vec(&password_json).unwrap(), [0x45; 24])
            .unwrap();
        let err = joiner
            .open(&injected, &code, now, &mut ConsumedPairings::default())
            .unwrap_err();
        assert_eq!(err, PairingError::BadField);
        assert_clean(&err.to_string());
        assert!(!injected.contains(PASSWORD));
        let v2 = serde_json::json!({
            "v": 2, "account_id": TV_ACCOUNT, "repo_owner": "fake-owner", "repo_name": "fake-vault",
            "branch": "main", "secret_key": SecretKey([0xAB; 16]).encode(), "sync_token": TOKEN,
            "token_kind": TOKEN_KIND_FINE_GRAINED_PAT,
        });
        let wrong_v = host
            .seal_raw(&serde_json::to_vec(&v2).unwrap(), [0x46; 24])
            .unwrap();
        assert_eq!(
            joiner
                .open(&wrong_v, &code, now, &mut ConsumedPairings::default())
                .unwrap_err(),
            PairingError::BadVersion
        );
    }

    #[test]
    fn consumed_file_round_trip_and_test_vector() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = ConsumedPairings::default();
        c.mark_accepted("abcDEF-_", 1_700_000_100, 1_700_000_000)
            .unwrap();
        c.mark_released("abcDEF-_", 1_700_000_100, 1_700_000_000)
            .unwrap();
        save_consumed(dir.path(), &c).unwrap();
        let loaded = load_consumed(dir.path()).unwrap();
        assert!(loaded.released.contains_key("abcDEF-_"));
        assert!(!loaded.accepted.contains_key("abcDEF-_"));
        let ignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(ignore.lines().any(|l| l.trim() == CONSUMED_FILE));
        fs::write(
            dir.path().join(CONSUMED_FILE),
            b"{\"v\":1,\"accepted\":{},\"released\":{},\"password\":\"x\"}",
        )
        .unwrap();
        assert_eq!(load_consumed(dir.path()).unwrap_err(), PairingError::State);
        let raw = fs::read_to_string(dir.path().join(CONSUMED_FILE)).unwrap_or_default();
        assert!(!raw.contains(TOKEN));

        let payload = test_vector_open().unwrap();
        assert_eq!(payload.repo_owner, "fake-owner");
        assert_eq!(&payload.sync_token[..], TV_TOKEN);
        assert!(payload.secret_key.encode().starts_with("A3-"));
        let json = test_vector_json().unwrap();
        assert!(json.contains("gitpass-pair:1:offer:"));
        assert!(json.contains("gitpass-pair:1:grant:"));
        assert!(!json.contains(PASSWORD));
        assert!(!json.contains("password"));
        assert_eq!(
            test_vector_open().unwrap().secret_key.encode(),
            payload.secret_key.encode()
        );
    }

    #[test]
    fn unlock_after_pairing_registers_the_new_device() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let created = store::init(root, PASSWORD, fast()).unwrap();
        let opts = UnlockOpts {
            accept_rollback: false,
            target_kdf: fast(),
        };
        let desktop =
            Session::unlock_device(root, PASSWORD, &created, &opts, Some("FAKE desktop")).unwrap();
        let account = desktop.fmt.account_id.clone();
        drop(desktop);
        let now = now_unix();
        let mut joiner = JoinerSession::begin(now).unwrap();
        let request = PairingRequest::new(
            &account,
            "fake-owner",
            "fake-vault",
            "main",
            SecretKey(created.0),
            TOKEN,
        )
        .unwrap();
        let mut host = HostSession::accept(
            &joiner.offer(),
            request,
            now,
            &mut ConsumedPairings::default(),
        )
        .unwrap();
        let code = joiner.read_announce(&host.announce(), now).unwrap();
        let grant = host.grant_if_code_matches(&code, now).unwrap();
        assert_clean(&grant);
        let payload = joiner
            .open(&grant, &code, now, &mut ConsumedPairings::default())
            .unwrap();
        assert_eq!(payload.secret_key.encode(), created.encode());
        assert!(!format!("{payload:?}").contains(PASSWORD));
        joiner.consume();
        assert_eq!(
            joiner
                .open(&grant, &code, now, &mut ConsumedPairings::default())
                .unwrap_err(),
            PairingError::Replay
        );
        fs::remove_file(root.join(crate::device::LOCAL_DEVICE_FILE)).unwrap();
        let key_file = root.join(crate::device::LOCAL_DEVICE_KEY_FILE);
        if key_file.exists() {
            fs::remove_file(key_file).unwrap();
        }
        let phone = Session::unlock_device(
            root,
            PASSWORD,
            &payload.secret_key,
            &opts,
            Some("FAKE phone"),
        )
        .unwrap();
        let names: Vec<String> = phone
            .devices()
            .unwrap()
            .devices
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert!(names.iter().any(|n| n == "FAKE desktop"));
        assert!(names.iter().any(|n| n == "FAKE phone"));
    }

    #[test]
    fn three_wrong_codes_burn_the_session_and_classic_tokens_are_refused() {
        assert_eq!(
            PairingRequest::new(
                TV_ACCOUNT,
                "fake-owner",
                "fake-vault",
                "main",
                SecretKey([0xAB; 16]),
                "ghp_FAKE0123456789"
            )
            .unwrap_err(),
            PairingError::BadField
        );
        let now = 1_700_000_000;
        let mut joiner = JoinerSession::from_parts([0x11; 32], [0x55; 16], now + PAIRING_TTL_SECS).unwrap();
        let mut host = HostSession::accept_with(
            &joiner.offer(),
            req(TOKEN),
            now,
            &mut ConsumedPairings::default(),
            Some([0x22; 32]),
        )
        .unwrap();
        let code = joiner.read_announce(&host.announce(), now).unwrap();
        let bad = wrong(&code);
        assert_eq!(host.grant_if_code_matches(&bad, now).unwrap_err(), PairingError::BadCode);
        assert_eq!(host.grant_if_code_matches(&bad, now).unwrap_err(), PairingError::BadCode);
        assert_eq!(
            host.grant_if_code_matches(&bad, now).unwrap_err(),
            PairingError::Exhausted
        );
        assert_eq!(
            host.grant_if_code_matches(&code, now).unwrap_err(),
            PairingError::Replay
        );
        let mut joiner = JoinerSession::from_parts([0x12; 32], [0x56; 16], now + PAIRING_TTL_SECS).unwrap();
        let mut host = HostSession::accept_with(
            &joiner.offer(),
            req(TOKEN),
            now,
            &mut ConsumedPairings::default(),
            Some([0x23; 32]),
        )
        .unwrap();
        let code = joiner.read_announce(&host.announce(), now).unwrap();
        let grant = host.grant_with_nonce(&code, now, Some([0x47; 24])).unwrap();
        let bad = wrong(&code);
        let mut consumed = ConsumedPairings::default();
        assert_eq!(
            joiner.open(&grant, &bad, now, &mut consumed).unwrap_err(),
            PairingError::BadCode
        );
        assert_eq!(
            joiner.open(&grant, &bad, now, &mut consumed).unwrap_err(),
            PairingError::BadCode
        );
        assert_eq!(
            joiner.open(&grant, &bad, now, &mut consumed).unwrap_err(),
            PairingError::Exhausted
        );
        assert_eq!(
            joiner.open(&grant, &code, now, &mut consumed).unwrap_err(),
            PairingError::Replay
        );
    }

    #[test]
    fn clock_skew_boundary_and_replay_prune() {
        let now = 1_700_000_000;
        // A peer clock 60s ahead of this one still fits (exp = now + ttl + 60). 61s does not.
        assert!(check_window(now, now + PAIRING_TTL_SECS + 60).is_ok());
        assert_eq!(
            check_window(now, now + PAIRING_TTL_SECS + 61).unwrap_err(),
            PairingError::BadMessage
        );
        assert_eq!(check_window(now, now).unwrap_err(), PairingError::Expired);
        let dir = tempfile::tempdir().unwrap();
        let mut c = ConsumedPairings::default();
        let fresh_exp = now + 30;
        let stale_exp = now - RETAIN_AFTER_EXP_SECS - 1;
        c.mark_accepted("freshSID", fresh_exp, 1).unwrap();
        c.mark_released("staleSID", stale_exp, 1).unwrap();
        save_consumed(dir.path(), &c).unwrap();
        let mut loaded = load_consumed(dir.path()).unwrap();
        assert!(loaded.is_used("freshSID", now));
        assert!(!loaded.is_used("staleSID", now));
        save_consumed(dir.path(), &loaded).unwrap();
        let raw = fs::read_to_string(dir.path().join(CONSUMED_FILE)).unwrap();
        assert!(raw.contains("freshSID"));
        assert!(!raw.contains("staleSID"));
    }

    #[test]
    fn tamper_inside_ciphertext_nonce_or_key_fails_authentication() {
        let now = 1_700_000_000;
        let mut joiner = JoinerSession::from_parts([0x11; 32], [0x33; 16], now + PAIRING_TTL_SECS).unwrap();
        let mut host = HostSession::accept_with(
            &joiner.offer(),
            req(TOKEN),
            now,
            &mut ConsumedPairings::default(),
            Some([0x22; 32]),
        )
        .unwrap();
        let code = joiner.read_announce(&host.announce(), now).unwrap();
        let grant = host.grant_with_nonce(&code, now, Some([0x44; 24])).unwrap();
        let mut consumed = ConsumedPairings::default();
        for bad in [flip_ct(&grant), flip_nonce(&grant), seal_with_other_key(&grant)] {
            assert_eq!(
                joiner.open(&bad, &code, now, &mut consumed).unwrap_err(),
                PairingError::Tampered,
                "{bad}"
            );
        }
        joiner
            .open(&grant, &code, now, &mut consumed)
            .unwrap();
    }

    fn envelope_of(grant: &str) -> (String, crate::crypto::Envelope) {
        let dot = grant.rfind('.').unwrap();
        let (head, blob) = grant.split_at(dot);
        let raw = B64U.decode(&blob[1..]).unwrap();
        let env = serde_json::from_slice(&raw).unwrap();
        (head.to_string(), env)
    }
    fn grant_with(head: &str, env: &crate::crypto::Envelope) -> String {
        let raw = serde_json::to_vec(env).unwrap();
        format!("{head}.{}", b64(&raw))
    }
    fn flip_ct(grant: &str) -> String {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine;
        let (head, mut env) = envelope_of(grant);
        let mut ct = B64.decode(&env.ct).unwrap();
        let i = ct.len() / 2;
        ct[i] ^= 0x01;
        env.ct = B64.encode(ct);
        grant_with(&head, &env)
    }
    fn flip_nonce(grant: &str) -> String {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine;
        let (head, mut env) = envelope_of(grant);
        let mut nonce = B64.decode(&env.nonce).unwrap();
        nonce[0] ^= 0x01;
        env.nonce = B64.encode(nonce);
        grant_with(&head, &env)
    }
    fn seal_with_other_key(grant: &str) -> String {
        let (head, env) = envelope_of(grant);
        let wrong = crate::crypto::Key32([0x99; 32]);
        let sealed = crate::crypto::seal_with_nonce(&wrong, b"{}", b"aad", [0x48; 24]).unwrap();
        let _ = env;
        grant_with(&head, &sealed)
    }
}
