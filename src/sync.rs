use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    Ok((o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))))
}

/// git add -A && commit && pull --rebase && push (skips remote steps if no origin).
pub fn sync(dir: &Path) -> Result<String> {
    let mut log = String::new();
    if !dir.join(".git").exists() {
        bail!("{} is not a git repository. Run: git -C {} init && git -C {} remote add origin git@github.com:YOU/your-private-vault.git", dir.display(), dir.display(), dir.display());
    }
    git(dir, &["add", "-A"])?;
    let (clean, _) = git(dir, &["diff", "--cached", "--quiet"])?;
    if !clean {
        let (ok, out) = git(dir, &["commit", "-m", "sync"])?;
        if !ok { bail!("git commit failed:\n{out}"); }
        log.push_str("committed local changes\n");
    } else {
        log.push_str("nothing to commit\n");
    }
    let (has_remote, _) = git(dir, &["remote", "get-url", "origin"])?;
    if !has_remote {
        log.push_str("no 'origin' remote configured; skipped pull/push\n");
        return Ok(log);
    }
    let (ok, out) = git(dir, &["pull", "--rebase", "origin", "HEAD"])?;
    if !ok && !out.contains("couldn't find remote ref") { bail!("git pull --rebase failed:\n{out}"); }
    let (ok, out) = git(dir, &["push", "-u", "origin", "HEAD"])?;
    if !ok { bail!("git push failed:\n{out}"); }
    log.push_str("pulled and pushed to origin\n");
    Ok(log)
}
