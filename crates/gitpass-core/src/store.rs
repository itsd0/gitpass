//! On-disk vault layout (format v2) and envelope hierarchy: AUK -> Account Key -> Vault Key -> Item,
//! plus the authenticated manifest that protects against rollback / file deletion / substitution.
use crate::crypto::*;
use crate::merge::{self, ItemRecord, MergeReport};
use crate::fsutil::{self, atomic_write};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Highest on-disk format this build understands. Newer versions are refused.
pub const FORMAT_VERSION: u32 = 5;
/// Oldest on-disk format this build still reads (v2/v3 are upgraded lazily, on next write).
pub const MIN_READ_FORMAT: u32 = 2;
/// Manifest format written by merge/device features when no v4 structure (identity / members) exists.
/// format.json stays at 2 until the keyset is re-wrapped, so keyset AAD and existing Emergency Kits stay valid.
pub const MANIFEST_FORMAT_V3: u32 = 3;
/// Manifest format written once the account has an identity keypair or any vault has members.
pub const MANIFEST_FORMAT_V4: u32 = 4;
/// Manifest format written once any vault has encrypted attachment files.
pub const MANIFEST_FORMAT_V5: u32 = 5;
/// Directory (inside a vault) holding encrypted attachments.
pub const FILES_DIR: &str = "files";
pub const IDENTITY_PUB_FILE: &str = "identity.pub";
pub const IDENTITY_ENC_FILE: &str = "identity.enc";
pub const REVOKE_PENDING_FILE: &str = ".gitpass-revoke-pending.json";
/// format.json version written by `init`. It is only bumped to FORMAT_VERSION when the keyset is re-wrapped
/// (passwd / rotate-secret / KDF upgrade / v1 migrate), because the keyset AAD binds the whole format.json.
pub const INIT_FORMAT_VERSION: u32 = MIN_READ_FORMAT;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Format {
    pub version: u32,
    pub kdf: KdfParams,
    pub salt: String,
    pub account_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize, ZeroizeOnDrop)]
pub struct Item {
    pub title: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp: Option<String>,
    pub updated_at: String,
}

/// Item file: plaintext version (needed to rebuild AAD) + encrypted, padded envelope.
#[derive(Serialize, Deserialize, Debug)]
pub struct ItemFile {
    pub version: u64,
    pub envelope: Envelope,
}

/// vault.key.enc: Vault Key wrapped under the Account Key.
#[derive(Serialize, Deserialize, Debug)]
pub struct VaultKeyFile {
    pub key_version: u32,
    pub envelope: Envelope,
}

/// Decrypted content of meta.enc.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct VaultMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// v5: UI icon name and color (encrypted with the rest of the meta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ItemEntry {
    pub version: u64,
    /// blake3 of the item file bytes
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct VaultEntry {
    pub key_version: u32,
    pub key_hash: String,
    pub meta_hash: String,
    pub items: BTreeMap<String, ItemEntry>,
    /// v4: recipient fingerprint -> blake3 of `members/<fpr>.enc`. Absent (empty) before v4.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub members: BTreeMap<String, String>,
    /// v5: attachment file id -> blake3 of `files/<id>.enc`. Absent (empty) before v5.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

/// v5 `vaults/<vid>/files/<fid>.enc`: an attachment sealed under the Vault Key (padded).
#[derive(Serialize, Deserialize, Debug)]
pub struct FileBlob { pub format: u32, pub item_id: String, pub envelope: Envelope }
pub fn file_aad(vault_id: &str, item_id: &str, file_id: &str) -> Vec<u8> { format!("gitpass/file/v5|{vault_id}|{item_id}|{file_id}").into_bytes() }

/// v4: hashes of the account identity files.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityEntry {
    pub pub_hash: String,
    pub enc_hash: String,
}

/// Plaintext `identity.pub` (committed). Integrity is covered by the owner's manifest; other people must
/// compare the fingerprint out of band (there is no PKI / public signature).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityPub {
    pub format: u32,
    pub account_id: String,
    pub public: String,
    pub fingerprint: String,
}

/// `vaults/<vid>/members/<fpr>.enc`: the Vault Key sealed to a recipient's identity key.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberFile {
    pub format: u32,
    pub vault_id: String,
    pub key_version: u32,
    pub recipient_fpr: String,
    pub recipient_pub: String,
    pub sender_fpr: String,
    pub sender_pub: String,
    pub sealed: crate::keys::SealedBox,
    /// Ed25519 verifying key (base64) of the sender's share-signing key.
    /// `#[serde(default)]` so pre-signature member files still parse — and
    /// are then rejected as unsigned by `accept_share`.
    #[serde(default)]
    pub sender_sign_pub: String,
    /// Ed25519 signature (base64) over `share_sig_payload` for this file.
    #[serde(default)]
    pub sender_sig: String,
    /// Sender binding tag (base64): BLAKE3-keyed over `share_sig_payload`,
    /// keyed by DH(sender X25519 secret, recipient public). Proves the share
    /// came from the holder of `sender_pub`, which the Ed25519 signature alone
    /// cannot (its verifying key is carried in the file).
    #[serde(default)]
    pub sender_auth: String,
}
pub fn member_aad(mf: &MemberFile) -> Vec<u8> {
    format!("gitpass/member/v4|{}|{}|{}|{}|{}|{}", mf.vault_id, mf.key_version, mf.recipient_fpr, mf.recipient_pub, mf.sender_fpr, mf.sender_pub).into_bytes()
}

/// Canonical payload a share signature covers: every field that decides what
/// the recipient imports, including the sealed vault key itself. Changing any
/// of them after signing invalidates the signature.
pub fn share_sig_payload(mf: &MemberFile) -> Vec<u8> {
    format!(
        "gitpass/share-sig/v1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        mf.vault_id, mf.key_version, mf.recipient_fpr, mf.recipient_pub, mf.sender_fpr, mf.sender_pub,
        mf.sender_sign_pub, mf.sealed.alg, mf.sealed.eph, mf.sealed.envelope.alg, mf.sealed.envelope.nonce,
        mf.sealed.envelope.ct
    )
    .into_bytes()
}

/// Fingerprint of a sender's Ed25519 signing key (base64), for out-of-band
/// verification on first contact. Empty when the file carries no signing key.
pub fn sender_sign_fingerprint(mf: &MemberFile) -> String {
    let Ok(pk) = B64.decode(&mf.sender_sign_pub) else { return String::new() };
    let Ok(pk): Result<[u8; 32], _> = pk.try_into() else { return String::new() };
    crate::keys::fingerprint(&pk)
}
fn identity_key(ak: &Key32) -> Key32 { hkdf32(&ak.0, None, b"gitpass/identity/v4") }
fn identity_aad(account_id: &str, public_b64: &str) -> Vec<u8> { format!("{account_id}|identity|4|{public_b64}").into_bytes() }
fn device_kek(auk: &Key32, account_id: &str) -> Key32 { hkdf32(&auk.0, Some(account_id.as_bytes()), b"gitpass/device-key/v4") }

/// Result of [`Session::revoke_device_and_rotate`].
#[derive(Debug, Clone)]
pub struct RevokeReport {
    pub device_id: String,
    pub vaults_rotated: Vec<String>,
    pub account_key_backup: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct RevokePending { device_id: String }

/// Plaintext of manifest.enc. BTreeMaps + fixed struct order => deterministic serde_json encoding.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Manifest {
    pub format: u32,
    pub account_id: String,
    pub epoch: u64,
    pub keyset_hash: String,
    pub vaults: BTreeMap<String, VaultEntry>,
    /// blake3 of devices.enc (format v3). Absent in v2 manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices_hash: Option<String>,
    /// v4: identity.pub / identity.enc hashes. Absent before v4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityEntry>,
}

/// Manifest format implied by content: 4 if any v4 structure is present, else 3.
pub fn manifest_format_for(m: &Manifest) -> u32 {
    if m.vaults.values().any(|v| !v.files.is_empty()) { MANIFEST_FORMAT_V5 }
    else if m.identity.is_some() || m.vaults.values().any(|v| !v.members.is_empty()) { MANIFEST_FORMAT_V4 } else { MANIFEST_FORMAT_V3 }
}

pub fn item_aad(vault_id: &str, item_id: &str, version: u64) -> Vec<u8> {
    format!("{vault_id}|{item_id}|{version}").into_bytes()
}
/// keyset.enc AAD binds the full canonical format.json content (KDF params, salt, account id, version).
pub fn keyset_aad(fmt: &Format) -> Vec<u8> {
    let mut v = b"gitpass/keyset/v2|".to_vec();
    v.extend(serde_json::to_vec(fmt).expect("format serializes"));
    v
}
pub fn vault_key_aad(vault_id: &str, key_version: u32) -> Vec<u8> { format!("{vault_id}|vault-key|{key_version}").into_bytes() }
pub fn meta_aad(vault_id: &str) -> Vec<u8> { format!("{vault_id}|meta|2").into_bytes() }
fn manifest_aad(account_id: &str) -> Vec<u8> { format!("{account_id}|manifest|2").into_bytes() }
pub fn manifest_key(account_key: &Key32) -> Key32 { hkdf32(&account_key.0, None, b"gitpass/manifest/v2") }

// ---------- padding ----------
/// Padded size for a payload of `n` bytes (+4-byte length header): 512, 1K, 2K, 4K, then powers of two.
pub fn bucket(n: usize) -> usize {
    let total = n + 4;
    for b in [512usize, 1024, 2048, 4096] { if total <= b { return b; } }
    total.next_power_of_two()
}
pub fn pad(pt: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bucket(pt.len()));
    out.extend_from_slice(&(pt.len() as u32).to_le_bytes());
    out.extend_from_slice(pt);
    out.resize(bucket(pt.len()), 0);
    out
}
pub fn unpad(buf: &[u8]) -> Result<Vec<u8>> {
    if buf.len() < 4 { bail!("padded payload too short"); }
    let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
    if n > buf.len() - 4 { bail!("padded payload length out of range"); }
    if buf[4 + n..].iter().any(|&b| b != 0) { bail!("bad padding"); }
    Ok(buf[4..4 + n].to_vec())
}

// ---------- io helpers ----------
pub fn hash_bytes(b: &[u8]) -> String { blake3::hash(b).to_hex().to_string() }
pub fn hash_file(p: &Path) -> Result<String> {
    Ok(hash_bytes(&fs::read(p).with_context(|| format!("reading {}", p.display()))?))
}
/// Atomically write pretty JSON; returns blake3 of the bytes written.
pub(crate) fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<String> {
    let b = serde_json::to_vec_pretty(v)?;
    atomic_write(p, &b)?;
    Ok(hash_bytes(&b))
}
pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> Result<T> {
    let b = fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    serde_json::from_slice(&b).with_context(|| format!("parsing {}", p.display()))
}

pub fn read_format(root: &Path) -> Result<Format> {
    let f: Format = read_json(&root.join("format.json")).context("not initialized? run `gitpass init`")?;
    if f.version > FORMAT_VERSION {
        bail!("format version {} is newer than this gitpass supports ({FORMAT_VERSION}); upgrade gitpass", f.version);
    }
    Ok(f)
}

