//! `Session`: unlock, vault/item operations and device/sharing state.
//! Pure move from the former `store.rs`: the struct, account-level
//! functions and unlock helpers live here; the method implementations
//! live in the sibling files of this directory. No behaviour, wire
//! format, AAD or crypto change.

use crate::crypto::*;
use crate::fsutil::{self, atomic_write};
use crate::store::{format::*, local_state::*, sharing::*};
use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

mod devices;
mod items;
mod keys;
mod records;
mod unlock;
mod vaults;

pub(crate) fn seal_manifest(root: &Path, ak: &Key32, m: &Manifest) -> Result<()> {
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
    let m = Manifest { format: INIT_FORMAT_VERSION, account_id: account_id.clone(), epoch: 1, keyset_hash, vaults: BTreeMap::new(), devices_hash: None, identity: None, generator_history_hash: None };
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
    match &m.generator_history_hash {
        Some(h) => check(&mut p, &root.join(crate::gen_history::GEN_HISTORY_FILE), h),
        None => if root.join(crate::gen_history::GEN_HISTORY_FILE).exists() {
            p.push("unexpected generator_history.enc (not in manifest)".into());
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

pub(crate) fn list_enc(d: &Path) -> Vec<String> {
    fs::read_dir(d).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("enc"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().to_string()).collect()).unwrap_or_default()
}

#[derive(Default, Clone, Debug)]
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

pub(crate) fn open_keyset(root: &Path, fmt: &Format, ks: &Path, password: &str, secret: &SecretKey) -> Result<Key32> {
    Ok(open_keyset_auk(root, fmt, ks, password, secret)?.1)
}
pub(crate) fn open_keyset_auk(root: &Path, fmt: &Format, ks: &Path, password: &str, secret: &SecretKey) -> Result<(Key32, Key32)> {
    let salt = B64.decode(&fmt.salt)?;
    let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
    let env: Envelope = read_json(ks)?;
    let _ = root;
    let ak = unwrap_key(&auk, &env, &keyset_aad(fmt))
        .map_err(|_| anyhow!("unlock failed: wrong master password or Secret Key (or format.json/keyset.enc tampered)"))?;
    Ok((auk, ak))
}

/// Roll forward an interrupted keyset re-wrap (format.json.new / keyset.enc.new). Returns true if completed.
pub(crate) fn finish_pending_rewrap(root: &Path, password: &str, secret: &SecretKey) -> Result<bool> {
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
