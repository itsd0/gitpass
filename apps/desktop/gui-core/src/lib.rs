//! gitpass-gui-core: the command layer behind the desktop GUI, independent of Tauri so it can be
//! compiled and unit-tested on any toolchain that builds gitpass-core.
//!
//! Security model:
//! * The unlocked `Session` lives only here (Rust process memory), never in the frontend.
//! * The master password and Secret Key are kept (zeroizing) while unlocked because `sync --merge`
//!   needs them to re-unlock; they are wiped on `lock()` / auto-lock / drop.
//! * List/detail DTOs never carry the password or TOTP secret; secrets are only returned by the
//!   explicit `reveal_field` / `field_for_copy` calls.
use anyhow::{anyhow, bail, Result};
use gitpass_core::crypto::{generate_password, SecretKey};
use gitpass_core::store::{Item, Session, UnlockOpts};
use gitpass_core::{search, totp};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use zeroize::Zeroizing;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CLIPBOARD_CLEAR_SECS: u64 = 30;

/// Default vault folder: ~/.gitpass
pub fn default_vault_dir() -> PathBuf {
    dirs::home_dir().map(|h| h.join(".gitpass")).unwrap_or_else(|| PathBuf::from(".gitpass"))
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct VaultDto { pub id: String, pub name: String, pub description: Option<String> }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ItemSummary { pub id: String, pub title: String, pub username: Option<String>, pub url: Option<String>, pub has_password: bool, pub has_totp: bool, pub score: Option<i64> }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ItemDetail { pub id: String, pub version: u64, pub title: String, pub username: Option<String>, pub url: Option<String>, pub notes: Option<String>, pub has_password: bool, pub has_totp: bool, pub updated_at: String }

/// Input for add/edit. `password: None` on edit = keep existing; `Some("")` = clear.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct ItemInput { pub title: String, pub username: Option<String>, pub password: Option<String>, pub url: Option<String>, pub notes: Option<String>, pub totp: Option<String> }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct TotpDto { pub code: String, pub remaining: u64, pub period: u64 }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DeviceDto { pub id: String, pub name: String, pub created_at: String, pub last_seen_at: String, pub revoked: bool, pub current: bool }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct StatusDto { pub unlocked: bool, pub vault_dir: Option<String>, pub device_id: Option<String>, pub device_revoked: bool, pub auto_lock_minutes: u64, pub warnings: Vec<String> }

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SyncDto { pub log: String, pub merged: Vec<String>, pub reapplied: Vec<String>, pub epoch: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Field { Username, Password, Totp, Url }

pub(crate) struct Unlocked {
    session: Session,
    password: Zeroizing<String>,
    secret: SecretKey, // ZeroizeOnDrop
    device_name: Option<String>,
    opts_accept_rollback: bool,
}

pub(crate) struct Inner { unlocked: Option<Unlocked>, last_activity: Instant, last_activity_wall: SystemTime, auto_lock: Duration,
    /// Run when the session is dropped (manual lock, idle auto-lock, sleep/screen-lock hooks).
    lock_hooks: Vec<Arc<dyn Fn() + Send + Sync>> }

/// Shared app state (wrap in Tauri `State`).
pub struct GuiState { pub(crate) inner: Mutex<Inner> }

impl Default for GuiState { fn default() -> Self { Self::new(5) } }

/// Parse a Secret Key from pasted text or the contents of a `secret.key` file.
pub fn parse_secret_key(text: &str) -> Result<SecretKey> {
    let t = Zeroizing::new(text.trim().to_string());
    SecretKey::parse(&t)
}
pub fn read_secret_key_file(p: &Path) -> Result<SecretKey> {
    let s = Zeroizing::new(std::fs::read_to_string(p).map_err(|e| anyhow!("cannot read {}: {e}", p.display()))?);
    parse_secret_key(&s)
}

fn summary(id: String, it: &Item, score: Option<i64>) -> ItemSummary {
    ItemSummary { id, title: it.title.clone(), username: it.username.clone(), url: it.url.clone(),
        has_password: it.password.as_deref().is_some_and(|p| !p.is_empty()), has_totp: it.totp.is_some(), score }
}
fn nonempty(o: Option<String>) -> Option<String> { o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) }

impl GuiState {
    pub fn new(auto_lock_minutes: u64) -> Self {
        GuiState { inner: Mutex::new(Inner { unlocked: None, last_activity: Instant::now(), last_activity_wall: SystemTime::now(), auto_lock: Duration::from_secs(auto_lock_minutes * 60), lock_hooks: Vec::new() }) }
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
        let mut g = self.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        Self::expire(&mut g);
        g.last_activity = Instant::now();
        g.last_activity_wall = SystemTime::now();
        let u = g.unlocked.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        f(u)
    }
    /// Like `with`, but does NOT count as user activity (SSH agent / background reads must not keep the
    /// vault unlocked forever).
    pub(crate) fn peek<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> {
        let mut g = self.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        Self::expire(&mut g);
        let u = g.unlocked.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        f(u)
    }
    fn expire(g: &mut Inner) -> bool {
        let idle_by_instant = !g.auto_lock.is_zero() && g.last_activity.elapsed() >= g.auto_lock;
        let idle_by_wall = !g.auto_lock.is_zero() && SystemTime::now().duration_since(g.last_activity_wall).map(|d| d >= g.auto_lock).unwrap_or(false);
        if g.unlocked.is_some() && (idle_by_instant || idle_by_wall) {
            g.unlocked = None;
            for h in &g.lock_hooks { h(); }
            return true;
        }
        false
    }
    pub(crate) fn writable(u: &Unlocked) -> Result<()> { if u.session.device_revoked { bail!("this device is revoked; edits are refused") } Ok(()) }

    pub fn unlock(&self, dir: &Path, password: &str, secret: SecretKey, device_name: Option<String>, accept_rollback: bool) -> Result<StatusDto> {
        if password.is_empty() { bail!("master password is empty"); }
        if !dir.join("format.json").exists() { bail!("{} is not a gitpass vault (no format.json)", dir.display()); }
        let opts = UnlockOpts { accept_rollback, ..Default::default() };
        self.unlock_with_opts(dir, password, secret, device_name, &opts)
    }

    /// Unlock via biometric authentication, resetting failure throttling on success.
    pub fn unlock_biometric_with_opts(&self, dir: &Path, password: &str, secret: SecretKey, device_name: Option<String>, opts: &UnlockOpts) -> Result<StatusDto> {
        let session = Session::unlock_device_biometric(dir, password, &secret, opts, device_name.as_deref())?;
        let mut g = self.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        g.unlocked = Some(Unlocked { session, password: Zeroizing::new(password.to_string()), secret, device_name, opts_accept_rollback: opts.accept_rollback });
        g.last_activity = Instant::now();
        g.last_activity_wall = SystemTime::now();
        drop(g);
        self.status()
    }

    /// Like `unlock` but with explicit options (tests use weak KDF targets to stay fast).
    pub fn unlock_with_opts(&self, dir: &Path, password: &str, secret: SecretKey, device_name: Option<String>, opts: &UnlockOpts) -> Result<StatusDto> {
        let session = Session::unlock_device(dir, password, &secret, opts, device_name.as_deref())?;
        let mut g = self.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        g.unlocked = Some(Unlocked { session, password: Zeroizing::new(password.to_string()), secret, device_name, opts_accept_rollback: opts.accept_rollback });
        g.last_activity = Instant::now();
        g.last_activity_wall = SystemTime::now();
        drop(g);
        self.status()
    }

    /// Drop the session; password, Secret Key and account key are zeroized on drop.
    pub fn lock(&self) {
        if let Ok(mut g) = self.inner.lock() {
            if g.unlocked.take().is_some() { for h in &g.lock_hooks { h(); } }
        }
    }
    /// Register a callback that runs whenever the session is dropped.
    pub(crate) fn on_lock(&self, h: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut g) = self.inner.lock() { g.lock_hooks.push(h); }
    }
    /// Cheap change stamp for live sync: a hash of the sealed manifest, which every write (GUI, extension
    /// via the agent, CLI, git sync) re-seals with a bumped epoch. `None` while locked. Not user activity.
    pub fn change_stamp(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let root = { let g = self.inner.lock().ok()?; g.unlocked.as_ref()?.session.root.clone() };
        let b = std::fs::read(root.join("manifest.enc")).ok()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        b.hash(&mut h);
        Some(h.finish())
    }
    pub fn is_unlocked(&self) -> bool { self.inner.lock().map(|g| g.unlocked.is_some()).unwrap_or(false) }
    /// Record user activity (resets the idle timer).
    pub fn touch(&self) { if let Ok(mut g) = self.inner.lock() { if !Self::expire(&mut g) { g.last_activity = Instant::now(); g.last_activity_wall = SystemTime::now(); } } }
    /// Lock if idle too long; returns true if it locked now. Call periodically.
    pub fn check_idle(&self) -> bool { self.inner.lock().map(|mut g| Self::expire(&mut g)).unwrap_or(false) }
    pub fn set_auto_lock_minutes(&self, m: u64) { if let Ok(mut g) = self.inner.lock() { g.auto_lock = Duration::from_secs(m * 60); } }
    #[doc(hidden)]
    pub fn set_auto_lock_duration(&self, d: Duration) { if let Ok(mut g) = self.inner.lock() { g.auto_lock = d; } }
    #[doc(hidden)]
    pub fn backdate_activity_wall(&self, d: Duration) {
        if let Ok(mut g) = self.inner.lock() {
            g.last_activity_wall = SystemTime::now().checked_sub(d).unwrap_or_else(SystemTime::now);
        }
    }

    pub fn status(&self) -> Result<StatusDto> {
        let g = self.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        let mins = g.auto_lock.as_secs() / 60;
        Ok(match &g.unlocked {
            None => StatusDto { unlocked: false, vault_dir: None, device_id: None, device_revoked: false, auto_lock_minutes: mins, warnings: vec![] },
            Some(u) => {
                let mut w = u.session.accepted_problems.clone();
                if u.session.kdf_upgraded { w.push("KDF parameters were upgraded on unlock".into()); }
                if u.session.device_revoked { w.push("this device is REVOKED: read-only".into()); }
                StatusDto { unlocked: true, vault_dir: Some(u.session.root.display().to_string()), device_id: Some(u.session.device_id.clone()), device_revoked: u.session.device_revoked, auto_lock_minutes: mins, warnings: w }
            }
        })
    }

    pub fn list_vaults(&self) -> Result<Vec<VaultDto>> {
        self.with(|u| Ok(u.session.list_vaults()?.into_iter().map(|(id, m)| VaultDto { id, name: m.name.clone(), description: m.description.clone() }).collect()))
    }
    pub fn create_vault(&self, name: &str) -> Result<String> {
        if name.trim().is_empty() { bail!("vault name is empty"); }
        self.with(|u| { Self::writable(u)?; u.session.create_vault(name.trim()) })
    }

    /// Items sorted by title, or ranked with core fuzzy search when `query` is non-empty.
    pub fn list_items(&self, vault: &str, query: &str) -> Result<Vec<ItemSummary>> {
        self.with(|u| {
            let items = u.session.list_items(vault)?;
            let q = query.trim();
            if q.is_empty() {
                let mut v: Vec<_> = items.iter().map(|(id, it)| summary(id.clone(), it, None)).collect();
                v.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
                Ok(v)
            } else {
                Ok(search::rank(q, items).into_iter().map(|(s, id, it)| summary(id, &it, Some(s))).collect())
            }
        })
    }

    pub fn get_item(&self, vault: &str, id: &str) -> Result<ItemDetail> {
        self.with(|u| {
            let (version, it) = u.session.read_item(vault, id)?;
            Ok(ItemDetail { id: id.into(), version, title: it.title.clone(), username: it.username.clone(), url: it.url.clone(), notes: it.notes.clone(),
                has_password: it.password.as_deref().is_some_and(|p| !p.is_empty()), has_totp: it.totp.is_some(), updated_at: it.updated_at.clone() })
        })
    }

    /// Secret value of one field (for "show password" or clipboard copy). TOTP returns the current code.
    pub fn field_value(&self, vault: &str, id: &str, field: Field) -> Result<Zeroizing<String>> {
        self.with(|u| {
            let (_, it) = u.session.read_item(vault, id)?;
            let v = match field {
                Field::Username => it.username.clone(),
                Field::Password => it.password.clone(),
                Field::Url => it.url.clone(),
                Field::Totp => match &it.totp { Some(t) => Some(totp::code_now(&totp::parse(t)?).0), None => None },
            };
            v.filter(|s| !s.is_empty()).map(Zeroizing::new).ok_or_else(|| anyhow!("item has no {field:?}"))
        })
    }

    pub fn totp(&self, vault: &str, id: &str) -> Result<TotpDto> {
        self.with(|u| {
            let (_, it) = u.session.read_item(vault, id)?;
            let spec = totp::parse(it.totp.as_deref().ok_or_else(|| anyhow!("item has no TOTP"))?)?;
            let (code, remaining) = totp::code_now(&spec);
            Ok(TotpDto { code, remaining, period: spec.period })
        })
    }

    pub fn add_item(&self, vault: &str, input: ItemInput) -> Result<String> {
        if input.title.trim().is_empty() { bail!("title is required"); }
        if let Some(t) = nonempty(input.totp.clone()) { totp::parse(&t)?; }
        self.with(|u| {
            Self::writable(u)?;
            let it = Item { title: input.title.trim().into(), username: nonempty(input.username.clone()), password: input.password.clone().filter(|p| !p.is_empty()),
                url: nonempty(input.url.clone()), notes: nonempty(input.notes.clone()), totp: nonempty(input.totp.clone()), updated_at: String::new() };
            u.session.add_item(vault, &it)
        })
    }

    pub fn update_item(&self, vault: &str, id: &str, input: ItemInput) -> Result<()> {
        if input.title.trim().is_empty() { bail!("title is required"); }
        if let Some(t) = nonempty(input.totp.clone()) { totp::parse(&t)?; }
        self.with(|u| {
            Self::writable(u)?;
            let (_, mut it) = u.session.read_item(vault, id)?;
            it.title = input.title.trim().into();
            it.username = nonempty(input.username.clone());
            it.url = nonempty(input.url.clone());
            it.notes = nonempty(input.notes.clone());
            it.totp = nonempty(input.totp.clone());
            if let Some(p) = &input.password { it.password = Some(p.clone()).filter(|p| !p.is_empty()); }
            u.session.update_item(vault, id, &it)
        })
    }

    pub fn delete_item(&self, vault: &str, id: &str) -> Result<()> {
        self.with(|u| { Self::writable(u)?; u.session.remove_item(vault, id) })
    }

    pub fn devices(&self) -> Result<Vec<DeviceDto>> {
        self.with(|u| {
            let cur = u.session.device_id.clone();
            Ok(u.session.devices()?.devices.into_iter().map(|d| DeviceDto { current: d.id == cur, id: d.id, name: d.name, created_at: d.created_at, last_seen_at: d.last_seen_at, revoked: d.revoked }).collect())
        })
    }

    /// `gitpass sync` (merge=false) or `gitpass sync --merge`, then re-unlock to verify integrity.
    pub fn sync(&self, merge: bool) -> Result<SyncDto> {
        self.sync_with_target(merge, None)
    }
    #[doc(hidden)]
    pub fn sync_with_target(&self, merge: bool, target_kdf: Option<gitpass_core::crypto::KdfParams>) -> Result<SyncDto> {
        self.with(|u| {
            let root = u.session.root.clone();
            // Keep the vault's existing KDF params: sync must never re-wrap keyset.enc / rewrite format.json.
            // (`target_kdf` is accepted for API compatibility but sync never upgrades the KDF.)
            let _ = &target_kdf;
            let opts = UnlockOpts { accept_rollback: u.opts_accept_rollback, target_kdf: u.session.fmt.kdf.clone() };
            // Device last-seen refresh (throttled) happens BEFORE the sync commit so it is pushed, not left dirty.
            let name = u.device_name.clone().unwrap_or_else(gitpass_core::device::default_device_name);
            u.session.touch_device(&name)?;
            let mut dto = SyncDto { log: String::new(), merged: vec![], reapplied: vec![], epoch: 0 };
            if merge {
                let r = gitpass_core::sync::sync_merge(&root, &u.password, &u.secret, &opts)?;
                dto.log = r.log.clone();
                for (v, i, m) in &r.merged {
                    let mut s = format!("{v}/{i}: remote={:?} local={:?}", m.from_remote, m.from_local);
                    if m.has_conflicts() { s.push_str(&format!(" CONFLICT (kept local): {:?}", m.conflicts)); }
                    dto.merged.push(s);
                }
                dto.reapplied = r.reapplied.iter().map(|(v, i)| format!("{v}/{i}")).collect();
            } else {
                dto.log = gitpass_core::sync::sync(&root)?;
            }
            // Re-unlock: post-pull integrity/rollback check, refresh session.
            // Verify-only reopen: integrity/rollback checks, no device/KDF/purge writes after the push.
            let s = Session::reopen_verify(&root, &u.password, &u.secret, opts.accept_rollback)
                .map_err(|e| anyhow!("post-pull integrity check failed (pre-pull backup in .gitpass-backup-*/): {e}"))?;
            dto.epoch = s.epoch()?;
            dto.log.push_str(&format!("post-pull integrity verified (manifest epoch {})\n", dto.epoch));
            u.session = s;
            Ok(dto)
        })
    }
}

/// Password generator (core charset). Length clamped to 8..=128.
pub fn gen_password(len: usize) -> String { generate_password(len.clamp(8, 128)) }

pub mod api;
pub mod favicon;
pub mod qr;
/// UNIX-socket agent (macOS / Linux).
#[cfg(unix)]
pub mod agent;
/// Windows: named-pipe agent is NOT implemented yet (stub; see agent_windows.rs).
#[cfg(not(unix))]
#[path = "agent_windows.rs"]
pub mod agent;
pub mod ssh_agent;
/// In-memory log of SSH agent sign requests (Settings → SSH Agent).
pub mod ssh_activity;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod api_tests;
#[cfg(all(test, unix))]
mod agent_tests;
#[cfg(all(test, unix))] // uses the UNIX-socket agent
mod ssh_tests;
