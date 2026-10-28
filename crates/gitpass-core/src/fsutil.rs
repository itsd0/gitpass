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

pub const GITIGNORE: &str = "secret.key\n*.tmp\n.gitpass.lock\n.gitpass-state.json\n.gitpass-device.json\n.gitpass-device-key.json\n.gitpass-revoke-pending.json\n.gitpass-backup-*/\n.gitpass-migrate-tmp/\n.gitpass-migrating\n.gitpass-settings.json\n*.new\n.DS_Store\n";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn atomic_write_creates_file_and_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("deep").join("test.txt");
        let data = b"hello atomic write";

        atomic_write(&target, data).unwrap();

        assert_eq!(fs::read(&target).unwrap(), data);
        let parent = target.parent().unwrap();
        let tmp_files: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(tmp_files.is_empty(), "temporary file left behind: {tmp_files:?}");
    }

    #[test]
    fn atomic_write_overwrites_existing_file_without_leaving_temps() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("file.txt");

        atomic_write(&target, b"initial content").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"initial content");

        atomic_write(&target, b"overwritten content").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"overwritten content");

        let tmp_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(tmp_files.is_empty(), "temporary file left behind: {tmp_files:?}");
    }

    #[test]
    fn atomic_write_fails_for_empty_path() {
        let res = atomic_write(Path::new(""), b"data");
        assert!(res.is_err());
    }

    #[test]
    fn rename_moves_file_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");

        fs::write(&from, b"data to rename").unwrap();
        rename(&from, &to).unwrap();

        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"data to rename");
    }

    #[test]
    fn is_local_only_identifies_excluded_and_allowed_names() {
        // Excluded
        assert!(is_local_only(".git"));
        assert!(is_local_only(".gitpass.lock"));
        assert!(is_local_only(".gitpass-migrate-tmp"));
        assert!(is_local_only(".gitpass-backup-123456789"));
        assert!(is_local_only(".gitpass-backup-v1-20261028"));
        assert!(is_local_only(".gitpass-backup-"));

        // Allowed
        assert!(!is_local_only("vault.json"));
        assert!(!is_local_only("items"));
        assert!(!is_local_only("secret.key"));
        assert!(!is_local_only(".gitpass-settings.json"));
        assert!(!is_local_only(".gitkeep"));
        assert!(!is_local_only("backup"));
        assert!(!is_local_only("my.git"));
    }

    #[test]
    fn copy_dir_copies_nested_and_applies_skip_only_at_top_level() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");

        // Top level entries
        fs::create_dir_all(src.join("skip_me")).unwrap();
        fs::write(src.join("skip_me").join("inner.txt"), b"skipped").unwrap();

        fs::create_dir_all(src.join("keep_me").join("skip_me")).unwrap();
        fs::write(src.join("keep_me").join("skip_me").join("nested.txt"), b"nested content").unwrap();
        fs::write(src.join("keep_me").join("plain.txt"), b"plain content").unwrap();

        copy_dir(&src, &dst, &|name| name == "skip_me").unwrap();

        // Top-level skip_me was skipped
        assert!(!dst.join("skip_me").exists());

        // Nested skip_me inside keep_me was kept (skip only applies at top level)
        assert_eq!(
            fs::read(dst.join("keep_me").join("skip_me").join("nested.txt")).unwrap(),
            b"nested content"
        );
        assert_eq!(
            fs::read(dst.join("keep_me").join("plain.txt")).unwrap(),
            b"plain content"
        );
    }

    #[test]
    fn backup_excludes_local_only_entries_and_earlier_backups() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("vault");
        fs::create_dir_all(&root).unwrap();

        // Regular vault data
        fs::write(root.join("vault.json"), b"{\"id\":\"v1\"}").unwrap();
        fs::create_dir_all(root.join("items")).unwrap();
        fs::write(root.join("items").join("login1.json"), b"{\"title\":\"example\"}").unwrap();

        // Entries that must be excluded
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("config"), b"git config").unwrap();
        fs::write(root.join(".gitpass.lock"), b"").unwrap();
        fs::create_dir_all(root.join(".gitpass-migrate-tmp")).unwrap();
        fs::write(root.join(".gitpass-migrate-tmp").join("tmp.json"), b"tmp").unwrap();
        fs::create_dir_all(root.join(".gitpass-backup-old123")).unwrap();
        fs::write(root.join(".gitpass-backup-old123").join("old.json"), b"old").unwrap();

        let b = backup(&root, "test-").unwrap();
        assert!(b.file_name().unwrap().to_string_lossy().starts_with(".gitpass-backup-test-"));

        // Regular items copied
        assert_eq!(fs::read(b.join("vault.json")).unwrap(), b"{\"id\":\"v1\"}");
        assert_eq!(
            fs::read(b.join("items").join("login1.json")).unwrap(),
            b"{\"title\":\"example\"}"
        );

        // Excluded items NOT copied into backup
        assert!(!b.join(".git").exists());
        assert!(!b.join(".gitpass.lock").exists());
        assert!(!b.join(".gitpass-migrate-tmp").exists());
        assert!(!b.join(".gitpass-backup-old123").exists());
    }

    #[test]
    fn ts_format_and_lexicographic_chronological_ordering() {
        let t1 = ts();
        thread::sleep(Duration::from_millis(2));
        let t2 = ts();

        // Must match {secs}-{nanos:09}
        let parts1: Vec<&str> = t1.split('-').collect();
        assert_eq!(parts1.len(), 2);
        assert_eq!(parts1[1].len(), 9);
        assert!(parts1[0].chars().all(|c| c.is_ascii_digit()));
        assert!(parts1[1].chars().all(|c| c.is_ascii_digit()));

        // Chronological order matches lexicographical sort
        assert!(t1 < t2, "expected {t1} < {t2}");

        // Padded nanos ensure lexicographical sort across nanos/seconds
        let sample_a = "100-000000001";
        let sample_b = "100-000000009";
        let sample_c = "100-999999999";
        let sample_d = "101-000000000";
        assert!(sample_a < sample_b);
        assert!(sample_b < sample_c);
        assert!(sample_c < sample_d);
    }

    #[test]
    fn prune_backups_keeps_newest_routine_and_never_deletes_migration_or_unrelated() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // 5 routine backups (digits after prefix)
        let routine_names = [
            ".gitpass-backup-1000",
            ".gitpass-backup-2000",
            ".gitpass-backup-3000",
            ".gitpass-backup-4000",
            ".gitpass-backup-5000",
        ];
        for name in routine_names {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("data.txt"), name.as_bytes()).unwrap();
        }

        // 1 v1 migration backup (non-digit prefix after .gitpass-backup-)
        let v1_backup = ".gitpass-backup-v1-migration-backup";
        fs::create_dir_all(root.join(v1_backup)).unwrap();
        fs::write(root.join(v1_backup).join("v1.txt"), b"v1").unwrap();

        // Unrelated directory and file
        fs::create_dir_all(root.join("unrelated_folder")).unwrap();
        fs::write(root.join("unrelated_file.txt"), b"plain").unwrap();

        // Prune to keep 3 newest routine backups
        prune_backups(root, 3).unwrap();

        // Oldest two routine backups (1000 and 2000) are removed
        assert!(!root.join(".gitpass-backup-1000").exists());
        assert!(!root.join(".gitpass-backup-2000").exists());

        // 3 newest routine backups are kept
        assert!(root.join(".gitpass-backup-3000").exists());
        assert!(root.join(".gitpass-backup-4000").exists());
        assert!(root.join(".gitpass-backup-5000").exists());

        // Migration backup is NEVER deleted
        assert!(root.join(v1_backup).exists());

        // Unrelated directory and file are untouched
        assert!(root.join("unrelated_folder").exists());
        assert!(root.join("unrelated_file.txt").exists());

        // Edge case: keep more than existing does nothing
        prune_backups(root, 10).unwrap();
        assert!(root.join(".gitpass-backup-3000").exists());
        assert!(root.join(".gitpass-backup-4000").exists());
        assert!(root.join(".gitpass-backup-5000").exists());

        // Edge case: keep 0 removes all routine backups but preserves migration backup
        prune_backups(root, 0).unwrap();
        assert!(!root.join(".gitpass-backup-3000").exists());
        assert!(!root.join(".gitpass-backup-4000").exists());
        assert!(!root.join(".gitpass-backup-5000").exists());
        assert!(root.join(v1_backup).exists());
        assert!(root.join("unrelated_folder").exists());
    }

    #[test]
    fn lock_blocks_concurrent_access_and_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        // Main thread acquires advisory lock
        let guard = lock(&root).unwrap();

        let lock_file_path = root.join(".gitpass.lock");
        assert!(lock_file_path.exists());

        // 1. Thread attempting try_lock_exclusive while guard is alive fails immediately
        let (tx_try, rx_try) = mpsc::channel();
        let path_clone = lock_file_path.clone();
        thread::spawn(move || {
            let f = fs::OpenOptions::new().write(true).open(&path_clone).unwrap();
            let is_locked = f.try_lock_exclusive().is_err();
            tx_try.send(is_locked).unwrap();
        });
        let blocked = rx_try.recv_timeout(Duration::from_secs(3)).expect("try_lock timeout");
        assert!(blocked, "expected try_lock_exclusive to fail while lock is held");

        // 2. Thread waiting on lock(&root) blocks until guard is dropped
        let (tx_ready, rx_ready) = mpsc::channel();
        let (tx_done, rx_done) = mpsc::channel();
        let root_clone = root.clone();
        let waiter_handle = thread::spawn(move || {
            tx_ready.send(()).unwrap();
            let _second_guard = lock(&root_clone).unwrap();
            tx_done.send(()).unwrap();
        });

        rx_ready.recv_timeout(Duration::from_secs(3)).expect("waiter thread startup timeout");

        // Ensure waiter is blocked while guard is held
        thread::sleep(Duration::from_millis(50));
        assert!(
            rx_done.try_recv().is_err(),
            "waiter acquired lock prematurely before guard was dropped"
        );

        // Drop guard to release lock
        drop(guard);

        // Waiter thread should now unblock and acquire the lock
        rx_done
            .recv_timeout(Duration::from_secs(3))
            .expect("waiter failed to acquire lock after drop");
        waiter_handle.join().unwrap();
    }
}
