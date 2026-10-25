//! Device registry (format v3): which devices have unlocked this account, and which are revoked.
//!
//! `devices.enc` (vault root) is an XChaCha20-Poly1305 envelope under
//! `hkdf32(account_key, None, "gitpass/devices/v3")` with AAD `"{account_id}|devices|3"`.
//! Its blake3 hash is bound into the authenticated manifest (`devices_hash`).
//! The local device id lives in git-ignored `.gitpass-device.json`.
//!
//! LIMITATION: revocation is a policy marker only. It does NOT re-encrypt anything; a revoked device
//! that still has the master password + Secret Key (or a copy of the Account Key) can decrypt data.
//! After revoking a compromised device run `gitpass rotate-account-key --with-vault-keys` and change the
//! master password / Secret Key.
use crate::crypto::{hkdf32, open, seal, Envelope, Key32};
use crate::store::{read_json, write_json};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEVICES_FORMAT: u32 = 3;
pub const DEVICES_FILE: &str = "devices.enc";
pub const LOCAL_DEVICE_FILE: &str = ".gitpass-device.json";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_seen_at: String,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DeviceRegistry {
    pub format: u32,
    pub devices: Vec<Device>,
}
impl Default for DeviceRegistry {
    fn default() -> Self { DeviceRegistry { format: DEVICES_FORMAT, devices: vec![] } }
}

impl DeviceRegistry {
    pub fn get(&self, id: &str) -> Option<&Device> { self.devices.iter().find(|d| d.id == id) }
    /// Resolve a full id or unique id prefix.
    pub fn resolve(&self, q: &str) -> Result<String> {
        if q.is_empty() { bail!("empty device id"); }
        if let Some(d) = self.get(q) { return Ok(d.id.clone()); }
        let m: Vec<&Device> = self.devices.iter().filter(|d| d.id.starts_with(q)).collect();
        match m.len() {
            0 => bail!("no device matches '{q}'"),
            1 => Ok(m[0].id.clone()),
            n => bail!("{n} devices match '{q}'; use a longer prefix"),
        }
    }
    /// Register `id` or refresh its last_seen_at (revoked devices are left untouched).
    pub fn touch(&mut self, id: &str, name: &str, now: &str) {
        match self.devices.iter_mut().find(|d| d.id == id) {
            Some(d) => { if !d.revoked { d.last_seen_at = now.into(); } }
            None => self.devices.push(Device { id: id.into(), name: name.into(), created_at: now.into(),
                last_seen_at: now.into(), revoked: false, revoked_at: None }),
        }
    }
}

pub fn devices_key(account_key: &Key32) -> Key32 { hkdf32(&account_key.0, None, b"gitpass/devices/v3") }
pub fn devices_aad(account_id: &str) -> Vec<u8> { format!("{account_id}|devices|3").into_bytes() }

pub fn seal_registry(account_key: &Key32, account_id: &str, r: &DeviceRegistry) -> Result<Envelope> {
    let pt = zeroize::Zeroizing::new(serde_json::to_vec(r)?);
    seal(&devices_key(account_key), &pt, &devices_aad(account_id))
}
pub fn open_registry(account_key: &Key32, account_id: &str, env: &Envelope) -> Result<DeviceRegistry> {
    let pt = zeroize::Zeroizing::new(open(&devices_key(account_key), env, &devices_aad(account_id))?);
    let r: DeviceRegistry = serde_json::from_slice(&pt)?;
    if r.format > DEVICES_FORMAT { bail!("devices registry format {} is newer than supported", r.format); }
    Ok(r)
}
/// Seal + atomic write of devices.enc; returns blake3 of the bytes written.
pub fn write_registry(root: &Path, account_key: &Key32, account_id: &str, r: &DeviceRegistry) -> Result<String> {
    write_json(&root.join(DEVICES_FILE), &seal_registry(account_key, account_id, r)?)
}
/// Read devices.enc; Ok(None) if it does not exist (pre-v3 vault).
pub fn read_registry(root: &Path, account_key: &Key32, account_id: &str) -> Result<Option<DeviceRegistry>> {
    let p = root.join(DEVICES_FILE);
    if !p.exists() { return Ok(None); }
    let env: Envelope = read_json(&p)?;
    Ok(Some(open_registry(account_key, account_id, &env)?))
}

// ---------- local device identity ----------
#[derive(Serialize, Deserialize)]
struct LocalDevice { id: String }
fn local_path(root: &Path) -> PathBuf { root.join(LOCAL_DEVICE_FILE) }

/// Stable per-checkout device id (created on first use, git-ignored).
pub fn local_device_id(root: &Path) -> Result<String> {
    if let Ok(l) = read_json::<LocalDevice>(&local_path(root)) {
        if uuid::Uuid::parse_str(&l.id).is_ok() { return Ok(l.id); }
    }
    let id = uuid::Uuid::new_v4().to_string();
    write_json(&local_path(root), &LocalDevice { id: id.clone() })?;
    ensure_gitignored(root)?;
    Ok(id)
}
fn ensure_gitignored(root: &Path) -> Result<()> {
    let p = root.join(".gitignore");
    let cur = std::fs::read_to_string(&p).unwrap_or_default();
    if !cur.lines().any(|l| l.trim() == LOCAL_DEVICE_FILE) {
        let mut n = cur;
        if !n.is_empty() && !n.ends_with('\n') { n.push('\n'); }
        n.push_str(LOCAL_DEVICE_FILE); n.push('\n');
        crate::fsutil::atomic_write(&p, n.as_bytes())?;
    }
    Ok(())
}

/// Default device name: $GITPASS_DEVICE_NAME, else hostname, else "unknown-device".
pub fn default_device_name() -> String {
    if let Ok(n) = std::env::var("GITPASS_DEVICE_NAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    if let Ok(n) = std::env::var("HOSTNAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    for f in ["/etc/hostname", "/proc/sys/kernel/hostname"] {
        if let Ok(n) = std::fs::read_to_string(f) { if !n.trim().is_empty() { return n.trim().into(); } }
    }
    if let Ok(n) = std::env::var("COMPUTERNAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    "unknown-device".into()
}

/// Current UTC time as ISO 8601 (`YYYY-MM-DDTHH:MM:SSZ`).
pub fn now_iso8601() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    iso8601(s)
}
pub fn iso8601(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
