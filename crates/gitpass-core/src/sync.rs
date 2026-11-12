//! git sync with a repo lock and a pre-pull backup. Integrity of what was pulled is verified by the
//! caller (CLI) by re-unlocking, which checks the authenticated manifest + rollback epoch.
use crate::fsutil;
use anyhow::{bail, Result};
use std::path::Path;

fn git(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    crate::remote::git_cmd(dir, args)
}

/// `git add -A && git commit -m msg` if there is anything staged. Returns true if a commit was made.
pub fn commit_all(dir: &Path, msg: &str) -> Result<bool> {
    crate::remote::ensure_gitignore(dir)?;
    crate::remote::commit_checked(dir, msg)
}

/// Lock; git add/commit; backup; pull --rebase; push (skips remote steps if no origin).
pub fn sync(dir: &Path) -> Result<String> {
    let mut log = String::new();
    if !dir.join(".git").exists() {
        bail!("{} is not a git repository. Connect a remote first: gitpass remote add git@github.com:YOU/your-private-vault.git (or Settings → Sync → Connect GitHub in the app)", dir.display());
    }
    let _g = fsutil::lock(dir)?;
    log.push_str(if commit_all(dir, "sync")? { "committed local changes\n" } else { "nothing to commit\n" });
    let (has_remote, _) = git(dir, &["remote", "get-url", "origin"])?;
    if !has_remote {
        log.push_str("no 'origin' remote configured; skipped pull/push\n");
        return Ok(log);
    }
    let b = fsutil::backup(dir, "")?;
    fsutil::prune_backups(dir, 5)?;
    log.push_str(&format!("pre-pull backup: {}\n", b.display()));
    let (ok, out) = git(dir, &["pull", "--rebase", "origin", "HEAD"])?;
    if !ok && !out.contains("couldn't find remote ref") { bail!("git pull --rebase failed (backup at {}):\n{out}", b.display()); }
    let (ok, out) = git(dir, &["push", "-u", "origin", "HEAD"])?;
    if !ok { return Err(crate::remote::classify_git_error("git push", &out)); }
    log.push_str("pulled and pushed to origin\n");
    Ok(log)
}

/// Summary of a `sync --merge` run.
#[derive(Debug, Default)]
pub struct MergeSyncReport {
    pub log: String,
    /// (vault id, item id, report) for every item that existed on both sides and differed.
    pub merged: Vec<(String, String, crate::merge::MergeReport)>,
    /// Local-only items kept and pushed (absent remotely and not deleted there since the last sync).
    pub reapplied: Vec<(String, String)>,
    /// Secret-free conflict report from the shared engine (ids, field names, kinds):
    /// equal-clock field conflicts, delete-vs-edit, vault meta/key, identity, blobs.
    /// Every conflict is resolved fail-safe (nothing is lost); see `SyncConflict::describe`.
    pub conflicts: Vec<String>,
}

/// `sync --merge` on the shared sync engine (issue #454): the same three-way
/// planner as every other platform ([`crate::sync_engine::plan_sync`]), driven
/// over the vault's git repository by [`crate::sync_remote::sync_with_remote`]
/// with the [`crate::sync_git::GitCliRemote`] transport.
///
/// 1. Unlock (integrity + rollback checks); commit pending local changes
///    under the repo lock; take a pre-pull backup (kept: last 5).
/// 2. Fetch the remote branch; decrypt the remote tree with this session's
///    keys (no KDF on remote data) and refuse a remote below this device's
///    last-synced epoch.
/// 3. Base = `git merge-base HEAD <remote>` (else the recorded last-synced
///    commit, else empty = union). Deletes propagate from either side when the
///    other side did not edit the item since the base; delete-vs-edit keeps the
///    edit and is reported. Whole vaults created or deleted on one side are
///    handled too (the old MVP aborted on a local-only vault).
/// 4. Apply locally, push a commit whose parent is the fetched head with a
///    plain non-forced push (re-fetch + re-plan if someone pushed meanwhile,
///    bounded), then move the local branch onto it. A pure pull is a
///    fast-forward of the files.
///
/// The caller re-unlocks afterwards (`Session::reopen_verify`) to verify the
/// final state, as before. A remote password/KDF change made on another device
/// is refused here (pull it with plain `sync`, which fast-forwards it).
pub fn sync_merge(dir: &Path, password: &str, secret: &crate::crypto::SecretKey, opts: &crate::store::UnlockOpts) -> Result<MergeSyncReport> {
    use crate::store::Session;
    use crate::sync_remote::{sync_with_remote, SyncOptions};
    let mut rep = MergeSyncReport::default();
    if !dir.join(".git").exists() { bail!("{} is not a git repository", dir.display()); }
    let s = Session::unlock_with(dir, password, secret, opts)?;
    {
        let _g = fsutil::lock(dir)?;
        crate::remote::ensure_gitattributes(dir)?;
        rep.log.push_str(if commit_all(dir, "sync")? { "committed local changes\n" } else { "nothing to commit\n" });
    }
    let (has_remote, _) = git(dir, &["remote", "get-url", "origin"])?;
    if !has_remote { rep.log.push_str("no 'origin' remote configured; skipped pull/push\n"); return Ok(rep); }
    let b = fsutil::backup(dir, "")?;
    fsutil::prune_backups(dir, 5)?;
    rep.log.push_str(&format!("pre-pull backup: {}\n", b.display()));
    let mut repo = crate::sync_git::GitCliRemote::open(dir)?;
    let id = repo.remote_id();
    let out = sync_with_remote(&s, &mut repo, &id, &SyncOptions { message: "sync --merge".into(), ..Default::default() })
        .map_err(|e| anyhow::anyhow!("{e:#} (pre-pull backup kept in .gitpass-backup-*/)"))?;
    rep.merged = out.merged;
    rep.reapplied = out.local_only;
    rep.conflicts = out.conflicts.iter().map(|c| c.describe()).collect();
    if out.attempts > 1 {
        rep.log.push_str(&format!("remote moved during sync; re-planned ({} attempts)\n", out.attempts));
    }
    rep.log.push_str(match (out.local_changed, out.pushed) {
        (false, false) => "already in sync with remote\n",
        (true, false) => "fast-forwarded to remote\n",
        (false, true) => "local is ahead; pushed to origin\n",
        (true, true) => "merged with remote (shared engine) and pushed to origin\n",
    });
    if !rep.merged.is_empty() || !rep.reapplied.is_empty() {
        rep.log.push_str(&format!("field-merged {} item(s), kept {} local-only item(s)\n", rep.merged.len(), rep.reapplied.len()));
    }
    for c in &rep.conflicts { rep.log.push_str(&format!("conflict: {c}\n")); }
    Ok(rep)
}
