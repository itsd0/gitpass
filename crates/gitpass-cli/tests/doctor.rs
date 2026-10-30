//! Integration tests for `gitpass doctor`.
//! Tests vault file decryption, permission auditing, git state, browser integration,
//! local throttle/backup state, exit codes (0 = ok, 1 = fail, 2 = warn),
//! and confirms that no secrets or password values are ever printed or returned in JSON.
//! All test data is FAKE.

use gitpass_core::crypto::SecretKey;
use gitpass_core::store::Session;
use serde_json::Value;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FAKE_MASTER_PW: &str = "fake-master-pw-123";
const SECRET_LOGIN_PW: &str = "FAKE-secret-login-pw-999!";

fn gp(root: &Path, home: &Path, args: &[&str], stdin: Option<&str>, envs: &[(&str, &str)]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args)
        .env("HOME", home)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", FAKE_MASTER_PW)
        .env("GITPASS_DEVICE_NAME", "test-device")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        c.env(k, v);
    }
    let mut ch = c.spawn().unwrap();
    if let Some(s) = stdin {
        let mut i = ch.stdin.take().unwrap();
        i.write_all(s.as_bytes()).unwrap();
    }
    ch.wait_with_output().unwrap()
}

struct TestVault {
    _dir: tempfile::TempDir,
    root: PathBuf,
    _item_id: String,
}

fn setup_vault() -> TestVault {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    fs::create_dir_all(&root).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

    let home = dir.path();
    let out = gp(&root, home, &["init", "--save-secret-key"], None, &[]);
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));

    let out = gp(&root, home, &["vault", "create", "Personal"], None, &[]);
    assert!(out.status.success(), "vault create failed");

    let out = gp(
        &root,
        home,
        &["add", "Personal", "--title", "Example Login", "--username", "alice", "--url", "https://example.com", "--password-stdin"],
        Some(&format!("{SECRET_LOGIN_PW}
")),
        &[],
    );
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));

    let sk = SecretKey::parse(&fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, FAKE_MASTER_PW, &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();
    let items = s.list_items(&vid).unwrap();
    let item_id = items[0].0.clone();

    TestVault {
        _dir: dir,
        root,
        _item_id: item_id,
    }
}

fn setup_git_repo_with_remote(root: &Path, home: &Path) -> tempfile::TempDir {
    let bare_dir = tempfile::tempdir().unwrap();
    let git = |dir: &Path, args: &[&str]| {
        let o = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&o.stderr));
    };
    git(bare_dir.path(), &["init", "--bare", "-q", "-b", "main"]);

    let remote_url = format!("file://{}", bare_dir.path().display());
    let out = gp(root, home, &["remote", "add", &remote_url, "--allow-local"], None, &[]);
    assert!(out.status.success(), "remote add failed: {}", String::from_utf8_lossy(&out.stderr));

    bare_dir
}

#[test]
fn test_doctor_clean_vault_json_and_text() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Test text output
    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(0), "expected exit code 0; stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✓] Vault files:"), "{stdout}");
    assert!(stdout.contains("[✓] Permissions:"), "{stdout}");
    assert!(stdout.contains("[✓] Git repository:"), "{stdout}");
    assert!(stdout.contains("All 5 checks passed."), "{stdout}");
    assert!(!stdout.contains(SECRET_LOGIN_PW), "leaked secret in stdout");

    // Test JSON output
    let out = gp(&tv.root, home, &["doctor", "--json"], None, &[]);
    assert_eq!(out.status.code(), Some(0));
    let json: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["status"], "ok");
    let checks = json["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 5);
    for c in checks {
        assert_eq!(c["status"], "ok", "check {} was not ok: {}", c["name"], c["message"]);
    }
    let vault_files = &checks[0];
    assert!(vault_files["items_checked"].as_u64().unwrap() >= 1);
    assert!(vault_files["items_failed"].as_array().unwrap().is_empty());
    assert!(!String::from_utf8_lossy(&out.stdout).contains(SECRET_LOGIN_PW));
}

#[test]
fn test_doctor_corrupted_item_file_fails() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Locate the encrypted item file dynamically
    let mut item_path = None;
    for v_entry in fs::read_dir(tv.root.join("vaults")).unwrap().flatten() {
        let items_dir = v_entry.path().join("items");
        if items_dir.exists() {
            for i_entry in fs::read_dir(items_dir).unwrap().flatten() {
                if i_entry.path().extension().and_then(|s| s.to_str()) == Some("enc") {
                    item_path = Some(i_entry.path());
                    break;
                }
            }
        }
    }
    let item_enc = item_path.expect("found item enc file");
    let item_id = item_enc.file_stem().unwrap().to_string_lossy().to_string();

    // Deliberately corrupt the item file
    fs::write(&item_enc, br#"{"version":1,"envelope":{"tag":"bad","nonce":"bad","ciphertext":"corrupted"}}"#).unwrap();

    // Test text output: exit code 1
    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(1), "expected exit code 1; stdout: {}", String::from_utf8_lossy(&out.stdout));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✗] Vault files:"), "{stdout}");
    assert!(stdout.contains(&format!("failing item: {}", item_id)), "{stdout}");
    assert!(!stdout.contains(SECRET_LOGIN_PW));

    // Test JSON output: exit code 1, check vault_files has fail status
    let out = gp(&tv.root, home, &["doctor", "--json"], None, &[]);
    assert_eq!(out.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["status"], "fail");
    let checks = json["checks"].as_array().unwrap();
    let vault_files = checks.iter().find(|c| c["name"] == "vault_files").unwrap();
    assert_eq!(vault_files["status"], "fail");
    let failed_items = vault_files["items_failed"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v.as_str() == Some(&item_id)));
    assert!(!String::from_utf8_lossy(&out.stdout).contains(SECRET_LOGIN_PW));
}

