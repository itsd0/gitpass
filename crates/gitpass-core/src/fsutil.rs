//! Crash-safe file helpers: atomic writes, directory copy/backup, and the repo lock.
use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Write to a temp file in the same directory, fsync it, atomically rename over `p`, fsync the dir.
pub fn atomic_write(p: &Path, data: &[u8]) -> Result<()> {
    let dir = p.parent().context("path has no parent")?;
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::Builder::new().prefix(".tmp-").suffix(".tmp").tempfile_in(dir)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    tmp.persist(p).map_err(|e| e.error).with_context(|| format!("renaming into {}", p.display()))?;
    fsync_dir(dir);
    Ok(())
}

pub fn fsync_dir(d: &Path) {
    if let Ok(f) = File::open(d) { let _ = f.sync_all(); }
}

/// Rename + fsync parent.
pub fn rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).with_context(|| format!("rename {} -> {}", from.display(), to.display()))?;
    if let Some(d) = to.parent() { fsync_dir(d); }
    Ok(())
}

/// Exclusive advisory lock on `<root>/.gitpass.lock`, released on drop.
/// NOTE: not re-entrant; never take it twice in one call chain.
pub struct LockGuard(File);
impl Drop for LockGuard {
    fn drop(&mut self) { let _ = FileExt::unlock(&self.0); }
}
pub fn lock(root: &Path) -> Result<LockGuard> {
    fs::create_dir_all(root)?;
    let f = fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join(".gitpass.lock"))?;
    f.lock_exclusive().context("acquiring .gitpass.lock")?;
    Ok(LockGuard(f))
}

/// Names never copied into backups / never treated as vault data.
pub fn is_local_only(name: &str) -> bool {
    name == ".git" || name == ".gitpass.lock" || name.starts_with(".gitpass-backup-") || name == ".gitpass-migrate-tmp"
}

pub fn copy_dir(src: &Path, dst: &Path, skip: &dyn Fn(&str) -> bool) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        if skip(&name) { continue; }
        let (s, d) = (e.path(), dst.join(&name));
        if e.file_type()?.is_dir() { copy_dir(&s, &d, &|_| false)?; } else { fs::copy(&s, &d)?; }
    }
    Ok(())
}

pub fn ts() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}-{:09}", d.as_secs(), d.subsec_nanos())
}

/// Copy the vault dir (minus .git, locks, other backups) to `<root>/.gitpass-backup-<tag><ts>/`.
pub fn backup(root: &Path, tag: &str) -> Result<PathBuf> {
    let dst = root.join(format!(".gitpass-backup-{tag}{}", ts()));
    copy_dir(root, &dst, &is_local_only)?;
    fsync_dir(root);
    Ok(dst)
}

/// Keep only the newest `keep` routine backups (`.gitpass-backup-<digits>...`); v1 migration backups are kept.
pub fn prune_backups(root: &Path, keep: usize) -> Result<()> {
    let mut v: Vec<PathBuf> = fs::read_dir(root)?.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(".gitpass-backup-")).map(|r| r.starts_with(|c: char| c.is_ascii_digit())).unwrap_or(false))
        .collect();
    v.sort();
    while v.len() > keep { let p = v.remove(0); let _ = fs::remove_dir_all(p); }
    Ok(())
}

pub const GITIGNORE: &str = "secret.key\n*.tmp\n.gitpass.lock\n.gitpass-state.json\n.gitpass-device.json\n.gitpass-device-key.json\n.gitpass-revoke-pending.json\n.gitpass-backup-*/\n.gitpass-migrate-tmp/\n.gitpass-migrating\n";