// ---------- local (non-git) state: last seen epoch ----------
#[derive(Serialize, Deserialize, Default)]
struct LocalState { account_id: String, epoch: u64 }
fn state_path(root: &Path) -> PathBuf { root.join(".gitpass-state.json") }
/// Last manifest epoch this device has seen for `account_id` (None = first use on this device: TOFU).
pub fn last_epoch(root: &Path, account_id: &str) -> Option<u64> {
    let s: LocalState = read_json(&state_path(root)).ok()?;
    (s.account_id == account_id).then_some(s.epoch)
}
fn save_epoch(root: &Path, account_id: &str, epoch: u64) -> Result<()> {
    write_json(&state_path(root), &LocalState { account_id: account_id.into(), epoch }).map(|_| ())
}

fn seal_manifest(root: &Path, ak: &Key32, m: &Manifest) -> Result<()> {
    let env = seal(&manifest_key(ak), &serde_json::to_vec(m)?, &manifest_aad(&m.account_id))?;
    write_json(&root.join("manifest.enc"), &env)?;
    Ok(())
}

/// Create a new v2 account. Returns the generated Secret Key.
pub fn init(root: &Path, password: &str, kdf: KdfParams) -> Result<SecretKey> {
    if root.join("format.json").exists() { bail!("already initialized at {}", root.display()); }
    fs::create_dir_all(root.join("vaults"))?;
    let _g = fsutil::lock(root)?;
    let salt: [u8; 16] = random_bytes();
    let account_id = uuid::Uuid::new_v4().to_string();
    let secret = SecretKey::generate();
    let fmt = Format { version: INIT_FORMAT_VERSION, kdf, salt: B64.encode(salt), account_id: account_id.clone() };
    let auk = derive_auk(password, &secret, &salt, &account_id, &fmt.kdf)?;
    let account_key = Key32::random();
    let env = wrap_key(&auk, &account_key, &keyset_aad(&fmt))?;
    let keyset_hash = write_json(&root.join("keyset.enc"), &env)?;
    let m = Manifest { format: INIT_FORMAT_VERSION, account_id: account_id.clone(), epoch: 1, keyset_hash, vaults: BTreeMap::new(), devices_hash: None, identity: None };
    seal_manifest(root, &account_key, &m)?;
    atomic_write(&root.join(".gitignore"), fsutil::GITIGNORE.as_bytes())?;
    write_json(&root.join("format.json"), &fmt)?; // last
    save_epoch(root, &account_id, 1)?;
    Ok(secret)
}

/// Compare on-disk files to the manifest. Returns human-readable problems (empty = OK).
pub fn verify(root: &Path, m: &Manifest) -> Vec<String> {
    let mut p = vec![];
    let check = |p: &mut Vec<String>, path: &Path, want: &str| match hash_file(path) {
        Ok(h) if h == want => {}
        Ok(_) => p.push(format!("{} does not match manifest hash (modified or replaced)", path.display())),
        Err(_) => p.push(format!("{} is missing", path.display())),
    };
    check(&mut p, &root.join("keyset.enc"), &m.keyset_hash);
    match &m.devices_hash {
        Some(h) => check(&mut p, &root.join(crate::device::DEVICES_FILE), h),
        None => if root.join(crate::device::DEVICES_FILE).exists() {
            p.push("unexpected devices.enc (not in manifest)".into());
        },
    }
    let (ip, ie) = (root.join(IDENTITY_PUB_FILE), root.join(IDENTITY_ENC_FILE));
    match &m.identity {
        Some(id) => { check(&mut p, &ip, &id.pub_hash); check(&mut p, &ie, &id.enc_hash); }
        None => for f in [&ip, &ie] { if f.exists() { p.push(format!("unexpected {} (not in manifest)", f.display())); } },
    }
    let vd = root.join("vaults");
    let on_disk: Vec<String> = fs::read_dir(&vd).map(|r| r.filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.')).collect()).unwrap_or_default();
    for v in &on_disk { if !m.vaults.contains_key(v) { p.push(format!("unexpected vault directory vaults/{v} (not in manifest)")); } }
    for (vid, ve) in &m.vaults {
        let d = vd.join(vid);
        if !d.is_dir() { p.push(format!("vault {vid} is missing")); continue; }
        check(&mut p, &d.join("vault.key.enc"), &ve.key_hash);
        check(&mut p, &d.join("meta.enc"), &ve.meta_hash);
        let items: Vec<String> = fs::read_dir(d.join("items")).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("enc"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().to_string()).collect()).unwrap_or_default();
        for i in &items { if !ve.items.contains_key(i) { p.push(format!("unexpected item {vid}/{i} (not in manifest)")); } }
        for (iid, ie) in &ve.items { check(&mut p, &d.join("items").join(format!("{iid}.enc")), &ie.hash); }
        for f in list_enc(&d.join("members")) { if !ve.members.contains_key(&f) { p.push(format!("unexpected member {vid}/members/{f} (not in manifest)")); } }
        for (fpr, h) in &ve.members { check(&mut p, &d.join("members").join(format!("{fpr}.enc")), h); }
        for f in list_enc(&d.join(FILES_DIR)) { if !ve.files.contains_key(&f) { p.push(format!("unexpected attachment {vid}/files/{f} (not in manifest)")); } }
        for (fid, h) in &ve.files { check(&mut p, &d.join(FILES_DIR).join(format!("{fid}.enc")), h); }
    }
    p
}

fn list_enc(d: &Path) -> Vec<String> {
    fs::read_dir(d).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("enc"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().to_string()).collect()).unwrap_or_default()
}

#[derive(Default)]
pub struct UnlockOpts {
    /// Accept integrity/rollback failures and re-seal the manifest from what's on disk.
    pub accept_rollback: bool,
    /// KDF target; weaker stored params are auto-upgraded on unlock.
    pub target_kdf: KdfParams,
}

/// Unlocked session holding the Account Key.
pub struct Session {
    pub root: PathBuf,
    pub fmt: Format,
    account_key: Key32,
    /// Set if unlock auto-upgraded the KDF params.
    pub kdf_upgraded: bool,
    /// Integrity problems that were accepted via --accept-rollback.
    pub accepted_problems: Vec<String>,
    /// Stable id of this device (from .gitpass-device.json).
    pub device_id: String,
    /// True if this device is marked revoked in the registry: mutating operations are refused.
    pub device_revoked: bool,
    /// Key (derived from the AUK) protecting this device's local X25519 private key.
    device_kek: Key32,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("root", &self.root).field("account_key", &"<redacted>").finish()
    }
}

fn open_keyset(root: &Path, fmt: &Format, ks: &Path, password: &str, secret: &SecretKey) -> Result<Key32> {
    Ok(open_keyset_auk(root, fmt, ks, password, secret)?.1)
}
fn open_keyset_auk(root: &Path, fmt: &Format, ks: &Path, password: &str, secret: &SecretKey) -> Result<(Key32, Key32)> {
    let salt = B64.decode(&fmt.salt)?;
    let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
    let env: Envelope = read_json(ks)?;
    let _ = root;
    let ak = unwrap_key(&auk, &env, &keyset_aad(fmt))
        .map_err(|_| anyhow!("unlock failed: wrong master password or Secret Key (or format.json/keyset.enc tampered)"))?;
    Ok((auk, ak))
}

/// Roll forward an interrupted keyset re-wrap (format.json.new / keyset.enc.new). Returns true if completed.
fn finish_pending_rewrap(root: &Path, password: &str, secret: &SecretKey) -> Result<bool> {
    let nf_p = root.join("format.json.new");
    if !nf_p.exists() { return Ok(false); }
    let nk_p = root.join("keyset.enc.new");
    let ks = if nk_p.exists() { nk_p.clone() } else { root.join("keyset.enc") };
    let ok = read_json::<Format>(&nf_p).ok().filter(|f| (MIN_READ_FORMAT..=FORMAT_VERSION).contains(&f.version))
        .map(|f| open_keyset(root, &f, &ks, password, secret).is_ok()).unwrap_or(false);
    if ok {
        if nk_p.exists() { fsutil::rename(&nk_p, &root.join("keyset.enc"))?; }
        fsutil::rename(&nf_p, &root.join("format.json"))?;
    } else {
        let _ = fs::remove_file(&nf_p);
        let _ = fs::remove_file(&nk_p);
    }
    Ok(ok)
}

impl Session {
    pub fn unlock(root: &Path, password: &str, secret: &SecretKey) -> Result<Self> {
        Self::unlock_with(root, password, secret, &UnlockOpts::default())
    }

    pub fn unlock_with(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts) -> Result<Self> {
        Self::unlock_device(root, password, secret, opts, None)
    }

    /// Unlock and register/refresh this device in the registry (`device_name` None => env/hostname).
    pub fn unlock_device(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts, device_name: Option<&str>) -> Result<Self> {
        Self::unlock_device_inner(root, password, secret, opts, device_name, true)
    }
    /// Verify-only re-unlock used after a sync pull: same integrity/rollback checks, but never touches the
    /// device registry or upgrades the KDF, so the working tree stays exactly as committed/pulled.
    pub fn reopen_verify(root: &Path, password: &str, secret: &SecretKey, accept_rollback: bool) -> Result<Self> {
        let fmt = read_format(root)?;
        let opts = UnlockOpts { accept_rollback, target_kdf: fmt.kdf.clone() };
        Self::unlock_device_inner(root, password, secret, &opts, None, false)
    }
    fn unlock_device_inner(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts, device_name: Option<&str>, touch: bool) -> Result<Self> {
        let recovered = {
            let _g = fsutil::lock(root)?;
            finish_pending_rewrap(root, password, secret)?
        };
        let fmt = read_format(root)?;
        if fmt.version < MIN_READ_FORMAT { bail!("vault is format v{}; run `gitpass migrate` to upgrade to v{FORMAT_VERSION}", fmt.version); }
        let (auk, account_key) = open_keyset_auk(root, &fmt, &root.join("keyset.enc"), password, secret)?;
        let device_kek = device_kek(&auk, &fmt.account_id);
        drop(auk);
        let device_id = crate::device::local_device_id(root)?;
        let mut s = Session { root: root.to_path_buf(), fmt, account_key, kdf_upgraded: false, accepted_problems: vec![], device_id, device_revoked: false, device_kek };
        if recovered { s.commit(|_| Ok(()))?; } // re-auth keyset hash after roll-forward
        let m = s.load_manifest().context("manifest.enc missing or failed authentication")?;
        if m.account_id != s.fmt.account_id || !(MIN_READ_FORMAT..=FORMAT_VERSION).contains(&m.format) { bail!("manifest does not belong to this account/format"); }
        let mut problems = verify(root, &m);
        let last = last_epoch(root, &s.fmt.account_id);
        if let Some(l) = last {
            if m.epoch < l { problems.push(format!("manifest epoch {} is older than last seen epoch {l} on this device (rollback?)", m.epoch)); }
        }
        if !problems.is_empty() {
            if !opts.accept_rollback {
                bail!("integrity check FAILED, refusing to continue (use --accept-rollback to override):\n  - {}", problems.join("\n  - "));
            }
            s.reseal(m.epoch.max(last.unwrap_or(0)))?;
            s.accepted_problems = problems;
        } else {
            save_epoch(root, &s.fmt.account_id, m.epoch)?;
        }
        if s.fmt.kdf.below(&opts.target_kdf) {
            let k = s.fmt.kdf.max_with(&opts.target_kdf);
            s.rewrap_keyset(password, secret, k)?;
            s.kdf_upgraded = true;
        }
        if touch {
            let name = device_name.map(str::to_string).unwrap_or_else(crate::device::default_device_name);
            s.touch_device(&name)?;
        } else if let Some(d) = s.devices()?.get(&s.device_id) {
            if d.revoked { s.device_revoked = true; }
        }
        // v5: purge soft-deleted items older than 30 days (best effort; never blocks unlock). Skipped on verify-only reopen.
        if touch && !s.device_revoked { let _ = s.purge_expired(now_secs()); }
        // Roll forward an interrupted `device revoke` (marker + key rotation) from this device.
        if let Ok(p) = read_json::<RevokePending>(&root.join(REVOKE_PENDING_FILE)) {
            if touch && !s.device_revoked && p.device_id != s.device_id {
                s.revoke_device_and_rotate(&p.device_id, password, secret)?;
            }
        }
        Ok(s)
    }

