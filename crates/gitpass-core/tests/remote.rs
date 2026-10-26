//! `remote add/show/remove` against a local bare repository in a tempdir. All data is FAKE.
use gitpass_core::crypto::*;
use gitpass_core::remote;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use std::path::Path;
use std::process::Command;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn item(t: &str) -> Item { Item { title: t.into(), username: Some("me".into()), password: Some("hunter2-FAKE".into()), url: None, notes: None, totp: None, updated_at: "1".into() } }

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn bare() -> tempfile::TempDir {
    let r = tempfile::tempdir().unwrap();
    git(r.path(), &["init", "--bare", "-q", "-b", "main"]);
    r
}
/// A fresh vault with one item, a secret.key, device files, and a stray backup dir.
fn vault() -> (tempfile::TempDir, SecretKey, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    std::fs::write(d.path().join("secret.key"), sk.encode()).unwrap();
    let s = Session::unlock_device(d.path(), PW, &sk, &opts(), Some("dev-a")).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    s.add_item(&vid, &item("GitHub")).unwrap();
    s.identity().unwrap(); // v4 identity: creates identity.enc/.pub and the local .gitpass-device-key.json
    drop(s);
    gitpass_core::fsutil::backup(d.path(), "").unwrap();
    (d, sk, vid)
}
fn remote_files(bare: &Path) -> Vec<String> {
    let mut v: Vec<String> = git(bare, &["ls-tree", "-r", "--name-only", "main"]).lines().map(String::from).collect();
    v.sort(); v
}
fn url(p: &Path) -> String { p.display().to_string() }

#[test]
fn connect_fresh_vault_pushes_only_encrypted_files() {
    let (d, sk, _) = vault();
    let r = bare();
    assert!(!d.path().join(".git").exists());
    let log = remote::add(d.path(), &url(r.path()), true).unwrap();
    assert!(log.contains("initialized git repository") && log.contains("pushed to origin/main"), "{log}");
    let files = remote_files(r.path());
    eprintln!("PUSHED: {files:#?}");
    let local: Vec<String> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    eprintln!("LOCAL TOP-LEVEL: {local:#?}");
    for f in &files { assert!(remote::is_allowed_path(f), "unexpected pushed file {f}"); }
    for secret in ["secret.key", ".gitpass-device-key.json", ".gitpass-device.json", ".gitpass-state.json", ".gitpass.lock"] {
        assert!(d.path().join(secret).exists(), "fixture should contain {secret}");
        assert!(!files.iter().any(|f| f == secret), "{secret} was pushed!");
    }
    assert!(!files.iter().any(|f| f.starts_with(".gitpass-")), "local-only file pushed: {files:?}");
    for must in [".gitignore", "format.json", "keyset.enc", "manifest.enc", "identity.enc", "identity.pub"] { assert!(files.iter().any(|f| f == must), "{must} missing: {files:?}"); }
    assert!(files.iter().any(|f| f.starts_with("vaults/") && f.ends_with(".enc")));
    // the pushed secret-looking value never appears in any blob
    let all = git(r.path(), &["log", "-p", "--all", "--text"]);
    assert!(!all.contains(&sk.encode()) && !all.contains("hunter2-FAKE"));
    let gi = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
    for n in remote::REQUIRED_IGNORES { assert!(gi.lines().any(|l| l == *n), "{n} not ignored"); }
    let info = remote::show(d.path()).unwrap();
    assert_eq!(info.url.as_deref(), Some(url(r.path()).as_str()));
    assert_eq!((info.ahead, info.behind, info.branch.as_deref()), (0, 0, Some("main")));
    // connecting twice is refused
    assert!(remote::add(d.path(), &url(r.path()), true).unwrap_err().to_string().starts_with(remote::E_ALREADY));
}

