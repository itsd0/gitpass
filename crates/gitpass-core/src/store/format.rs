//! On-disk vault format types, AAD helpers, padding and JSON helpers.
//! Pure move from the former `store.rs`; no behaviour or format change.

use super::sharing::IdentityEntry;
use crate::crypto::*;
use crate::fsutil::atomic_write;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use zeroize::{Zeroize, ZeroizeOnDrop};

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
    /// Generator history (format v1). Absent when no history recorded yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator_history_hash: Option<String>,
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
pub(crate) fn manifest_aad(account_id: &str) -> Vec<u8> { format!("{account_id}|manifest|2").into_bytes() }
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
