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
pub const FORMAT_VERSION: u32 = 3;
/// Oldest on-disk format this build still reads (v2 is upgraded to v3 lazily, on next write).
pub const MIN_READ_FORMAT: u32 = 2;
/// Manifest format written by merge/device features (same value as FORMAT_VERSION). format.json stays at 2
/// until the keyset is re-wrapped, so keyset AAD and existing Emergency Kits stay valid.
pub const MANIFEST_FORMAT_V3: u32 = FORMAT_VERSION;
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
}

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
    Ok(serde_json::from_slice(&b).with_context(|| format!("parsing {}", p.display()))?)
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
    let m = Manifest { format: INIT_FORMAT_VERSION, account_id: account_id.clone(), epoch: 1, keyset_hash, vaults: BTreeMap::new(), devices_hash: None };
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
    }
    p
}

pub struct UnlockOpts {
    /// Accept integrity/rollback failures and re-seal the manifest from what's on disk.
    pub accept_rollback: bool,
    /// KDF target; weaker stored params are auto-upgraded on unlock.
    pub target_kdf: KdfParams,
}
impl Default for UnlockOpts {
    fn default() -> Self { UnlockOpts { accept_rollback: false, target_kdf: KdfParams::default() } }
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
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("root", &self.root).field("account_key", &"<redacted>").finish()
    }
}

fn open_keyset(root: &Path, fmt: &Format, ks: &Path, password: &str, secret: &SecretKey) -> Result<Key32> {
    let salt = B64.decode(&fmt.salt)?;
    let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
    let env: Envelope = read_json(ks)?;
    let _ = root;
    unwrap_key(&auk, &env, &keyset_aad(fmt))
        .map_err(|_| anyhow!("unlock failed: wrong master password or Secret Key (or format.json/keyset.enc tampered)"))
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
        let recovered = {
            let _g = fsutil::lock(root)?;
            finish_pending_rewrap(root, password, secret)?
        };
        let fmt = read_format(root)?;
        if fmt.version < MIN_READ_FORMAT { bail!("vault is format v{}; run `gitpass migrate` to upgrade to v{FORMAT_VERSION}", fmt.version); }
        let account_key = open_keyset(root, &fmt, &root.join("keyset.enc"), password, secret)?;
        let device_id = crate::device::local_device_id(root)?;
        let mut s = Session { root: root.to_path_buf(), fmt, account_key, kdf_upgraded: false, accepted_problems: vec![], device_id, device_revoked: false };
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
        let name = device_name.map(str::to_string).unwrap_or_else(crate::device::default_device_name);
        s.touch_device(&name)?;
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
        m.format = FORMAT_VERSION; // any write upgrades the manifest to v3 (clocks and/or devices_hash)
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
            keyset_hash: hash_file(&self.root.join("keyset.enc"))?, vaults: BTreeMap::new(), devices_hash: None };
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
                    meta_hash: hash_file(&d.join("meta.enc"))?, items: BTreeMap::new() };
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
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)
    }

    fn vdir(&self, id: &str) -> PathBuf { self.root.join("vaults").join(id) }

    pub fn create_vault(&self, name: &str) -> Result<String> { self.create_vault_with(name, None) }

    pub fn create_vault_with(&self, name: &str, description: Option<String>) -> Result<String> {
        self.ensure_not_revoked()?;
        if self.list_vaults()?.iter().any(|(_, m)| m.name == name) { bail!("vault '{name}' already exists"); }
        let id = uuid::Uuid::new_v4().to_string();
        self.commit(|m| {
            let vk = Key32::random();
            let d = self.vdir(&id);
            let key_hash = write_json(&d.join("vault.key.enc"),
                &VaultKeyFile { key_version: 1, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(&id, 1))? })?;
            let meta = VaultMeta { name: name.into(), description };
            let mut pt = serde_json::to_vec(&meta)?;
            let env = seal(&vk, &pt, &meta_aad(&id));
            pt.zeroize();
            let meta_hash = write_json(&d.join("meta.enc"), &env?)?;
            fs::create_dir_all(d.join("items"))?;
            atomic_write(&d.join("items/.keep"), b"")?;
            m.vaults.insert(id.clone(), VaultEntry { key_version: 1, key_hash, meta_hash, items: BTreeMap::new() });
            Ok(())
        })?;
        Ok(id)
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
        let iid = uuid::Uuid::new_v4().to_string();
        let rec = ItemRecord { item: item.clone(), clocks: Some(merge::stamp(None, item, merge::now_ms())) };
        self.put_item(vid, &iid, 1, &rec)?;
        Ok(iid)
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
        let (v, old) = self.read_record(vid, iid)?;
        let clocks = merge::stamp(Some((&old.item, old.clocks_or_legacy())), item, merge::now_ms());
        self.put_item(vid, iid, v + 1, &ItemRecord { item: item.clone(), clocks: Some(clocks) })
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
        self.commit(|m| {
            fs::remove_file(&p)?;
            fsutil::fsync_dir(p.parent().unwrap());
            if let Some(ve) = m.vaults.get_mut(vid) { ve.items.remove(iid); }
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
            m.format = MANIFEST_FORMAT_V3;
        }
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
    pub fn rotate_vault(&self, vid: &str) -> Result<()> {
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
        for (iid, v, it) in &items {
            let h = Self::write_item_in(&tmp, &vk, vid, iid, v + 1, it)?;
            ne.items.insert(iid.clone(), ItemEntry { version: v + 1, hash: h });
        }
        let old = vaults.join(format!(".old-{vid}"));
        let _ = fs::remove_dir_all(&old);
        fsutil::rename(&self.vdir(vid), &old)?;
        fsutil::rename(&tmp, &self.vdir(vid))?;
        m.vaults.insert(vid.to_string(), ne);
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
        self.mutate_devices(|r| { r.touch(&id, name, &now); Ok(()) })
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

    #[doc(hidden)]
    pub fn account_key_bytes_for_tests(&self) -> [u8; 32] { self.account_key.0 }
}

pub fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}")
}

/// Overwrite this device's last-seen epoch. Only for `sync --merge`, which deliberately discards
/// unpushed local commits (after snapshotting their plaintext) and must accept the remote epoch, which
/// the caller has checked is >= the epoch at the merge base.
pub fn set_last_epoch(root: &Path, account_id: &str, epoch: u64) -> Result<()> { save_epoch(root, account_id, epoch) }