    pub fn load_manifest(&self) -> Result<Manifest> {
        let env: Envelope = read_json(&self.root.join("manifest.enc"))?;
        let pt = zeroize::Zeroizing::new(open(&manifest_key(&self.account_key), &env, &manifest_aad(&self.fmt.account_id))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    pub fn epoch(&self) -> Result<u64> { Ok(self.load_manifest()?.epoch) }

    /// Apply `f` to the manifest under the repo lock, bump epoch, re-hash keyset, re-seal.
    fn commit<F: FnOnce(&mut Manifest) -> Result<()>>(&self, f: F) -> Result<u64> {
        let _g = fsutil::lock(&self.root)?;
        self.commit_locked(f)
    }
    fn commit_locked<F: FnOnce(&mut Manifest) -> Result<()>>(&self, f: F) -> Result<u64> {
        let mut m = self.load_manifest()?;
        f(&mut m)?;
        m.format = manifest_format_for(&m); // any write upgrades the manifest to v3 (v4 if identity/members exist)
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        Ok(m.epoch)
    }

    /// Rebuild the manifest from whatever on-disk files authenticate correctly (used by --accept-rollback).
    fn reseal(&self, prev_epoch: u64) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let mut m = Manifest { format: FORMAT_VERSION, account_id: self.fmt.account_id.clone(), epoch: prev_epoch + 1,
            keyset_hash: hash_file(&self.root.join("keyset.enc"))?, vaults: BTreeMap::new(), devices_hash: None, identity: None };
        if self.load_identity().is_ok() {
            m.identity = Some(IdentityEntry { pub_hash: hash_file(&self.root.join(IDENTITY_PUB_FILE))?, enc_hash: hash_file(&self.root.join(IDENTITY_ENC_FILE))? });
        }
        if crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id).ok().flatten().is_some() {
            m.devices_hash = Some(hash_file(&self.root.join(crate::device::DEVICES_FILE))?);
            m.format = MANIFEST_FORMAT_V3;
        }
        if let Ok(rd) = fs::read_dir(self.root.join("vaults")) {
            for e in rd.filter_map(|e| e.ok()) {
                let vid = e.file_name().to_string_lossy().to_string();
                if vid.starts_with('.') || !e.path().is_dir() { continue; }
                let d = e.path();
                let Ok(kf) = read_json::<VaultKeyFile>(&d.join("vault.key.enc")) else { continue };
                let Ok(vk) = unwrap_key(&self.account_key, &kf.envelope, &vault_key_aad(&vid, kf.key_version)) else { continue };
                let Ok(menv) = read_json::<Envelope>(&d.join("meta.enc")) else { continue };
                if open(&vk, &menv, &meta_aad(&vid)).is_err() { continue; }
                let mut ve = VaultEntry { key_version: kf.key_version, key_hash: hash_file(&d.join("vault.key.enc"))?,
                    meta_hash: hash_file(&d.join("meta.enc"))?, items: BTreeMap::new(), members: BTreeMap::new(), files: BTreeMap::new() };
                for fid in list_enc(&d.join(FILES_DIR)) {
                    let fp = d.join(FILES_DIR).join(format!("{fid}.enc"));
                    if let Ok(b) = read_json::<FileBlob>(&fp) {
                        if open(&vk, &b.envelope, &file_aad(&vid, &b.item_id, &fid)).is_ok() { ve.files.insert(fid, hash_file(&fp)?); }
                    }
                }
                // Member files are sealed to other people and cannot be authenticated by us; keep the
                // structurally valid ones that match this vault/key version.
                for fpr in list_enc(&d.join("members")) {
                    let mp = d.join("members").join(format!("{fpr}.enc"));
                    if let Ok(mf) = read_json::<MemberFile>(&mp) {
                        if mf.vault_id == vid && mf.key_version == kf.key_version && mf.recipient_fpr == fpr {
                            ve.members.insert(fpr, hash_file(&mp)?);
                        }
                    }
                }
                for ie in fs::read_dir(d.join("items")).into_iter().flatten().filter_map(|e| e.ok()) {
                    let p = ie.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("enc") { continue; }
                    let iid = p.file_stem().unwrap().to_string_lossy().to_string();
                    let Ok(f) = read_json::<ItemFile>(&p) else { continue };
                    if open(&vk, &f.envelope, &item_aad(&vid, &iid, f.version)).is_ok() {
                        ve.items.insert(iid, ItemEntry { version: f.version, hash: hash_file(&p)? });
                    }
                }
                m.vaults.insert(vid, ve);
            }
        }
        m.format = manifest_format_for(&m).max(m.format);
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)
    }

    fn vdir(&self, id: &str) -> PathBuf { self.root.join("vaults").join(id) }

    pub fn create_vault(&self, name: &str) -> Result<String> { self.create_vault_with(name, None) }

    pub fn create_vault_with(&self, name: &str, description: Option<String>) -> Result<String> {
        self.create_vault_full(name, description, None, None)
    }

    /// v5: create a vault with description, icon and color (all stored in the encrypted meta).
    pub fn create_vault_full(&self, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<String> {
        self.ensure_not_revoked()?;
        if self.list_vaults()?.iter().any(|(_, m)| m.name == name) { bail!("vault '{name}' already exists"); }
        let id = uuid::Uuid::new_v4().to_string();
        self.commit(|m| {
            let vk = Key32::random();
            let d = self.vdir(&id);
            let key_hash = write_json(&d.join("vault.key.enc"),
                &VaultKeyFile { key_version: 1, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(&id, 1))? })?;
            let meta = VaultMeta { name: name.into(), description, icon: icon.clone(), color: color.clone() };
            let mut pt = serde_json::to_vec(&meta)?;
            let env = seal(&vk, &pt, &meta_aad(&id));
            pt.zeroize();
            let meta_hash = write_json(&d.join("meta.enc"), &env?)?;
            fs::create_dir_all(d.join("items"))?;
            atomic_write(&d.join("items/.keep"), b"")?;
            m.vaults.insert(id.clone(), VaultEntry { key_version: 1, key_hash, meta_hash, items: BTreeMap::new(), members: BTreeMap::new(), files: BTreeMap::new() });
            Ok(())
        })?;
        Ok(id)
    }

    /// Rename a vault and/or change its description, icon and color (re-seals the encrypted meta).
    pub fn update_vault_meta(&self, vid: &str, meta: VaultMeta) -> Result<()> {
        self.ensure_not_revoked()?;
        let name = meta.name.trim().to_string();
        if name.is_empty() { bail!("vault name is empty"); }
        if self.list_vaults()?.iter().any(|(id, m)| id != vid && m.name == name) { bail!("vault '{name}' already exists"); }
        let vk = self.vault_key(vid)?;
        let meta = VaultMeta { name, description: meta.description.clone(), icon: meta.icon.clone(), color: meta.color.clone() };
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("no vault {vid}"))?;
            let mut pt = serde_json::to_vec(&meta)?;
            let env = seal(&vk, &pt, &meta_aad(vid));
            pt.zeroize();
            ve.meta_hash = write_json(&self.vdir(vid).join("meta.enc"), &env?)?;
            Ok(())
        })?;
        Ok(())
    }

    /// Permanently delete a vault: its items, attachments, history, key and member (share) envelopes.
    /// Not recoverable except from git history / backups. The last remaining vault can't be deleted.
    pub fn delete_vault(&self, vid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let vaults = self.list_vaults()?;
        if !vaults.iter().any(|(id, _)| id == vid) { bail!("no vault {vid}"); }
        if vaults.len() <= 1 { bail!("can't delete the last vault"); }
        let d = self.vdir(vid);
        self.commit(|m| {
            m.vaults.remove(vid);
            if d.exists() { fs::remove_dir_all(&d)?; }
            fsutil::fsync_dir(&self.root.join("vaults"));
            Ok(())
        })?;
        Ok(())
    }

    pub fn list_vaults(&self) -> Result<Vec<(String, VaultMeta)>> {
        let m = self.load_manifest()?;
        let mut out = vec![];
        for vid in m.vaults.keys() { out.push((vid.clone(), self.vault_meta(vid)?)); }
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(out)
    }

    pub fn vault_meta(&self, vid: &str) -> Result<VaultMeta> {
        let vk = self.vault_key(vid)?;
        let env: Envelope = read_json(&self.vdir(vid).join("meta.enc"))?;
        let pt = zeroize::Zeroizing::new(open(&vk, &env, &meta_aad(vid))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    /// Resolve a vault by name or id.
    pub fn resolve_vault(&self, q: &str) -> Result<String> {
        self.list_vaults()?
            .into_iter()
            .find(|(id, m)| m.name == q || id == q)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| anyhow!("no vault named '{q}'"))
    }

    fn vault_key(&self, vid: &str) -> Result<Key32> {
        let f: VaultKeyFile = read_json(&self.vdir(vid).join("vault.key.enc"))?;
        unwrap_key(&self.account_key, &f.envelope, &vault_key_aad(vid, f.key_version))
    }

    fn write_item_in(dir: &Path, vk: &Key32, vid: &str, iid: &str, version: u64, rec: &ItemRecord) -> Result<String> {
        let mut pt = serde_json::to_vec(rec)?;
        let mut padded = pad(&pt);
        let env = seal(vk, &padded, &item_aad(vid, iid, version));
        pt.zeroize();
        padded.zeroize();
        write_json(&dir.join("items").join(format!("{iid}.enc")), &ItemFile { version, envelope: env? })
    }

    fn put_item(&self, vid: &str, iid: &str, version: u64, item: &ItemRecord) -> Result<()> {
        self.ensure_not_revoked()?;
        let vk = self.vault_key(vid)?;
        let vid_s = vid.to_string();
        self.commit(|m| {
            let ve = m.vaults.get_mut(&vid_s).ok_or_else(|| anyhow!("vault {vid_s} not in manifest"))?;
            let hash = Self::write_item_in(&self.vdir(vid), &vk, vid, iid, version, item)?;
            ve.items.insert(iid.to_string(), ItemEntry { version, hash });
            Ok(())
        })?;
        Ok(())
    }

    pub fn add_item(&self, vid: &str, item: &Item) -> Result<String> {
        self.add_record(vid, ItemRecord::new(item.clone()))
    }

    pub fn read_item(&self, vid: &str, iid: &str) -> Result<(u64, Item)> {
        let (v, r) = self.read_record(vid, iid)?;
        Ok((v, r.item.clone()))
    }

    /// Read the full v3 record. v2 payloads have `clocks: None`; use [`ItemRecord::clocks_or_legacy`].
    pub fn read_record(&self, vid: &str, iid: &str) -> Result<(u64, ItemRecord)> {
        let f: ItemFile = read_json(&self.vdir(vid).join("items").join(format!("{iid}.enc")))?;
        let vk = self.vault_key(vid)?;
        let padded = zeroize::Zeroizing::new(open(&vk, &f.envelope, &item_aad(vid, iid, f.version))?);
        let pt = zeroize::Zeroizing::new(unpad(&padded)?);
        let rec: ItemRecord = serde_json::from_slice(&pt)?;
        Ok((f.version, rec))
    }

    /// Write `item`, stamping a fresh clock on every field whose value changed (clearing a field to `None`
    /// is a tombstone write and also bumps its clock). Note: this overwrites whatever is on disk; when
    /// applying an item from another device use [`Session::merge_item`] instead.
    pub fn update_item(&self, vid: &str, iid: &str, item: &Item) -> Result<()> {
        let (_, old) = self.read_record(vid, iid)?;
        let mut rec = old.clone();
        rec.item = item.clone();
        self.update_record(vid, iid, rec)
    }

    /// Field-merge `remote` into the on-disk item `iid` and write the result (version+1, manifest re-sealed)
    /// if anything changed. Equal-clock conflicts keep the local value and are reported.
    pub fn merge_item(&self, vid: &str, iid: &str, remote: &ItemRecord) -> Result<MergeReport> {
        let (v, local) = self.read_record(vid, iid)?;
        let (merged, rep) = merge::merge_records(&local, remote);
        if rep.changed_local() || local.clocks.is_none() {
            self.put_item(vid, iid, v + 1, &merged)?;
        }
        Ok(rep)
    }

    /// Insert a record verbatim under a given id (used by `sync --merge` to re-apply local-only items).
    pub fn put_record(&self, vid: &str, iid: &str, rec: &ItemRecord) -> Result<()> {
        let v = self.read_record(vid, iid).map(|(v, _)| v + 1).unwrap_or(1);
        self.put_item(vid, iid, v, rec)
    }

    /// Decrypt a manifest.enc blob (e.g. from `git show <rev>:manifest.enc`) with this account's key.
    pub fn open_manifest_bytes(&self, bytes: &[u8]) -> Result<Manifest> {
        let env: Envelope = serde_json::from_slice(bytes)?;
        let pt = zeroize::Zeroizing::new(open(&manifest_key(&self.account_key), &env, &manifest_aad(&self.fmt.account_id))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    pub fn remove_item(&self, vid: &str, iid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let p = self.vdir(vid).join("items").join(format!("{iid}.enc"));
        if !p.exists() { bail!("no item {iid}"); }
        let fids: Vec<String> = self.read_record(vid, iid).map(|(_, r)| r.attachments.iter().map(|a| a.id.clone()).collect()).unwrap_or_default();
        self.commit(|m| {
            fs::remove_file(&p)?;
            fsutil::fsync_dir(p.parent().unwrap());
            if let Some(ve) = m.vaults.get_mut(vid) {
                ve.items.remove(iid);
                for f in &fids {
                    let fp = self.vdir(vid).join(FILES_DIR).join(format!("{f}.enc"));
                    if fp.exists() { fs::remove_file(&fp)?; }
                    ve.files.remove(f);
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    pub fn list_items(&self, vid: &str) -> Result<Vec<(String, Item)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let mut out = vec![];
        for iid in ve.items.keys() {
            let (_, item) = self.read_item(vid, iid)?;
            out.push((iid.clone(), item));
        }
        out.sort_by(|a, b| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()));
        Ok(out)
    }

    /// Find item(s) by id, id prefix, or case-insensitive title substring.
    pub fn find_items(&self, vid: &str, q: &str) -> Result<Vec<(String, Item)>> {
        let ql = q.to_lowercase();
        let all = self.list_items(vid)?;
        if let Some(exact) = all.iter().find(|(id, _)| id == q) { return Ok(vec![exact.clone()]); }
        Ok(all.into_iter().filter(|(id, it)| id.starts_with(q) || it.title.to_lowercase().contains(&ql)).collect())
    }

    pub fn resolve_item(&self, vid: &str, q: &str) -> Result<String> {
        let m = self.find_items(vid, q)?;
        match m.len() {
            0 => bail!("no item matches '{q}'"),
            1 => Ok(m[0].0.clone()),
            n => bail!("{n} items match '{q}'; use the item id"),
        }
    }

    // ---------- key lifecycle ----------

    /// Re-wrap the Account Key under a new AUK (new salt, given password/secret/KDF). Crash-safe via
    /// format.json.new + keyset.enc.new, rolled forward on next unlock.
    pub fn rewrap_keyset(&mut self, password: &str, secret: &SecretKey, kdf: KdfParams) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let salt: [u8; 16] = random_bytes();
        let nf = Format { version: FORMAT_VERSION, kdf, salt: B64.encode(salt), account_id: self.fmt.account_id.clone() };
        let auk = derive_auk(password, secret, &salt, &nf.account_id, &nf.kdf)?;
        let env = wrap_key(&auk, &self.account_key, &keyset_aad(&nf))?;
        write_json(&self.root.join("format.json.new"), &nf)?;
        write_json(&self.root.join("keyset.enc.new"), &env)?;
        fsutil::rename(&self.root.join("keyset.enc.new"), &self.root.join("keyset.enc"))?;
        fsutil::rename(&self.root.join("format.json.new"), &self.root.join("format.json"))?;
        // Re-protect the local device key under the new AUK-derived key (best effort; local-only file).
        let new_kek = device_kek(&auk, &nf.account_id);
        if self.root.join(crate::device::LOCAL_DEVICE_KEY_FILE).exists() {
            let (kp, _) = crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?;
            crate::device::store_device_key(&self.root, &self.device_id, &new_kek, &kp)?;
        }
        self.device_kek = new_kek;
        self.fmt = nf;
        self.commit_locked(|_| Ok(()))?;
        Ok(())
    }

    /// `gitpass passwd`
    pub fn change_password(&mut self, new_password: &str, secret: &SecretKey, kdf: KdfParams) -> Result<()> {
        self.ensure_not_revoked()?;
        self.rewrap_keyset(new_password, secret, kdf)
    }

    /// `gitpass rotate-secret-key`: returns the new Secret Key (print a new Emergency Kit!).
    pub fn rotate_secret_key(&mut self, password: &str, kdf: KdfParams) -> Result<SecretKey> {
        self.ensure_not_revoked()?;
        let sk = SecretKey::generate();
        self.rewrap_keyset(password, &sk, kdf)?;
        Ok(sk)
    }

    /// `gitpass rotate-account-key`: new Account Key; re-wrap every Vault Key, keyset.enc and the manifest.
    /// A backup is taken first; returns its path.
    pub fn rotate_account_key(&mut self, password: &str, secret: &SecretKey) -> Result<PathBuf> {
        let _g = fsutil::lock(&self.root)?;
        let backup = fsutil::backup(&self.root, "")?;
        let mut m = self.load_manifest()?;
        let new_ak = Key32::random();
        for (vid, ve) in m.vaults.iter_mut() {
            let vk = self.vault_key(vid)?;
            ve.key_hash = write_json(&self.vdir(vid).join("vault.key.enc"),
                &VaultKeyFile { key_version: ve.key_version, envelope: wrap_key(&new_ak, &vk, &vault_key_aad(vid, ve.key_version))? })?;
        }
        let reg = crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?;
        if let Some(r) = &reg {
            m.devices_hash = Some(crate::device::write_registry(&self.root, &new_ak, &self.fmt.account_id, r)?);
            m.format = m.format.max(MANIFEST_FORMAT_V3);
        }
        if m.identity.is_some() {
            let kp = self.load_identity()?;
            m.identity = Some(self.write_identity(&new_ak, &kp)?);
        }
        m.format = manifest_format_for(&m);
        let salt = B64.decode(&self.fmt.salt)?;
        let auk = derive_auk(password, secret, &salt, &self.fmt.account_id, &self.fmt.kdf)?;
        write_json(&self.root.join("keyset.enc"), &wrap_key(&auk, &new_ak, &keyset_aad(&self.fmt))?)?;
        self.account_key = new_ak;
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        Ok(backup)
    }

    /// `gitpass vault rotate`: new Vault Key (key_version+1); re-encrypt meta + all items into a temp dir,
    /// then swap directories.
    pub fn rotate_vault(&self, vid: &str) -> Result<()> { self.rotate_vault_excluding(vid, None) }

    /// Rotate the Vault Key; every remaining member gets the NEW key re-sealed to their identity key,
    /// `exclude` (a member fingerprint) is dropped. Crash-safety as for `rotate_vault` (temp dir + swap).
    fn rotate_vault_excluding(&self, vid: &str, exclude: Option<&str>) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let mut m = self.load_manifest()?;
        let ve = m.vaults.get(vid).cloned().ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let meta = self.vault_meta(vid)?;
        let mut items = vec![];
        for (iid, ie) in &ve.items { let (v, it) = self.read_record(vid, iid)?; items.push((iid.clone(), v.max(ie.version), it)); }
        let vk = Key32::random();
        let kv = ve.key_version + 1;
        let vaults = self.root.join("vaults");
        let tmp = vaults.join(format!(".rotate-{vid}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("items"))?;
        let mut ne = VaultEntry { key_version: kv, items: BTreeMap::new(), ..Default::default() };
        ne.key_hash = write_json(&tmp.join("vault.key.enc"),
            &VaultKeyFile { key_version: kv, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(vid, kv))? })?;
        let mpt = zeroize::Zeroizing::new(serde_json::to_vec(&meta)?);
        ne.meta_hash = write_json(&tmp.join("meta.enc"), &seal(&vk, &mpt, &meta_aad(vid))?)?;
        atomic_write(&tmp.join("items/.keep"), b"")?;
        if !ve.files.is_empty() {
            let old_vk = self.vault_key(vid)?;
            fs::create_dir_all(tmp.join(FILES_DIR))?;
            for fid in ve.files.keys() {
                let b: FileBlob = read_json(&self.vdir(vid).join(FILES_DIR).join(format!("{fid}.enc")))?;
                let pt = zeroize::Zeroizing::new(open(&old_vk, &b.envelope, &file_aad(vid, &b.item_id, fid))?);
                let nb = FileBlob { format: 5, item_id: b.item_id.clone(), envelope: seal(&vk, &pt, &file_aad(vid, &b.item_id, fid))? };
                ne.files.insert(fid.clone(), write_json(&tmp.join(FILES_DIR).join(format!("{fid}.enc")), &nb)?);
            }
        }
        for (iid, v, it) in &items {
            let h = Self::write_item_in(&tmp, &vk, vid, iid, v + 1, it)?;
            ne.items.insert(iid.clone(), ItemEntry { version: v + 1, hash: h });
        }
        if !ve.members.is_empty() {
            let me = self.load_identity()?;
            for fpr in ve.members.keys() {
                if Some(fpr.as_str()) == exclude { continue; }
                let old: MemberFile = read_json(&self.vdir(vid).join("members").join(format!("{fpr}.enc")))?;
                let rpub = crate::keys::parse_pub(&old.recipient_pub)?;
                if crate::keys::fingerprint(&rpub) != *fpr { bail!("member file {fpr} has a mismatched public key"); }
                let h = Self::write_member(&tmp, vid, kv, &vk, &rpub, &me)?;
                ne.members.insert(fpr.clone(), h);
            }
        }
        let old = vaults.join(format!(".old-{vid}"));
        let _ = fs::remove_dir_all(&old);
        fsutil::rename(&self.vdir(vid), &old)?;
        fsutil::rename(&tmp, &self.vdir(vid))?;
        m.vaults.insert(vid.to_string(), ne);
        m.format = manifest_format_for(&m);
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        fs::remove_dir_all(&old)?;
        Ok(())
    }

    // ---------- device registry (v3) ----------

    /// Refuse mutating operations if this device has been revoked.
    pub fn ensure_not_revoked(&self) -> Result<()> {
        if self.device_revoked {
            bail!("this device ({}) has been REVOKED in the device registry; mutating operations are refused. \
                   Allowed: `gitpass device list`, `gitpass rotate-account-key`.", self.device_id);
        }
        Ok(())
    }

    /// Decrypted device registry (empty if devices.enc does not exist yet).
    pub fn devices(&self) -> Result<crate::device::DeviceRegistry> {
        Ok(crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?.unwrap_or_default())
    }

    fn mutate_devices<F: FnOnce(&mut crate::device::DeviceRegistry) -> Result<()>>(&self, f: F) -> Result<()> {
        self.commit(|m| {
            let mut r = crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?.unwrap_or_default();
            f(&mut r)?;
            m.devices_hash = Some(crate::device::write_registry(&self.root, &self.account_key, &self.fmt.account_id, &r)?);
            Ok(())
        })?;
        Ok(())
    }

    /// Register this device or refresh its last_seen_at. A revoked device is not modified; it is flagged.
    pub fn touch_device(&mut self, name: &str) -> Result<()> {
        let reg = self.devices()?;
        if let Some(d) = reg.get(&self.device_id) {
            if d.revoked { self.device_revoked = true; return Ok(()); }
        }
        let (id, now) = (self.device_id.clone(), crate::device::now_iso8601());
        // Throttled: a no-op unlock must not rewrite devices.enc / manifest.enc.
        if reg.needs_touch(&id, name, &now) {
            self.mutate_devices(|r| { r.touch(&id, name, &now); Ok(()) })?;
        }
        if self.has_identity() { self.record_device_key()?; }
        Ok(())
    }

    pub fn rename_device(&self, query: &str, name: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        if name.trim().is_empty() { bail!("device name must not be empty"); }
        let id = self.devices()?.resolve(query)?;
        self.mutate_devices(|r| {
            r.devices.iter_mut().find(|d| d.id == id).unwrap().name = name.trim().into();
            Ok(())
        })?;
        Ok(id)
    }

    /// Mark a device revoked (policy marker only; does NOT re-encrypt keys).
    pub fn revoke_device(&mut self, query: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        let now = crate::device::now_iso8601();
        self.mutate_devices(|r| {
            let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
            if d.revoked { bail!("device {id} is already revoked"); }
            d.revoked = true;
            d.revoked_at = Some(now);
            Ok(())
        })?;
        if id == self.device_id { self.device_revoked = true; }
        Ok(id)
    }

    /// Clear a revocation marker. Must be run from a different, non-revoked device.
    pub fn unrevoke_device(&self, query: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        self.mutate_devices(|r| {
            let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
            if !d.revoked { bail!("device {id} is not revoked"); }
            d.revoked = false;
            d.revoked_at = None;
            Ok(())
        })?;
        Ok(id)
    }

    /// REAL revocation (v4): mark `query` revoked, then rotate every Vault Key (re-encrypting all items and
    /// re-sealing to vault members) and the Account Key, bumping the manifest epoch each step. A local
    /// `.gitpass-revoke-pending.json` is written first and removed last; an interrupted run is rolled
    /// forward on the next unlock (every step is idempotent).
    ///
    /// LIMITS: this protects FUTURE changes only. A device that already copied the repo / keys keeps what it
    /// had, and if it knew the master password + Secret Key it can derive the new AUK, so rotate those too
    /// ([`Session::rotate_credentials`]).
    pub fn revoke_device_and_rotate(&mut self, query: &str, password: &str, secret: &SecretKey) -> Result<RevokeReport> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        if id == self.device_id { bail!("refusing to revoke THIS device with key rotation (it would receive the new keys); run from another device or use --marker-only"); }
        let pending = self.root.join(REVOKE_PENDING_FILE);
        write_json(&pending, &RevokePending { device_id: id.clone() })?;
        crate::device::ensure_ignored(&self.root, REVOKE_PENDING_FILE)?;
        if !self.devices()?.get(&id).map(|d| d.revoked).unwrap_or(false) {
            let now = crate::device::now_iso8601();
            self.mutate_devices(|r| {
                let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
                d.revoked = true;
                d.revoked_at = Some(now);
                Ok(())
            })?;
        }
        let mut rotated = vec![];
        for vid in self.load_manifest()?.vaults.keys() { self.rotate_vault(vid)?; rotated.push(vid.clone()); }
        let backup = self.rotate_account_key(password, secret)?;
        fs::remove_file(&pending)?;
        fsutil::fsync_dir(&self.root);
        Ok(RevokeReport { device_id: id, vaults_rotated: rotated, account_key_backup: backup })
    }

    /// New master password AND new Secret Key in one keyset re-wrap. Returns the new Secret Key.
    pub fn rotate_credentials(&mut self, new_password: &str, kdf: KdfParams) -> Result<SecretKey> {
        self.ensure_not_revoked()?;
        let sk = SecretKey::generate();
        self.rewrap_keyset(new_password, &sk, kdf)?;
        Ok(sk)
    }

    /// This device's X25519 public key (v4 accounts only; created on demand).
    pub fn device_public_key(&self) -> Result<[u8; 32]> {
        Ok(crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?.0.public)
    }

    fn record_device_key(&self) -> Result<()> {
        let (kp, _) = crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?;
        let pk = B64.encode(kp.public);
        if self.devices()?.get(&self.device_id).and_then(|d| d.public_key.clone()) == Some(pk.clone()) { return Ok(()); }
        let id = self.device_id.clone();
        self.mutate_devices(|r| {
            if let Some(d) = r.devices.iter_mut().find(|d| d.id == id) { d.public_key = Some(pk); }
            Ok(())
        })
    }

    // ---------- identity + sharing (v4) ----------

    pub fn has_identity(&self) -> bool { self.root.join(IDENTITY_ENC_FILE).exists() }

    fn load_identity(&self) -> Result<crate::keys::KeyPair> {
        let ip: IdentityPub = read_json(&self.root.join(IDENTITY_PUB_FILE))?;
        if ip.account_id != self.fmt.account_id { bail!("identity.pub belongs to another account"); }
        let env: Envelope = read_json(&self.root.join(IDENTITY_ENC_FILE))?;
        let pt = zeroize::Zeroizing::new(open(&identity_key(&self.account_key), &env, &identity_aad(&ip.account_id, &ip.public))?);
        let b: [u8; 32] = pt[..].try_into().map_err(|_| anyhow!("identity key has wrong length"))?;
        let kp = crate::keys::KeyPair::from_secret_bytes(b);
        if B64.encode(kp.public) != ip.public { bail!("identity.pub does not match identity.enc"); }
        Ok(kp)
    }

    fn write_identity(&self, ak: &Key32, kp: &crate::keys::KeyPair) -> Result<IdentityEntry> {
        let public = B64.encode(kp.public);
        let ip = IdentityPub { format: 4, account_id: self.fmt.account_id.clone(), public: public.clone(), fingerprint: kp.fingerprint() };
        let enc_hash = write_json(&self.root.join(IDENTITY_ENC_FILE), &seal(&identity_key(ak), &kp.secret_bytes()[..], &identity_aad(&ip.account_id, &public))?)?;
        let pub_hash = write_json(&self.root.join(IDENTITY_PUB_FILE), &ip)?;
        Ok(IdentityEntry { pub_hash, enc_hash })
    }

    /// Create the account identity keypair if missing (upgrades the manifest to v4) and register this
    /// device's X25519 public key. Returns the identity keypair.
    pub fn ensure_identity(&self) -> Result<crate::keys::KeyPair> {
        if self.has_identity() { return self.load_identity(); }
        self.ensure_not_revoked()?;
        let kp = crate::keys::KeyPair::generate();
        self.commit(|m| { m.identity = Some(self.write_identity(&self.account_key, &kp)?); Ok(()) })?;
        self.record_device_key()?;
        Ok(kp)
    }

    /// (public key, fingerprint) of the identity if it exists (never creates one).
    pub fn identity_if_exists(&self) -> Result<Option<([u8; 32], String)>> {
        if !self.has_identity() { return Ok(None); }
        let kp = self.load_identity()?;
        Ok(Some((kp.public, kp.fingerprint())))
    }

    /// (public key, fingerprint) of this account's identity (created on first use).
    pub fn identity(&self) -> Result<([u8; 32], String)> {
        let kp = self.ensure_identity()?;
        Ok((kp.public, kp.fingerprint()))
    }

    fn write_member(dir: &Path, vid: &str, kv: u32, vk: &Key32, recipient: &[u8; 32], me: &crate::keys::KeyPair) -> Result<String> {
        let mut mf = MemberFile { format: 4, vault_id: vid.into(), key_version: kv, recipient_fpr: crate::keys::fingerprint(recipient),
            recipient_pub: B64.encode(recipient), sender_fpr: me.fingerprint(), sender_pub: B64.encode(me.public),
            sealed: crate::keys::SealedBox { alg: String::new(), eph: String::new(), envelope: Envelope { alg: String::new(), nonce: String::new(), ct: String::new() } },
            sender_sign_pub: String::new(), sender_sig: String::new(), sender_auth: String::new() };
        mf.sealed = crate::keys::seal_to(recipient, &vk.0, &member_aad(&mf))?;
        // Sign the finished share (sealed box included) with the sender's
        // share-signing key, derived from the identity secret.
        let secret = me.secret_bytes();
        let signing = crate::keys::signing_key_from_identity_secret(&secret);
        mf.sender_sign_pub = B64.encode(signing.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&signing, &share_sig_payload(&mf)));
        mf.sender_auth = B64.encode(crate::keys::share_auth_tag(me, recipient, &share_sig_payload(&mf))?);
        write_json(&dir.join("members").join(format!("{}.enc", mf.recipient_fpr)), &mf)
    }

    /// `gitpass vault share`: seal the current Vault Key to `recipient`. Returns the member file path.
    pub fn share_vault(&self, vid: &str, recipient: &[u8; 32]) -> Result<PathBuf> {
        self.ensure_not_revoked()?;
        let me = self.ensure_identity()?;
        if *recipient == me.public { bail!("refusing to share a vault with your own identity"); }
        let fpr = crate::keys::fingerprint(recipient);
        let vk = self.vault_key(vid)?;
        let d = self.vdir(vid);
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let h = Self::write_member(&d, vid, ve.key_version, &vk, recipient, &me)?;
            ve.members.insert(fpr.clone(), h);
            Ok(())
        })?;
        Ok(d.join("members").join(format!("{fpr}.enc")))
    }

    /// Members of a vault: (fingerprint, base64 public key).
    pub fn members(&self, vid: &str) -> Result<Vec<(String, String)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        ve.members.keys().map(|f| {
            let mf: MemberFile = read_json(&self.vdir(vid).join("members").join(format!("{f}.enc")))?;
            Ok((f.clone(), mf.recipient_pub))
        }).collect()
    }

    /// `gitpass vault unshare`: drop a member (fingerprint or unique prefix) and rotate the Vault Key so the
    /// member cannot read FUTURE changes. Returns the removed fingerprint.
    pub fn unshare_vault(&self, vid: &str, member: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let all: Vec<String> = self.members(vid)?.into_iter().map(|(f, _)| f).collect();
        let hits: Vec<&String> = all.iter().filter(|f| f.starts_with(member) && !member.is_empty()).collect();
        let fpr = match hits.len() { 0 => bail!("no member matches '{member}'"), 1 => hits[0].clone(), n => bail!("{n} members match '{member}'") };
        self.rotate_vault_excluding(vid, Some(&fpr))?;
        Ok(fpr)
    }

    /// `gitpass vault accept`: open a member file sealed to our identity and import a SNAPSHOT of that vault
    /// (meta + items, each authenticated under the shared Vault Key) into this account. The vault directory
    /// is taken as `<share>/../..` (i.e. `vaults/<id>/members/<fpr>.enc` in the sharer's checkout).
    /// Returns the imported vault id.
    pub fn accept_share(&self, share: &Path) -> Result<String> {
        self.ensure_not_revoked()?;
        let me = self.ensure_identity()?;
        let mf: MemberFile = read_json(share)?;
        if mf.format != 4 { bail!("unsupported member file format {}", mf.format); }
        if mf.recipient_fpr != me.fingerprint() || mf.recipient_pub != B64.encode(me.public) { bail!("this share is not addressed to your identity ({})", me.fingerprint()); }
        // Sender authentication: the sender's X25519 public key is self-asserted
        // in the file, so the sealed box alone proves nothing about who sent
        // this. Require a valid Ed25519 signature over the whole share.
        // Pre-signature shares (no signature fields) are rejected, not
        // grandfathered in — ask the sender to re-share with a current build.
        if mf.sender_sig.is_empty() || mf.sender_sign_pub.is_empty() || mf.sender_auth.is_empty() {
            bail!("share is unsigned (created before share signatures, or stripped) — refusing; ask the sender to re-share");
        }
        let sender_pub = crate::keys::parse_pub(&mf.sender_pub)?;
        if crate::keys::fingerprint(&sender_pub) != mf.sender_fpr {
            bail!("share sender fingerprint does not match sender public key — refusing");
        }
        let sign_pub: [u8; 32] = B64.decode(&mf.sender_sign_pub).context("bad sender signing key")?.try_into().map_err(|_| anyhow!("bad sender signing key length"))?;
        let sig: [u8; 64] = B64.decode(&mf.sender_sig).context("bad share signature")?.try_into().map_err(|_| anyhow!("bad share signature length"))?;
        crate::keys::verify_share_payload(&sign_pub, &share_sig_payload(&mf), &sig)
            .context("share signature invalid — wrong sender or tampered share")?;
        // Bind the share to sender_pub (the signing key above is self-asserted).
        let tag: [u8; 32] = B64.decode(&mf.sender_auth).context("bad sender binding")?.try_into().map_err(|_| anyhow!("bad sender binding length"))?;
        crate::keys::verify_share_auth_tag(&me, &sender_pub, &share_sig_payload(&mf), &tag)
            .context("share is not bound to the claimed sender identity — refusing")?;
        let pt = crate::keys::open_sealed(&me, &mf.sealed, &member_aad(&mf))?;
        let b: [u8; 32] = pt[..].try_into().map_err(|_| anyhow!("shared vault key has wrong length"))?;
        let vk = Key32(b);
        let vid = mf.vault_id.clone();
        uuid::Uuid::parse_str(&vid).context("shared vault id is not a uuid")?;
        let src = share.parent().and_then(|p| p.parent()).ok_or_else(|| anyhow!("share path must be vaults/<id>/members/<fpr>.enc"))?;
        if src.file_name().map(|n| n.to_string_lossy() != vid.as_str()).unwrap_or(true) { bail!("share file is not inside vaults/{vid}/members/"); }
        let menv: Envelope = read_json(&src.join("meta.enc"))?;
        let meta: VaultMeta = serde_json::from_slice(&zeroize::Zeroizing::new(open(&vk, &menv, &meta_aad(&vid)).context("shared vault meta failed authentication")?))?;
        if self.load_manifest()?.vaults.contains_key(&vid) { bail!("vault {vid} is already in this account"); }
        if self.list_vaults()?.iter().any(|(_, m)| m.name == meta.name) { bail!("a vault named '{}' already exists here; rename it first", meta.name); }
        let mut items = vec![];
        for iid in list_enc(&src.join("items")) {
            let p = src.join("items").join(format!("{iid}.enc"));
            let f: ItemFile = read_json(&p)?;
            open(&vk, &f.envelope, &item_aad(&vid, &iid, f.version)).with_context(|| format!("shared item {iid} failed authentication"))?;
            items.push((iid, f.version, fs::read(&p)?));
        }
        let mut files = vec![];
        for fid in list_enc(&src.join(FILES_DIR)) {
            let p = src.join(FILES_DIR).join(format!("{fid}.enc"));
            let b: FileBlob = read_json(&p)?;
            open(&vk, &b.envelope, &file_aad(&vid, &b.item_id, &fid)).with_context(|| format!("shared attachment {fid} failed authentication"))?;
            files.push((fid, fs::read(&p)?));
        }
        let _g = fsutil::lock(&self.root)?;
        let tmp = self.root.join("vaults").join(format!(".accept-{vid}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("items"))?;
        let mut ve = VaultEntry { key_version: mf.key_version, ..Default::default() };
        ve.key_hash = write_json(&tmp.join("vault.key.enc"), &VaultKeyFile { key_version: mf.key_version, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(&vid, mf.key_version))? })?;
        let mb = fs::read(src.join("meta.enc"))?;
        atomic_write(&tmp.join("meta.enc"), &mb)?;
        ve.meta_hash = hash_bytes(&mb);
        atomic_write(&tmp.join("items/.keep"), b"")?;
        for (iid, v, bytes) in &items {
            atomic_write(&tmp.join("items").join(format!("{iid}.enc")), bytes)?;
            ve.items.insert(iid.clone(), ItemEntry { version: *v, hash: hash_bytes(bytes) });
        }
        if !files.is_empty() { fs::create_dir_all(tmp.join(FILES_DIR))?; }
        for (fid, bytes) in &files {
            atomic_write(&tmp.join(FILES_DIR).join(format!("{fid}.enc")), bytes)?;
            ve.files.insert(fid.clone(), hash_bytes(bytes));
        }
        fsutil::rename(&tmp, &self.vdir(&vid))?;
        self.commit_locked(|m| { m.vaults.insert(vid.clone(), ve); Ok(()) })?;
        Ok(vid)
    }

    #[doc(hidden)]
    pub fn vault_key_bytes_for_tests(&self, vid: &str) -> Result<[u8; 32]> { Ok(self.vault_key(vid)?.0) }

    #[doc(hidden)]
    pub fn account_key_bytes_for_tests(&self) -> [u8; 32] { self.account_key.0 }
}


