//! Local (non-git) state: last-seen epoch, item usage and wall-clock
//! helpers. Pure move from `store.rs`.

use crate::store::{read_json, write_json};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// ---------- local (non-git) state: last seen epoch & item usage ----------
#[derive(Serialize, Deserialize, Default)]
struct LocalState {
    account_id: String,
    epoch: u64,
    #[serde(default)]
    last_used: BTreeMap<String, u64>,
    /// Last-synced remote state for API transports (issue #453); absent until the first sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sync_base: Option<SyncBase>,
}

/// What this device last synced with an API remote (GitHub has no merge-base,
/// so the base is tracked locally, issue #453). Stored in the local-only,
/// git-ignored `.gitpass-state.json`; never pushed. Holds only public ids:
/// no paths, tokens or decrypted data.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SyncBase {
    /// Opaque remote identity (e.g. `github:owner/repo#branch`): a base is
    /// only used against the remote it was recorded for.
    pub remote: String,
    /// Commit id both sides last agreed on: the merge base of the next sync.
    pub commit: String,
    /// Git tree id of that commit, re-verified when the base is fetched again.
    pub tree: String,
    /// Manifest epoch of that commit: the remote must never go below it.
    pub epoch: u64,
}
fn state_path(root: &Path) -> PathBuf { root.join(".gitpass-state.json") }
/// Last manifest epoch this device has seen for `account_id` (None = first use on this device: TOFU).
pub fn last_epoch(root: &Path, account_id: &str) -> Option<u64> {
    let s: LocalState = read_json(&state_path(root)).ok()?;
    (s.account_id == account_id).then_some(s.epoch)
}
/// Read-modify-write `.gitpass-state.json` under an exclusive advisory lock, so the native host, the
/// desktop agent and the CLI can't lose each other's updates (e.g. a usage write must never put back an
/// older anti-rollback epoch). The lock is taken on the vault directory itself (stable, unlike the state
/// file, which is replaced by rename) and is separate from `.gitpass.lock`, so callers that already hold
/// the vault lock can use it without deadlocking.
/// `f` gets `None` if the existing file can't be parsed; it returns the state to write, or `None` to skip.
fn update_state(root: &Path, f: impl FnOnce(Option<LocalState>) -> Option<LocalState>) -> Result<()> {
    use fs2::FileExt;
    fs::create_dir_all(root)?;
    let dir = fs::File::open(root).with_context(|| format!("opening {}", root.display()))?;
    dir.lock_exclusive().context("locking the vault directory for .gitpass-state.json")?;
    let p = state_path(root);
    let cur = match fs::read(&p) {
        Ok(b) => serde_json::from_slice(&b).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(LocalState::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", p.display())),
    };
    let res = match f(cur) { Some(next) => write_json(&p, &next).map(|_| ()), None => Ok(()) };
    let _ = FileExt::unlock(&dir);
    res
}
pub(crate) fn save_epoch(root: &Path, account_id: &str, epoch: u64) -> Result<()> { write_epoch(root, account_id, epoch, false) }
/// `allow_lower` is only for `set_last_epoch` (e.g. `sync --merge` deliberately rewinds to the
/// merge-base epoch before adopting the remote); normal saves never lower the last-seen epoch.
fn write_epoch(root: &Path, account_id: &str, epoch: u64, allow_lower: bool) -> Result<()> {
    update_state(root, |cur| {
        // A corrupt file is replaced: the epoch is what matters for rollback detection.
        let mut s = cur.unwrap_or_default();
        if s.account_id == account_id {
            s.epoch = if allow_lower { epoch } else { s.epoch.max(epoch) };
        } else {
            s.account_id = account_id.into();
            s.epoch = epoch;
            s.sync_base = None; // another account's base is meaningless here
        }
        Some(s)
    })
}

/// This device's last-synced base for `remote` (None: never synced with it, another account, or unreadable).
pub fn sync_base(root: &Path, account_id: &str, remote: &str) -> Option<SyncBase> {
    let s: LocalState = read_json(&state_path(root)).ok()?;
    if s.account_id != account_id {
        return None;
    }
    s.sync_base.filter(|b| b.remote == remote)
}

/// Record the last-synced base. Refuses to lower the base epoch for the same
/// remote (that would re-open a rollback window) and never touches the
/// anti-rollback epoch or usage data.
pub fn set_sync_base(root: &Path, account_id: &str, base: &SyncBase) -> Result<()> {
    let mut refused = false;
    update_state(root, |cur| {
        let mut s = cur.unwrap_or_default();
        if s.account_id.is_empty() {
            s.account_id = account_id.into();
        } else if s.account_id != account_id {
            refused = true;
            return None;
        }
        if let Some(old) = &s.sync_base {
            if old.remote == base.remote && base.epoch < old.epoch {
                refused = true;
                return None;
            }
        }
        s.sync_base = Some(base.clone());
        Some(s)
    })?;
    if refused {
        anyhow::bail!("refusing to record a sync base below the last-synced epoch or for another account");
    }
    Ok(())
}

/// Record that an item was used at timestamp `ts` (seconds since unix epoch).
/// Stored in `.gitpass-state.json` (local-only, git-ignored) so no git commit or sync conflict is created.
pub fn touch_item_used(root: &Path, item_id: &str, ts: u64) -> Result<()> {
    update_state(root, |cur| {
        // Unparseable state: skip the usage write rather than wiping `account_id`/`epoch`.
        let mut s = cur?;
        s.last_used.insert(item_id.into(), ts);
        Some(s)
    })
}

/// Retrieve the last used timestamp for an item (None if never used on this device).
pub fn get_item_last_used(root: &Path, item_id: &str) -> Option<u64> {
    let s: LocalState = read_json(&state_path(root)).ok()?;
    s.last_used.get(item_id).copied()
}

/// Retrieve all recorded item last used timestamps on this device.
pub fn get_all_last_used(root: &Path) -> BTreeMap<String, u64> {
    let s: LocalState = read_json(&state_path(root)).ok().unwrap_or_default();
    s.last_used
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
pub fn set_last_epoch(root: &Path, account_id: &str, epoch: u64) -> Result<()> { write_epoch(root, account_id, epoch, true) }

#[cfg(test)]
mod local_state_tests {
    use super::*;

    #[test]
    fn sync_base_is_per_remote_and_never_lowered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        save_epoch(root, "acct", 5).unwrap();
        let b = |remote: &str, epoch| SyncBase { remote: remote.into(), commit: "c".repeat(40), tree: "t".repeat(40), epoch };
        assert_eq!(sync_base(root, "acct", "r1"), None);
        set_sync_base(root, "acct", &b("r1", 5)).unwrap();
        assert_eq!(sync_base(root, "acct", "r1"), Some(b("r1", 5)));
        assert_eq!(sync_base(root, "acct", "r2"), None, "only for the remote it was recorded for");
        assert_eq!(sync_base(root, "other", "r1"), None);
        assert!(set_sync_base(root, "acct", &b("r1", 4)).is_err(), "never lowered");
        assert!(set_sync_base(root, "other", &b("r1", 9)).is_err(), "other account");
        set_sync_base(root, "acct", &b("r2", 1)).unwrap(); // a different remote starts over
        touch_item_used(root, "item", 1).unwrap();
        save_epoch(root, "acct", 7).unwrap();
        assert_eq!(sync_base(root, "acct", "r2"), Some(b("r2", 1)), "other writers keep the base");
        assert_eq!(last_epoch(root, "acct"), Some(7));
        save_epoch(root, "new-acct", 1).unwrap();
        assert_eq!(sync_base(root, "new-acct", "r2"), None, "account switch drops the base");
    }

    #[test]
    fn usage_writes_never_lower_the_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        save_epoch(root, "acct", 5).unwrap();
        touch_item_used(root, "item-a", 100).unwrap();
        assert_eq!(last_epoch(root, "acct"), Some(5));
        // A stale writer for the same account can't put back an older epoch.
        save_epoch(root, "acct", 3).unwrap();
        assert_eq!(last_epoch(root, "acct"), Some(5));
        assert_eq!(get_item_last_used(root, "item-a"), Some(100));
        save_epoch(root, "acct", 6).unwrap();
        assert_eq!(last_epoch(root, "acct"), Some(6));
        // An explicit rewind (sync --merge) may lower it, and keeps usage data.
        set_last_epoch(root, "acct", 4).unwrap();
        assert_eq!(last_epoch(root, "acct"), Some(4));
        assert_eq!(get_item_last_used(root, "item-a"), Some(100));
    }

    #[test]
    fn concurrent_usage_and_epoch_writes_keep_the_max_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        save_epoch(&root, "acct", 1).unwrap();
        let hs: Vec<_> = (0..8).map(|t| {
            let root = root.clone();
            std::thread::spawn(move || {
                for i in 0..25u64 {
                    if t % 2 == 0 { save_epoch(&root, "acct", 2 + i).unwrap(); }
                    else { touch_item_used(&root, &format!("item-{t}-{i}"), i).unwrap(); }
                }
            })
        }).collect();
        for h in hs { h.join().unwrap(); }
        assert_eq!(last_epoch(&root, "acct"), Some(26));
        assert_eq!(get_all_last_used(&root).len(), 4 * 25, "no usage write may be lost");
    }

    #[test]
    fn corrupt_state_is_not_wiped_by_a_usage_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(state_path(root), b"{not json").unwrap();
        touch_item_used(root, "item-a", 1).unwrap();
        assert_eq!(fs::read(state_path(root)).unwrap(), b"{not json");
    }
}
