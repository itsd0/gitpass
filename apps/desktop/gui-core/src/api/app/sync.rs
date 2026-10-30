use super::super::dto::*;
use super::App;
use anyhow::{Result};
use std::path::{Path, PathBuf};

impl App {
    // ---------------- sync
    pub fn sync_status(&self) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        let last_sync = self.read_local().last_sync;
        if git(&root, &["rev-parse", "--is-inside-work-tree"]).is_none() {
            return Ok(SyncStatus { remote: None, last_sync, ahead: 0, behind: 0, dirty: false });
        }
        let remote = git(&root, &["remote", "get-url", "origin"]).filter(|s| !s.is_empty());
        let (mut ahead, mut behind) = (0, 0);
        if let Some(c) = git(&root, &["rev-list", "--left-right", "--count", "HEAD...@{u}"]) {
            let mut it = c.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
            ahead = it.next().unwrap_or(0); behind = it.next().unwrap_or(0);
        }
        let dirty = git(&root, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
        Ok(SyncStatus { remote, last_sync, ahead, behind, dirty })
    }
    /// Settings → Sync → Connect: `gitpass remote add <url>` for the unlocked vault. Uses the user's existing git
    /// credentials only (no tokens are asked for or stored). Errors start with a `remote::E_*` category marker.
    pub fn remote_connect(&self, url: &str) -> Result<SyncStatus> { self.remote_connect_opts(url, false) }
    #[doc(hidden)]
    pub fn remote_connect_opts(&self, url: &str, allow_local: bool) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        gitpass_core::remote::add(&root, url, allow_local)?;
        let _ = self.update_local(|l| l.last_sync = Some(gitpass_core::device::now_iso8601()));
        let _ = self.set_onboarding(OnboardingPatch { repo_connected: Some(true), ..Default::default() });
        self.sync_status()
    }
    /// Remove `origin` (local files and history are kept; nothing is deleted on the remote).
    pub fn remote_disconnect(&self) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        gitpass_core::remote::remove(&root)?;
        let _ = self.set_onboarding(OnboardingPatch { repo_connected: Some(false), ..Default::default() });
        self.sync_status()
    }
    pub fn sync(&self, merge: bool) -> Result<SyncResult> {
        let d = self.state.sync(merge)?;
        let _ = self.update_local(|l| l.last_sync = Some(gitpass_core::device::now_iso8601()));
        let mut m = d.log;
        for x in d.merged { m.push_str(&format!("merged {x}\n")); }
        Ok(SyncResult { message: m })
    }

    /// Favicons are opt-in (`showWebsiteIcons` setting or Rust-only `allowFavicons` key).
    /// Returns None immediately without network calls when off.
    pub fn favicon_url(&self, url: &str) -> Option<String> {
        let s = self.get_settings();
        if !s.show_website_icons && !self.read_local().allow_favicons {
            return None;
        }
        let cache_dir = self.favicon_cache_dir.lock().unwrap().clone().unwrap_or_else(crate::favicon::default_favicon_cache_dir);
        crate::favicon::fetch_and_cache(url, &cache_dir, false).ok().flatten()
    }

    pub fn favicon_cache_dir(&self) -> PathBuf {
        self.favicon_cache_dir.lock().unwrap().clone().unwrap_or_else(crate::favicon::default_favicon_cache_dir)
    }

    pub fn set_favicon_cache_dir(&self, dir: PathBuf) {
        *self.favicon_cache_dir.lock().unwrap() = Some(dir);
    }

}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