/// Targets without duplicates, first occurrence wins (keeps the caller's order).
fn dedup_targets(t: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    t.iter().filter(|x| seen.insert((*x).clone())).cloned().collect()
}

// ---------- format v5: typed items, soft delete, history, attachments ----------
impl Session {
    /// Add a v5 record (any category). Stamps legacy + v5 clocks and `created_at`; history starts empty.
    pub fn add_record(&self, vid: &str, mut rec: ItemRecord) -> Result<String> {
        let iid = uuid::Uuid::new_v4().to_string();
        let now = merge::now_ms();
        let now_sec = now / 1000;
        if rec.item.updated_at.is_empty() { rec.item.updated_at = now_sec.to_string(); }
        let up_sec = rec.item.updated_at.trim().parse::<u64>().unwrap_or(now_sec);
        rec.created_at.get_or_insert(up_sec);
        rec.clocks = Some(merge::stamp(None, &rec.item, now));
        merge::stamp_ext(None, &mut rec, now);
        rec.history.clear();
        self.put_item(vid, &iid, 1, &rec)?;
        Ok(iid)
    }

    /// Overwrite item `iid` with `new` (local edit): changed fields get fresh clocks, and if the content
    /// (item fields / category / sections / tags) changed, the previous content is pushed onto the
    /// encrypted history (last [`crate::item::HISTORY_LIMIT`] kept).
    pub fn update_record(&self, vid: &str, iid: &str, new: ItemRecord) -> Result<()> {
        let (v, rec) = self.prepare_update(vid, iid, new)?;
        self.put_item(vid, iid, v, &rec)
    }