#[test]
fn second_device_clones_and_syncs() {
    let (a, sk, vid) = vault();
    let r = bare();
    remote::add(a.path(), &url(r.path()), true).unwrap();
    let b = tempfile::tempdir().unwrap();
    let bp = b.path().join("vault");
    let o = Command::new("git").args(["clone", "-q", &url(r.path()), bp.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success());
    assert!(!bp.join("secret.key").exists());
    let s = Session::unlock_device(&bp, PW, &sk, &opts(), Some("dev-b")).unwrap();
    assert_eq!(s.list_items(&vid).unwrap().len(), 1);
    s.add_item(&vid, &item("FromB")).unwrap();
    drop(s);
    let log = gitpass_core::sync::sync(&bp).unwrap();
    assert!(log.contains("pushed"), "{log}");
    let log = gitpass_core::sync::sync(a.path()).unwrap();
    assert!(log.contains("pushed"), "{log}");
    let s = Session::unlock_device(a.path(), PW, &sk, &opts(), Some("dev-a")).unwrap();
    let titles: Vec<String> = s.list_items(&vid).unwrap().into_iter().map(|(_, i)| i.title.clone()).collect();
    assert!(titles.contains(&"FromB".to_string()), "{titles:?}");
    for f in remote_files(r.path()) { assert!(remote::is_allowed_path(&f), "{f}"); }
    // B's local device files are not pushed either
    assert!(!remote_files(r.path()).iter().any(|f| f.contains("device-key") || f == "secret.key"));
}

#[test]
fn refuses_remote_with_other_account() {
    let (a, _, _) = vault();
    let r = bare();
    remote::add(a.path(), &url(r.path()), true).unwrap();
    let before = git(r.path(), &["rev-parse", "main"]);
    let (c, _, _) = vault();
    let e = remote::add(c.path(), &url(r.path()), true).unwrap_err().to_string();
    assert!(e.starts_with(remote::E_WRONG_ACCOUNT), "{e}");
    assert_eq!(git(r.path(), &["rev-parse", "main"]), before, "remote must be untouched");
    assert_eq!(remote::show(c.path()).unwrap().url, None, "origin must be rolled back");
}

#[test]
fn refuses_non_empty_non_gitpass_remote() {
    let r = bare();
    let w = tempfile::tempdir().unwrap();
    git(w.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(w.path().join("README.md"), "hi").unwrap();
    git(w.path(), &["add", "."]);
    git(w.path(), &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    git(w.path(), &["push", "-q", &url(r.path()), "main"]);
    let (a, _, _) = vault();
    let e = remote::add(a.path(), &url(r.path()), true).unwrap_err().to_string();
    assert!(e.starts_with(remote::E_NOT_EMPTY), "{e}");
    assert_eq!(remote_files(r.path()), vec!["README.md".to_string()]);
}

#[test]
fn aborts_when_plaintext_file_would_be_staged() {
    let (a, _, _) = vault();
    let r = bare();
    std::fs::write(a.path().join("passwords-export.csv"), "site,user,pass\nx,y,z\n").unwrap();
    let e = remote::add(a.path(), &url(r.path()), true).unwrap_err().to_string();
    assert!(e.starts_with(remote::E_SECRET_STAGED) && e.contains("passwords-export.csv"), "{e}");
    assert!(remote_files_opt(r.path()).is_empty(), "nothing may be pushed");
}
fn remote_files_opt(bare: &Path) -> Vec<String> {
    let o = Command::new("git").arg("-C").arg(bare).args(["ls-tree", "-r", "--name-only", "main"]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()
}

#[test]
fn remove_then_reconnect() {
    let (a, _, _) = vault();
    let r = bare();
    remote::add(a.path(), &url(r.path()), true).unwrap();
    remote::remove(a.path()).unwrap();
    let i = remote::show(a.path()).unwrap();
    assert!(i.is_repo && i.url.is_none());
    assert!(a.path().join("format.json").exists() && a.path().join("secret.key").exists(), "remove must not delete files");
    assert!(remote::remove(a.path()).is_err());
    // reconnecting to the same remote with the same history works (no force push)
    let log = remote::add(a.path(), &url(r.path()), true).unwrap();
    assert!(log.contains("remote already holds this account's vault"), "{log}");
    assert_eq!(remote::show(a.path()).unwrap().url.as_deref(), Some(url(r.path()).as_str()));
}

#[test]
fn invalid_url_rejected_without_touching_vault() {
    let (a, _, _) = vault();
    let e = remote::add(a.path(), "https://ghp_TOKEN@github.com/me/v.git", false).unwrap_err().to_string();
    assert!(e.starts_with(remote::E_BAD_URL), "{e}");
    assert!(!a.path().join(".git").exists());
}
