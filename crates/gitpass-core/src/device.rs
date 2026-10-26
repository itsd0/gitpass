//! Device registry (format v3): which devices have unlocked this account, and which are revoked.
//!
//! `devices.enc` (vault root) is an XChaCha20-Poly1305 envelope under
//! `hkdf32(account_key, None, "gitpass/devices/v3")` with AAD `"{account_id}|devices|3"`.
//! Its blake3 hash is bound into the authenticated manifest (`devices_hash`).
//! The local device id lives in git-ignored `.gitpass-device.json`.
//!
//! Format v4: each device has an X25519 keypair. The private key stays in the git-ignored
//! `.gitpass-device-key.json`, encrypted under a key derived from the AUK (master password + Secret Key);
//! the public key is recorded in devices.enc (registry format 4 once any device has a key).
//! `Session::revoke_device_and_rotate` rotates the Account Key and every Vault Key so a revoked device
//! cannot read FUTURE changes. It cannot claw back data/keys the device already copied, and if the device
//! knew the master password + Secret Key those must be rotated too (`--rotate-credentials`).
use crate::crypto::{hkdf32, open, seal, Envelope, Key32};
use crate::store::{read_json, write_json};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Highest registry format understood.
pub const DEVICES_FORMAT: u32 = 4;
/// Registry format written while no device has a public key (keeps v3 readers happy).
pub const DEVICES_FORMAT_V3: u32 = 3;
pub const LOCAL_DEVICE_KEY_FILE: &str = ".gitpass-device-key.json";
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
    /// v4: device X25519 public key (base64). Absent for v3 devices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DeviceRegistry {
    pub format: u32,
    pub devices: Vec<Device>,
}
impl Default for DeviceRegistry {
    fn default() -> Self { DeviceRegistry { format: DEVICES_FORMAT_V3, devices: vec![] } }
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
            Some(d) => { if !d.revoked {
                d.last_seen_at = now.into();
                // relabel records registered before a real hostname was available
                if (d.name.trim().is_empty() || d.name == UNKNOWN_DEVICE) && !name.trim().is_empty() { d.name = name.trim().into(); }
            } }
            None => self.devices.push(Device { id: id.into(), name: name.into(), created_at: now.into(),
                last_seen_at: now.into(), revoked: false, revoked_at: None, public_key: None }),
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
    let mut r = r.clone();
    r.format = if r.devices.iter().any(|d| d.public_key.is_some()) { DEVICES_FORMAT } else { r.format.min(DEVICES_FORMAT_V3) };
    write_json(&root.join(DEVICES_FILE), &seal_registry(account_key, account_id, &r)?)
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
fn ensure_gitignored(root: &Path) -> Result<()> { ensure_ignored(root, LOCAL_DEVICE_FILE) }
/// Append `name` to <root>/.gitignore if it is not listed yet.
pub fn ensure_ignored(root: &Path, name: &str) -> Result<()> {
    let p = root.join(".gitignore");
    let cur = std::fs::read_to_string(&p).unwrap_or_default();
    if !cur.lines().any(|l| l.trim() == name) {
        let mut n = cur;
        if !n.is_empty() && !n.ends_with('\n') { n.push('\n'); }
        n.push_str(name); n.push('\n');
        crate::fsutil::atomic_write(&p, n.as_bytes())?;
    }
    Ok(())
}

// ---------- local device keypair (v4) ----------
#[derive(Serialize, Deserialize)]
struct LocalDeviceKey { format: u32, device_id: String, public: String, envelope: Envelope }
fn device_key_aad(device_id: &str, public: &str) -> Vec<u8> { format!("gitpass/device-key/v4|{device_id}|{public}").into_bytes() }

/// Load this device's X25519 keypair, or create + store one. `kek` is derived from the AUK, so the file is
/// useless without the master password + Secret Key. If it cannot be decrypted (e.g. credentials changed
/// on another device) a fresh keypair is generated; the returned bool is true when a new key was made.
pub fn load_or_create_device_key(root: &Path, device_id: &str, kek: &Key32) -> Result<(crate::keys::KeyPair, bool)> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let p = root.join(LOCAL_DEVICE_KEY_FILE);
    if let Ok(l) = read_json::<LocalDeviceKey>(&p) {
        if l.device_id == device_id {
            if let Ok(pt) = open(kek, &l.envelope, &device_key_aad(device_id, &l.public)) {
                if let Ok(b) = <[u8; 32]>::try_from(&pt[..]) {
                    let kp = crate::keys::KeyPair::from_secret_bytes(b);
                    if B64.encode(kp.public) == l.public { return Ok((kp, false)); }
                }
            }
        }
    }
    let kp = crate::keys::KeyPair::generate();
    store_device_key(root, device_id, kek, &kp)?;
    Ok((kp, true))
}
/// (Re)write the local device key file under `kek` (atomic).
pub fn store_device_key(root: &Path, device_id: &str, kek: &Key32, kp: &crate::keys::KeyPair) -> Result<()> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let public = B64.encode(kp.public);
    let env = seal(kek, &kp.secret_bytes()[..], &device_key_aad(device_id, &public))?;
    write_json(&root.join(LOCAL_DEVICE_KEY_FILE), &LocalDeviceKey { format: 4, device_id: device_id.into(), public, envelope: env })?;
    ensure_ignored(root, LOCAL_DEVICE_KEY_FILE)
}

pub const UNKNOWN_DEVICE: &str = "unknown-device";

/// Default device name: $GITPASS_DEVICE_NAME, else hostname, else "unknown-device".
pub fn default_device_name() -> String {
    if let Ok(n) = std::env::var("GITPASS_DEVICE_NAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    if let Ok(n) = std::env::var("HOSTNAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    for f in ["/etc/hostname", "/proc/sys/kernel/hostname"] {
        if let Ok(n) = std::fs::read_to_string(f) { if !n.trim().is_empty() { return n.trim().into(); } }
    }
    if let Ok(n) = std::env::var("COMPUTERNAME") { if !n.trim().is_empty() { return n.trim().into(); } }
    // macOS has no /etc/hostname and GUI apps get no $HOSTNAME: ask the OS.
    #[cfg(target_os = "macos")]
    let cmds: &[(&str, &[&str])] = &[("scutil", &["--get", "ComputerName"]), ("hostname", &["-s"])];
    #[cfg(not(target_os = "macos"))]
    let cmds: &[(&str, &[&str])] = &[("hostname", &[])];
    for (c, a) in cmds {
        if let Ok(o) = std::process::Command::new(c).args(*a).output() {
            let n = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if o.status.success() && !n.is_empty() { return n; }
        }
    }
    UNKNOWN_DEVICE.into()
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