    /// The `update_record` logic without writing: returns (new version, finished record).
    fn prepare_update(&self, vid: &str, iid: &str, mut new: ItemRecord) -> Result<(u64, ItemRecord)> {
        let (v, old) = self.read_record(vid, iid)?;
        let now = merge::now_ms();
        let mut cmp_new = new.snapshot(); cmp_new.item.updated_at.clear();
        let mut cmp_old = old.snapshot(); cmp_old.item.updated_at.clear();
        new.history = old.history.clone();
        if cmp_new != cmp_old || old.attachments != new.attachments {
            new.history.push(crate::item::HistoryEntry { version: v, updated_at: old.item.updated_at.clone(), snapshot: old.snapshot() });
            let n = new.history.len();
            if n > crate::item::HISTORY_LIMIT { new.history.drain(..n - crate::item::HISTORY_LIMIT); }
            new.item.updated_at = (now / 1000).to_string();
        } else {
            new.item.updated_at = old.item.updated_at.clone();
        }
        new.created_at = old.created_at.or(new.created_at);
        if new.removed_attachments.is_empty() { new.removed_attachments = old.removed_attachments.clone(); }
        if new.attachments.is_empty() {
            new.attachments = old.attachments.iter().filter(|a| !new.removed_attachments.contains(&a.id)).cloned().collect();
        }
        new.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &new.item, now));
        merge::stamp_ext(Some(&old), &mut new, now);
        Ok((v + 1, new))
    }

    /// Write several items (any vaults) under ONE manifest commit: one epoch bump / re-seal / git change.
    fn put_items(&self, writes: &[(String, String, u64, ItemRecord)]) -> Result<()> {
        if writes.is_empty() { return Ok(()); }
        self.ensure_not_revoked()?;
        let mut keys: BTreeMap<String, Key32> = BTreeMap::new();
        for (vid, ..) in writes { if !keys.contains_key(vid) { keys.insert(vid.clone(), self.vault_key(vid)?); } }
        self.commit(|m| {
            for (vid, iid, version, rec) in writes {
                let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
                let hash = Self::write_item_in(&self.vdir(vid), &keys[vid], vid, iid, *version, rec)?;
                ve.items.insert(iid.clone(), ItemEntry { version: *version, hash });
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Bulk edit: apply `f` to every (vault, item) in `targets` and write them all in one commit.
    /// Items whose record does not change are skipped. Returns how many were written.
    /// Fails before writing anything if any target cannot be read.
    pub fn mutate_records<F: FnMut(&mut ItemRecord)>(&self, targets: &[(String, String)], mut f: F) -> Result<usize> {
        let mut writes = Vec::with_capacity(targets.len());
        for (vid, iid) in dedup_targets(targets) {
            let (_, old) = self.read_record(&vid, &iid)?;
            let mut r = old.clone();
            f(&mut r);
            if serde_json::to_vec(&r)? == serde_json::to_vec(&old)? { continue; }
            let (v, rec) = self.prepare_update(&vid, &iid, r)?;
            writes.push((vid, iid, v, rec));
        }
        self.put_items(&writes)?;
        Ok(writes.len())
    }

    /// Permanently remove several items (and their attachment files) in one commit.
    pub fn remove_items(&self, targets: &[(String, String)]) -> Result<usize> {
        self.ensure_not_revoked()?;
        let targets = dedup_targets(targets);
        let mut plan = vec![];
        for (vid, iid) in &targets {
            let p = self.vdir(vid).join("items").join(format!("{iid}.enc"));
            if !p.exists() { bail!("no item {iid}"); }
            let fids: Vec<String> = self.read_record(vid, iid).map(|(_, r)| r.attachments.iter().map(|a| a.id.clone()).collect()).unwrap_or_default();
            plan.push((vid.clone(), iid.clone(), p, fids));
        }
        if plan.is_empty() { return Ok(0); }
        self.commit(|m| {
            for (vid, iid, p, fids) in &plan {
                fs::remove_file(p)?;
                if let Some(ve) = m.vaults.get_mut(vid) {
                    ve.items.remove(iid);
                    for f in fids {
                        let fp = self.vdir(vid).join(FILES_DIR).join(format!("{f}.enc"));
                        if fp.exists() { fs::remove_file(&fp)?; }
                        ve.files.remove(f);
                    }
                }
            }
            for (vid, ..) in &plan { fsutil::fsync_dir(&self.vdir(vid).join("items")); }
            Ok(())
        })?;
        Ok(plan.len())
    }

    /// Copy several items to `to_vid`: all records land in one commit; attachments (if any) are then
    /// re-encrypted per file. Returns the new ids in input order.
    pub fn copy_items_to(&self, targets: &[(String, String)], to_vid: &str) -> Result<Vec<String>> {
        self.vault_key(to_vid)?; // target must exist
        let now = merge::now_ms();
        let mut writes = vec![];
        let mut atts = vec![];
        for (vid, iid) in dedup_targets(targets) {
            let (_, r) = self.read_record(&vid, &iid)?;
            let mut n = ItemRecord::from_snapshot(&r.snapshot());
            n.favorite = r.favorite;
            n.ignored_findings = r.ignored_findings.clone();
            n.archived = r.archived;
            if n.item.updated_at.is_empty() { n.item.updated_at = (now / 1000).to_string(); }
            n.created_at.get_or_insert(now / 1000);
            n.clocks = Some(merge::stamp(None, &n.item, now));
            merge::stamp_ext(None, &mut n, now);
            n.history.clear();
            let nid = uuid::Uuid::new_v4().to_string();
            if !r.attachments.is_empty() { atts.push((vid.clone(), iid.clone(), nid.clone(), r.attachments.clone())); }
            writes.push((to_vid.to_string(), nid, 1, n));
        }
        self.put_items(&writes)?;
        for (vid, iid, nid, list) in atts {
            for a in &list {
                let bytes = zeroize::Zeroizing::new(self.read_attachment(&vid, &iid, &a.id)?);
                self.add_attachment(to_vid, &nid, &a.name, &a.mime, &bytes)?;
            }
        }
        Ok(writes.into_iter().map(|w| w.1).collect())
    }

    /// Move several items to `to_vid` (items already there are left alone): copy (one commit) then
    /// remove the originals (one commit). Returns (old vault, old id, new id) for each moved item.
    pub fn move_items(&self, targets: &[(String, String)], to_vid: &str) -> Result<Vec<(String, String, String)>> {
        let movers: Vec<(String, String)> = dedup_targets(targets).into_iter().filter(|(v, _)| v != to_vid).collect();
        if movers.is_empty() { return Ok(vec![]); }
        let nids = self.copy_items_to(&movers, to_vid)?;
        self.remove_items(&movers)?;
        Ok(movers.into_iter().zip(nids).map(|((v, i), n)| (v, i, n)).collect())
    }

    fn mutate_record<F: FnOnce(&mut ItemRecord)>(&self, vid: &str, iid: &str, f: F) -> Result<()> {
        let (_, mut r) = self.read_record(vid, iid)?;
        f(&mut r);
        self.update_record(vid, iid, r)
    }

    pub fn set_favorite(&self, vid: &str, iid: &str, fav: bool) -> Result<()> { self.mutate_record(vid, iid, |r| r.favorite = fav) }
    pub fn set_archived(&self, vid: &str, iid: &str, a: bool) -> Result<()> { self.mutate_record(vid, iid, |r| r.archived = a) }
    pub fn ignore_finding(&self, vid: &str, iid: &str, kind: &str) -> Result<()> { self.mutate_record(vid, iid, |r| { r.ignore_finding(kind); }) }
    pub fn unignore_finding(&self, vid: &str, iid: &str, kind: &str) -> Result<()> { self.mutate_record(vid, iid, |r| { r.unignore_finding(kind); }) }
    /// Soft delete: the item stays (encrypted) with `deleted_at` set and is purged after 30 days.
    pub fn soft_delete(&self, vid: &str, iid: &str) -> Result<()> { let t = now_secs(); self.mutate_record(vid, iid, |r| r.deleted_at = Some(t)) }
    pub fn restore(&self, vid: &str, iid: &str) -> Result<()> { self.mutate_record(vid, iid, |r| r.deleted_at = None) }
    /// Permanently remove an item and its attachment files.
    pub fn purge_item(&self, vid: &str, iid: &str) -> Result<()> { self.remove_item(vid, iid) }

    /// Purge every item soft-deleted more than 30 days before `now` (unix secs). Returns purged ids.
    pub fn purge_expired(&self, now: u64) -> Result<Vec<(String, String)>> {
        let m = self.load_manifest()?;
        let mut out = vec![];
        for (vid, ve) in &m.vaults {
            for iid in ve.items.keys() {
                let Ok((_, r)) = self.read_record(vid, iid) else { continue };
                if let Some(d) = r.deleted_at { if now.saturating_sub(d) >= crate::item::PURGE_AFTER_SECS { out.push((vid.clone(), iid.clone())); } }
            }
        }
        for (v, i) in &out { self.remove_item(v, i)?; }
        Ok(out)
    }

    /// All records of a vault (including archived / deleted ones).
    pub fn list_records(&self, vid: &str) -> Result<Vec<(String, u64, ItemRecord)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let mut out = vec![];
        for iid in ve.items.keys() { let (v, r) = self.read_record(vid, iid)?; out.push((iid.clone(), v, r)); }
        Ok(out)
    }

    /// A prior version's content (from history), or the current one if `version` is current.
    pub fn item_version(&self, vid: &str, iid: &str, version: u64) -> Result<ItemRecord> {
        let (v, r) = self.read_record(vid, iid)?;
        if v == version { return Ok(r); }
        let h = r.history.iter().find(|h| h.version == version).ok_or_else(|| anyhow!("no version {version} in item history"))?;
        let mut out = ItemRecord::from_snapshot(&h.snapshot);
        out.created_at = r.created_at;
        Ok(out)
    }

    /// Restore an item to a previous version from its history.
    /// Writes a new version whose content equals the chosen snapshot.
    /// History is append-only: the pre-restore state is pushed to history, and no versions are deleted.
    /// Errors cleanly for an unknown version or a deleted item.
    pub fn restore_version(&self, vid: &str, iid: &str, version: u64) -> Result<u64> {
        let (v, old) = self.read_record(vid, iid)?;
        if old.deleted_at.is_some() {
            bail!("cannot restore version of deleted item");
        }
        let h = old.history.iter().find(|h| h.version == version).ok_or_else(|| anyhow!("no version {version} in item history"))?;
        let snapshot = h.snapshot.clone();
        let now = merge::now_ms();
        let mut new = ItemRecord::from_snapshot(&snapshot);
        new.favorite = old.favorite;
        new.archived = old.archived;
        new.ignored_findings = old.ignored_findings.clone();
        new.attachments = old.attachments.clone();
        // Keep attachment tombstones so a merge with a stale copy can't resurrect a removed file.
        new.removed_attachments = old.removed_attachments.clone();
        new.created_at = old.created_at;
        new.history = old.history.clone();
        new.history.push(crate::item::HistoryEntry {
            version: v,
            updated_at: old.item.updated_at.clone(),
            snapshot: old.snapshot(),
        });
        let n = new.history.len();
        if n > crate::item::HISTORY_LIMIT {
            new.history.drain(..n - crate::item::HISTORY_LIMIT);
        }
        new.item.updated_at = (now / 1000).to_string();
        new.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &new.item, now));
        merge::stamp_ext(Some(&old), &mut new, now);
        let new_v = v + 1;
        self.put_item(vid, iid, new_v, &new)?;
        Ok(new_v)
    }

    /// Copy an item (content, tags, attachments) to a new id in `to_vid` (title unchanged).
    pub fn copy_item_to(&self, vid: &str, iid: &str, to_vid: &str) -> Result<String> {
        let (_, r) = self.read_record(vid, iid)?;
        let mut n = ItemRecord::from_snapshot(&r.snapshot());
        n.favorite = r.favorite;
        n.archived = r.archived;
        let nid = self.add_record(to_vid, n)?;
        for a in &r.attachments {
            let bytes = zeroize::Zeroizing::new(self.read_attachment(vid, iid, &a.id)?);
            self.add_attachment(to_vid, &nid, &a.name, &a.mime, &bytes)?;
        }
        Ok(nid)
    }

    /// Move an item to another vault (re-encrypted under the target Vault Key). Returns the new id.
    pub fn move_item(&self, vid: &str, iid: &str, to_vid: &str) -> Result<String> {
        if vid == to_vid { return Ok(iid.to_string()); }
        let nid = self.copy_item_to(vid, iid, to_vid)?;
        self.remove_item(vid, iid)?;
        Ok(nid)
    }

    /// Encrypt and store an attachment for item `iid` (size cap [`crate::item::MAX_ATTACHMENT_BYTES`]).
    /// The file is padded, sealed under the Vault Key with AAD binding vault/item/file id, and its hash
    /// is recorded in the manifest.
    pub fn add_attachment(&self, vid: &str, iid: &str, name: &str, mime: &str, data: &[u8]) -> Result<crate::item::AttachmentMeta> {
        self.ensure_not_revoked()?;
        if data.len() > crate::item::MAX_ATTACHMENT_BYTES {
            bail!("attachment too large ({} bytes exceeds {} MiB limit)", data.len(), crate::item::MAX_ATTACHMENT_BYTES / (1024 * 1024));
        }
        let (_, mut rec) = self.read_record(vid, iid)?;
        let vk = self.vault_key(vid)?;
        let fid = uuid::Uuid::new_v4().to_string();
        let mut padded = pad(data);
        let env = seal(&vk, &padded, &file_aad(vid, iid, &fid));
        padded.zeroize();
        let blob = FileBlob { format: 5, item_id: iid.into(), envelope: env? };
        let meta = crate::item::AttachmentMeta { id: fid.clone(), name: name.into(), size: data.len() as u64, mime: mime.into() };
        let d = self.vdir(vid);
        fs::create_dir_all(d.join(FILES_DIR))?;
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let h = write_json(&d.join(FILES_DIR).join(format!("{fid}.enc")), &blob)?;
            ve.files.insert(fid.clone(), h);
            Ok(())
        })?;
        rec.attachments.push(meta.clone());
        self.update_record(vid, iid, rec)?;
        Ok(meta)
    }

    /// Decrypt an attachment (checks the manifest hash and the AEAD binding).
    pub fn read_attachment(&self, vid: &str, iid: &str, fid: &str) -> Result<Vec<u8>> {
        let (_, rec) = self.read_record(vid, iid)?;
        if !rec.attachments.iter().any(|a| a.id == fid) { bail!("item has no attachment {fid}"); }
        let m = self.load_manifest()?;
        let want = m.vaults.get(vid).and_then(|v| v.files.get(fid)).ok_or_else(|| anyhow!("attachment {fid} not in manifest"))?;
        let p = self.vdir(vid).join(FILES_DIR).join(format!("{fid}.enc"));
        let bytes = fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
        if &hash_bytes(&bytes) != want { bail!("attachment {fid} does not match manifest hash (tampered?)"); }
        let b: FileBlob = serde_json::from_slice(&bytes)?;
        if b.item_id != iid { bail!("attachment {fid} belongs to another item"); }
        let vk = self.vault_key(vid)?;
        let padded = zeroize::Zeroizing::new(open(&vk, &b.envelope, &file_aad(vid, iid, fid)).context("attachment failed authentication")?);
        unpad(&padded)
    }

    /// Remove an attachment from item `iid`: deletes the encrypted blob from `files/<fid>.enc`,
    /// removes the file from the manifest, removes the attachment metadata, and records the prior state
    /// in the item's history.
    pub fn remove_attachment(&self, vid: &str, iid: &str, fid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let (v, old) = self.read_record(vid, iid)?;
        let idx = old.attachments.iter().position(|a| a.id == fid).ok_or_else(|| anyhow!("item has no attachment {fid}"))?;
        let now = merge::now_ms();
        let mut rec = old.clone();
        rec.attachments.remove(idx);
        if !rec.removed_attachments.iter().any(|x| x == fid) { rec.removed_attachments.push(fid.to_string()); }
        rec.history.push(crate::item::HistoryEntry {
            version: v,
            updated_at: old.item.updated_at.clone(),
            snapshot: old.snapshot(),
        });
        let n = rec.history.len();
        if n > crate::item::HISTORY_LIMIT { rec.history.drain(..n - crate::item::HISTORY_LIMIT); }
        rec.item.updated_at = (now / 1000).to_string();
        rec.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &rec.item, now));
        merge::stamp_ext(Some(&old), &mut rec, now);
        let vk = self.vault_key(vid)?;
        let d = self.vdir(vid);
        let fp = d.join(FILES_DIR).join(format!("{fid}.enc"));
        // One manifest commit: the new record (without the attachment, with its tombstone), the blob
        // deletion and the manifest `files` entry all land together, like `remove_item`.
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let hash = Self::write_item_in(&d, &vk, vid, iid, v + 1, &rec)?;
            ve.items.insert(iid.to_string(), ItemEntry { version: v + 1, hash });
            if fp.exists() {
                fs::remove_file(&fp)?;
                if let Some(parent) = fp.parent() { fsutil::fsync_dir(parent); }
            }
            ve.files.remove(fid);
            Ok(())
        })?;
        Ok(())
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}")
}