#[cfg(unix)]
#[test]
fn test_doctor_0644_key_file_fails() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    let key_path = tv.root.join("secret.key");
    assert!(key_path.exists());
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644)).unwrap();

    // Text output: exit code 1, reports 0644 key file
    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(1), "expected exit code 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✗] Permissions:"), "{stdout}");
    assert!(stdout.contains("0644"), "{stdout}");
    assert!(stdout.contains("chmod 600"), "{stdout}");

    // JSON output: exit code 1
    let out = gp(&tv.root, home, &["doctor", "--json"], None, &[]);
    assert_eq!(out.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["status"], "fail");
    let permissions = json["checks"].as_array().unwrap().iter().find(|c| c["name"] == "permissions").unwrap();
    assert_eq!(permissions["status"], "fail");
}

#[test]
fn test_doctor_no_unlock_skips_item_decryption() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Pass --no-unlock and unset password
    let out = gp(&tv.root, home, &["doctor", "--no-unlock"], None, &[("GITPASS_PASSWORD", "")]);
    assert_eq!(out.status.code(), Some(2), "expected exit code 2 (warnings only); stdout: {}", String::from_utf8_lossy(&out.stdout));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[!] Vault files:"), "{stdout}");
    assert!(stdout.contains("--no-unlock"), "{stdout}");

    // JSON output with --no-unlock
    let out = gp(&tv.root, home, &["doctor", "--no-unlock", "--json"], None, &[("GITPASS_PASSWORD", "")]);
    assert_eq!(out.status.code(), Some(2));
    let json: Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["status"], "warn");
    let vault_files = json["checks"].as_array().unwrap().iter().find(|c| c["name"] == "vault_files").unwrap();
    assert_eq!(vault_files["status"], "warn");
}

#[test]
fn test_doctor_uncommitted_git_changes_warns() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Create an uncommitted file
    fs::write(tv.root.join("uncommitted.txt"), b"uncommitted data").unwrap();

    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(2), "expected exit code 2 for warnings");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[!] Git repository:"), "{stdout}");
    assert!(stdout.contains("uncommitted change(s)"), "{stdout}");
}

#[test]
fn test_doctor_conflicted_git_state_fails() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Simulate merge conflict via MERGE_HEAD
    fs::write(tv.root.join(".git").join("MERGE_HEAD"), b"deadbeef").unwrap();

    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(1), "expected exit code 1 for conflicts");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✗] Git repository:"), "{stdout}");
    assert!(stdout.contains("unresolved merge conflicts"), "{stdout}");
}

#[test]
fn test_doctor_throttled_state_warns() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Write a throttled record with retry_after in the future
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let rec = serde_json::json!({
        "failures": 3,
        "retry_after": now + 200,
        "updated_at": now,
    });
    fs::write(tv.root.join(".gitpass-throttle.json"), serde_json::to_vec_pretty(&rec).unwrap()).unwrap();

    let out = gp(&tv.root, home, &["doctor", "--no-unlock"], None, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[!] Local state: vault unlock currently throttled:"), "{stdout}");
}

#[test]
fn test_doctor_corrupted_throttle_file_fails() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // Write corrupted data into throttle file
    fs::write(tv.root.join(".gitpass-throttle.json"), b"{ not valid json }").unwrap();

    let out = gp(&tv.root, home, &["doctor", "--no-unlock"], None, &[]);
    assert_eq!(out.status.code(), Some(1), "expected exit code 1 for corrupted throttle");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✗] Local state:"), "{stdout}");
    assert!(stdout.contains("corrupted throttle file"), "{stdout}");
}

#[test]
fn test_doctor_browser_integration_checks() {
    let tv = setup_vault();
    let home = tv._dir.path();
    let _remote = setup_git_repo_with_remote(&tv.root, home);

    // 1. Simulate installed Chrome with missing manifest: should warn
    #[cfg(target_os = "macos")]
    let chrome_dir = home.join("Library/Application Support/Google/Chrome");
    #[cfg(not(target_os = "macos"))]
    let chrome_dir = home.join(".config/google-chrome");
    fs::create_dir_all(&chrome_dir).unwrap();

    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(2), "expected exit code 2 for missing manifest warning");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[!] Browser integration:"), "{stdout}");
    assert!(stdout.contains("manifest missing"), "{stdout}");

    // 2. Manifest pointing to missing binary: should fail
    let nm_dir = chrome_dir.join("NativeMessagingHosts");
    fs::create_dir_all(&nm_dir).unwrap();
    let manifest_file = nm_dir.join("com.gitpass.host.json");
    fs::write(&manifest_file, br#"{"name":"com.gitpass.host","path":"/nonexistent/gitpass-host"}"#).unwrap();

    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(1), "expected exit code 1 for missing host binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✗] Browser integration:"), "{stdout}");
    assert!(stdout.contains("missing binary"), "{stdout}");

    // 3. Manifest pointing to existing binary: should be ok
    let fake_bin = home.join("fake-gitpass-host");
    fs::write(&fake_bin, b"#!/bin/sh
exit 0
").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&fake_bin, fs::Permissions::from_mode(0o755)).unwrap();

    let manifest_content = serde_json::json!({
        "name": "com.gitpass.host",
        "path": fake_bin.to_string_lossy(),
    });
    fs::write(&manifest_file, serde_json::to_vec_pretty(&manifest_content).unwrap()).unwrap();

    let out = gp(&tv.root, home, &["doctor"], None, &[]);
    assert_eq!(out.status.code(), Some(0), "expected exit code 0 when manifest valid");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[✓] Browser integration:"), "{stdout}");
}
