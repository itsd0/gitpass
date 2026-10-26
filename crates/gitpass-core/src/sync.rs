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
    /// Local-only items re-applied on top of remote.
    pub reapplied: Vec<(String, String)>,
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let (ok, out) = git(dir, args)?;
    if !ok { bail!("git {} failed:\n{out}", args.join(" ")); }
    Ok(out)
}

/// `sync --merge`: field-level merge instead of failing on rebase conflicts.
///
/// 1. unlock, snapshot the plaintext record of every local item; commit local changes; backup.
/// 2. `git fetch origin HEAD`. Fast-forward / ahead-only cases are handled like plain sync.
/// 3. Diverged: decrypt the merge-base manifest and remember its epoch; `git reset --hard FETCH_HEAD`;
///    re-unlock requiring the remote epoch >= merge-base epoch (rollback check still applies);
///    then for each snapshotted item: field-merge into the remote copy (`Session::merge_item`) or, if the
///    remote lacks it, re-add it. Every write bumps the item version and re-seals the manifest (epoch+1).
/// 4. commit + push.
///
/// Limitations (MVP): a vault created locally but absent remotely aborts (backup kept); an item deleted
/// remotely but still present locally is resurrected (fail-safe: no data loss); a local delete of an item
/// that still exists remotely is lost (the item comes back).
pub fn sync_merge(dir: &Path, password: &str, secret: &crate::crypto::SecretKey, opts: &crate::store::UnlockOpts) -> Result<MergeSyncReport> {
    use crate::store::{set_last_epoch, Session};
    let mut rep = MergeSyncReport::default();
    if !dir.join(".git").exists() { bail!("{} is not a git repository", dir.display()); }
    let s = Session::unlock_with(dir, password, secret, opts)?;
    let mut snap = vec![];
    let lm = s.load_manifest()?;
    for (vid, ve) in &lm.vaults {
        for iid in ve.items.keys() { snap.push((vid.clone(), iid.clone(), s.read_record(vid, iid)?.1)); }
    }
    let local_vaults: Vec<String> = lm.vaults.keys().cloned().collect();
    drop(s);
    {
        let _g = fsutil::lock(dir)?;
        rep.log.push_str(if commit_all(dir, "sync")? { "committed local changes\n" } else { "nothing to commit\n" });
    }
    let (has_remote, _) = git(dir, &["remote", "get-url", "origin"])?;
    if !has_remote { rep.log.push_str("no 'origin' remote configured; skipped pull/push\n"); return Ok(rep); }
    let b = fsutil::backup(dir, "")?;
    fsutil::prune_backups(dir, 5)?;
    rep.log.push_str(&format!("pre-pull backup: {}\n", b.display()));
    let (ok, out) = git(dir, &["fetch", "origin", "HEAD"])?;
    if !ok {
        if out.contains("couldn't find remote ref") {
            git_ok(dir, &["push", "-u", "origin", "HEAD"])?;
            rep.log.push_str("remote empty; pushed\n");
            return Ok(rep);
        }
        bail!("git fetch failed:\n{out}");
    }
    let head = git_ok(dir, &["rev-parse", "HEAD"])?.trim().to_string();
    let theirs = git_ok(dir, &["rev-parse", "FETCH_HEAD"])?.trim().to_string();
    let base = git_ok(dir, &["merge-base", "HEAD", "FETCH_HEAD"])?.trim().to_string();
    if base == theirs {
        rep.log.push_str("local is ahead or equal\n");
    } else if base == head {
        git_ok(dir, &["merge", "--ff-only", "FETCH_HEAD"])?;
        rep.log.push_str("fast-forwarded to remote\n");
    } else {
        let s = Session::unlock_with(dir, password, secret, opts)?;
        let base_bytes = std::process::Command::new("git").arg("-C").arg(dir)
            .args(["show", &format!("{base}:manifest.enc")]).output()?;
        if !base_bytes.status.success() { bail!("cannot read merge-base manifest"); }
        let base_epoch = s.open_manifest_bytes(&base_bytes.stdout)?.epoch;
        let account_id = s.fmt.account_id.clone();
        drop(s);
        git_ok(dir, &["reset", "--hard", "FETCH_HEAD"])?;
        set_last_epoch(dir, &account_id, base_epoch)?;
        let s = Session::unlock_with(dir, password, secret, opts)
            .map_err(|e| anyhow::anyhow!("remote state failed integrity check ({e:#}); local state is in {}", b.display()))?;
        let rm = s.load_manifest()?;
        if let Some(v) = local_vaults.iter().find(|v| !rm.vaults.contains_key(*v)) {
            bail!("vault {v} exists only locally; sync --merge cannot re-apply whole vaults yet. Local state is in {}", b.display());
        }
        for (vid, iid, local) in &snap {
            if rm.vaults[vid].items.contains_key(iid) {
                let (_, remote) = s.read_record(vid, iid)?;
                if remote == *local { continue; }
                // merge local snapshot INTO the remote file, but report from the local device's viewpoint
                let (merged, r) = crate::merge::merge_records(local, &remote);
                if merged != remote { s.put_record(vid, iid, &merged)?; }
                rep.merged.push((vid.clone(), iid.clone(), r));
            } else {
                s.put_record(vid, iid, local)?;
                rep.reapplied.push((vid.clone(), iid.clone()));
            }
        }
        let _g = fsutil::lock(dir)?;
        commit_all(dir, "sync --merge: field-level merge")?;
        rep.log.push_str(&format!("diverged from remote; field-merged {} item(s), re-applied {} local-only item(s)\n",
            rep.merged.len(), rep.reapplied.len()));
    }
    git_ok(dir, &["push", "-u", "origin", "HEAD"])?;
    rep.log.push_str("pushed to origin\n");
    Ok(rep)
}