/// Overwrite this device's last-seen epoch. Only for `sync --merge`, which deliberately discards
/// unpushed local commits (after snapshotting their plaintext) and must accept the remote epoch, which
/// the caller has checked is >= the epoch at the merge base.
pub fn set_last_epoch(root: &Path, account_id: &str, epoch: u64) -> Result<()> { save_epoch(root, account_id, epoch) }

#[cfg(test)]
mod share_signature_tests {
    use super::*;
    use crate::crypto::KdfParams;

    fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

    fn unlock_fast(root: &Path, pw: &str, sk: &crate::crypto::SecretKey) -> Session {
        Session::unlock_with(root, pw, sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap()
    }

    struct Pair {
        _a: tempfile::TempDir,
        _b: tempfile::TempDir,
        sb: Session,
        vid: String,
        member: PathBuf,
    }

    fn setup() -> Pair {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ska = init(a.path(), "pw-a", fast()).unwrap();
        let skb = init(b.path(), "pw-b", fast()).unwrap();
        let sa = unlock_fast(a.path(), "pw-a", &ska);
        let sb = unlock_fast(b.path(), "pw-b", &skb);
        let vid = sa.create_vault_full("Shared", None, None, None).unwrap();
        let (rec_pub, _) = sb.identity().unwrap();
        let member = sa.share_vault(&vid, &rec_pub).unwrap();
        Pair { _a: a, _b: b, sb, vid, member }
    }

    fn read_member(p: &Path) -> MemberFile {
        serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
    }

    fn write_member(p: &Path, mf: &MemberFile) {
        fs::write(p, serde_json::to_vec_pretty(mf).unwrap()).unwrap();
    }

    #[test]
    fn signed_share_accepts_and_tamper_rejects() {
        let p = setup();
        // A genuine share carries a signature and accepts.
        let mf = read_member(&p.member);
        assert!(!mf.sender_sig.is_empty() && !mf.sender_sign_pub.is_empty());
        assert_eq!(p.sb.accept_share(&p.member).unwrap(), p.vid);
        assert!(p.sb.list_vaults().unwrap().iter().any(|(id, _)| id == &p.vid));
    }

    #[test]
    fn tampered_sealed_box_rejects() {
        let p = setup();
        let mut mf = read_member(&p.member);
        // Flip one character of the sealed vault-key ciphertext. The payload
        // covers it, so the signature must fail before the box is opened.
        let mut ct = mf.sealed.envelope.ct.clone();
        let first = ct.chars().next().unwrap();
        ct.replace_range(0..1, if first == 'A' { "B" } else { "A" });
        mf.sealed.envelope.ct = ct;
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn stripped_signature_rejects() {
        let p = setup();
        let mut mf = read_member(&p.member);
        mf.sender_sig.clear();
        mf.sender_sign_pub.clear();
        write_member(&p.member, &mf);
        let err = p.sb.accept_share(&p.member).unwrap_err().to_string();
        assert!(err.contains("unsigned"), "unexpected error: {err}");
    }

    #[test]
    fn forged_sender_rejects() {
        let p = setup();
        // Attacker re-labels the share with their own X25519 identity but
        // cannot re-sign it: the original signature covers the sender fields.
        let mut mf = read_member(&p.member);
        let attacker = crate::keys::KeyPair::generate();
        mf.sender_pub = B64.encode(attacker.public);
        mf.sender_fpr = attacker.fingerprint();
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn substituted_signing_key_rejects() {
        // Attacker keeps the victim's sender_pub/sender_fpr but signs with
        // their own Ed25519 key: the signature verifies, the binding must not.
        let p = setup();
        let mut mf = read_member(&p.member);
        let atk = crate::keys::signing_key_from_identity_secret(&crate::crypto::random_bytes::<32>());
        mf.sender_sign_pub = B64.encode(atk.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&atk, &share_sig_payload(&mf)));
        write_member(&p.member, &mf);
        let err = format!("{:#}", p.sb.accept_share(&p.member).unwrap_err());
        assert!(err.contains("not bound"), "unexpected error: {err}");
    }

    #[test]
    fn forged_sender_with_own_keys_rejects() {
        // Attacker seals a fresh vault key to the recipient, claims the
        // victim's identity, and signs + tags with their own keys.
        let p = setup();
        let mut mf = read_member(&p.member);
        let atk = crate::keys::KeyPair::generate();
        let rec = crate::keys::parse_pub(&mf.recipient_pub).unwrap();
        mf.sealed = crate::keys::seal_to(&rec, &crate::crypto::random_bytes::<32>(), &member_aad(&mf)).unwrap();
        let sk = crate::keys::signing_key_from_identity_secret(&atk.secret_bytes());
        mf.sender_sign_pub = B64.encode(sk.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&sk, &share_sig_payload(&mf)));
        mf.sender_auth = B64.encode(crate::keys::share_auth_tag(&atk, &rec, &share_sig_payload(&mf)).unwrap());
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn wrong_recipient_share_rejects() {
        // A share sealed to someone else is not addressed to us, even with a
        // valid signature — the recipient check still runs first.
        let p = setup();
        let c = tempfile::tempdir().unwrap();
        let skc = init(c.path(), "pw-c", fast()).unwrap();
        let sc = unlock_fast(c.path(), "pw-c", &skc);
        assert!(sc.accept_share(&p.member).is_err());
    }
}
