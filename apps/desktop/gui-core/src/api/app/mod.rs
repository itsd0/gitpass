//! `App`: the contract API behind the desktop app (see `api/mod.rs`).
//! Command families live in sibling modules as further `impl App` blocks.

mod import_export;
mod items;
mod qr;
mod settings;
mod sharing;
mod sync;
mod tools;
mod vaults;

use super::clipboard::{Clipboard, UreqHibp};
use super::dto::{BiometricStore, NoBiometrics};
use super::local::{LocalFile, SETTINGS_FILE};
use crate::{GuiState, Unlocked};
use anyhow::{anyhow, Result};
use gitpass_core::audit::PwnedRangeClient;
use gitpass_core::store::UnlockOpts;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

// ------------------------------------------------------------------ App
pub struct App {
    pub state: GuiState,
    settings_dir: PathBuf,
    pub(crate) clipboard: Arc<dyn Clipboard>,
    clip_gen: Arc<AtomicU64>,
    last_copied: Arc<Mutex<Option<Zeroizing<String>>>>,
    hibp: Box<dyn PwnedRangeClient + Send + Sync>,
    unlock_opts: Mutex<UnlockOpts>,
    /// Emergency kit text with the NEW Secret Key, kept only right after rotate_secret_key (until lock).
    fresh_kit: Mutex<Option<Zeroizing<String>>>,
    /// (account id, encoded Secret Key) right after create_account / rotate_secret_key (until lock).
    fresh_secret: Mutex<Option<(String, Zeroizing<String>)>>,
    biometric: Box<dyn BiometricStore>,
    file_lock: Mutex<()>,
    favicon_cache_dir: Mutex<Option<PathBuf>>,
    /// SSH agent sign requests since unlock (metadata only); cleared by every lock path.
    pub(crate) ssh_activity: Arc<crate::ssh_activity::SshActivityLog>,
    /// "Linked from" reverse index, keyed by the manifest change stamp (any write re-seals the manifest,
    /// so a stale index is never served); cleared by every lock path.
    pub(crate) backlink_cache: Arc<Mutex<Option<BacklinkIndex>>>,
}

/// Same hash as `GuiState::change_stamp`, computed from a session root (the state lock is already held here).
fn manifest_stamp(root: &std::path::Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let b = std::fs::read(root.join("manifest.enc")).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    Some(h.finish())
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BacklinkIndex {
    pub stamp: u64,
    pub by_target: std::collections::HashMap<String, Vec<super::dto::RelatedItem>>,
}


impl App {
    /// `settings_dir`: where `.gitpass-settings.json` lives (the desktop app uses the default vault dir).
    pub fn new(settings_dir: PathBuf, clipboard: Arc<dyn Clipboard>) -> Self {
        let app = App { state: GuiState::new(5), settings_dir, clipboard, clip_gen: Arc::new(AtomicU64::new(0)), last_copied: Arc::new(Mutex::new(None)), hibp: Box::new(UreqHibp),
            unlock_opts: Mutex::new(UnlockOpts::default()), fresh_kit: Mutex::new(None), fresh_secret: Mutex::new(None), biometric: Box::new(NoBiometrics), file_lock: Mutex::new(()),
            favicon_cache_dir: Mutex::new(None), ssh_activity: Default::default(), backlink_cache: Default::default() };
        app.state.on_lock({ let log = app.ssh_activity.clone(); Arc::new(move || log.clear()) });
        app.state.on_lock({ let bl = app.backlink_cache.clone(); Arc::new(move || { if let Ok(mut g) = bl.lock() { *g = None; } }) });
        let s = app.get_settings();
        app.state.set_auto_lock_minutes(s.auto_lock_minutes);
        app
    }
    /// Inject an HIBP client (tests).
    /// Inject the platform biometric store.
    pub fn with_biometric(mut self, b: Box<dyn BiometricStore>) -> Self { self.biometric = b; self }
    pub fn with_hibp(mut self, c: Box<dyn PwnedRangeClient + Send + Sync>) -> Self { self.hibp = c; self }
    #[doc(hidden)]
    pub fn set_unlock_opts(&self, o: UnlockOpts) { *self.unlock_opts.lock().unwrap() = o; }

    pub(crate) fn settings_path(&self) -> PathBuf { self.settings_dir.join(SETTINGS_FILE) }
    pub fn read_local(&self) -> LocalFile {
        std::fs::read(self.settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub(crate) fn update_local<T>(&self, f: impl FnOnce(&mut LocalFile) -> T) -> Result<T> {
        let _g = self.file_lock.lock().map_err(|_| anyhow!("poisoned"))?;
        let mut l = self.read_local();
        let r = f(&mut l);
        std::fs::create_dir_all(&self.settings_dir)?;
        let tmp = self.settings_path().with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&l)?)?;
        std::fs::rename(&tmp, self.settings_path())?;
        if self.settings_dir.join("format.json").exists() { gitpass_core::device::ensure_ignored(&self.settings_dir, SETTINGS_FILE)?; }
        Ok(r)
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> { self.state.with(f) }
    pub(crate) fn write<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> { self.state.with(|u| { GuiState::writable(u)?; f(u) }) }
    /// Items linking to `target_id` via Related items. Rebuilds the reverse index only when the manifest changed.
    pub(crate) fn get_backlinks(&self, s: &gitpass_core::store::Session, target_id: &str) -> Result<Vec<super::dto::RelatedItem>> {
        let stamp = manifest_stamp(&s.root);
        if let (Some(st), Ok(g)) = (stamp, self.backlink_cache.lock()) {
            if let Some(idx) = g.as_ref().filter(|i| i.stamp == st) { return Ok(idx.by_target.get(target_id).cloned().unwrap_or_default()); }
        }
        let by_target = super::display::build_backlinks_from_records(&Self::all_records(s)?);
        let out = by_target.get(target_id).cloned().unwrap_or_default();
        if let (Some(st), Ok(mut g)) = (stamp, self.backlink_cache.lock()) { *g = Some(BacklinkIndex { stamp: st, by_target }); }
        Ok(out)
    }
    /// Manifest epoch (bumped once per commit); used by tests to assert a single commit.
    #[cfg(test)]
    pub(crate) fn session_epoch(&self) -> Result<u64> { self.with(|u| u.session.epoch()) }

}
