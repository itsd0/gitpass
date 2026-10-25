//! git sync with a repo lock and a pre-pull backup. Integrity of what was pulled is verified by the
//! caller (CLI) by re-unlocking, which checks the authenticated manifest + rollback epoch.
use crate::fsutil;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    Ok((o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))))
}

/// `git add -A && git commit -m msg` if there is anything staged. Returns true if a commit was made.
pub fn commit_all(dir: &Path, msg: &str) -> Result<bool> {
    git(dir, &["add", "-A"])?;
    let (clean, _) = git(dir, &["diff", "--cached", "--quiet"])?;
    if clean { return Ok(false); }
    let (ok, out) = git(dir, &["commit", "-m", msg])?;
    if !ok { bail!("git commit failed:\n{out}"); }
    Ok(true)
}

/// Lock; git add/commit; backup; pull --rebase; push (skips remote steps if no origin).
pub fn sync(dir: &Path) -> Result<String> {
    let mut log = String::new();
    if !dir.join(".git").exists() {
        bail!("{} is not a git repository. Run: git -C {} init && git -C {} remote add origin git@github.com:YOU/your-private-vault.git", dir.display(), dir.display(), dir.display());
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
    if !ok { bail!("git push failed:\n{out}"); }
    log.push_str("pulled and pushed to origin\n");
    Ok(log)
}
