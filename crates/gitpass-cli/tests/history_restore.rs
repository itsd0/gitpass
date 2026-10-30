//! Integration tests for `gitpass history` and `gitpass restore`.
//! All test data is FAKE.

use gitpass_core::crypto::SecretKey;
use gitpass_core::item::Category;
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut ch = c.spawn().unwrap();
    {
        let mut i = ch.stdin.take().unwrap();
        if let Some(s) = stdin {
            i.write_all(s.as_bytes()).unwrap();
        }
    }
    ch.wait_with_output().unwrap()
}

const SECRET_V1: &str = "FAKE-pw-v1-secret";
const SECRET_V2: &str = "FAKE-pw-v2-secret";

#[test]
fn history_and_restore_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Example Site".into();
    it.username = Some("alice@example.invalid".into());
    it.password = Some(SECRET_V1.into());
    let mut r1 = ItemRecord::new(it);
    r1.category = Category::Login;
    let item_id = s.add_record(&vid, r1).unwrap();

    // Make an update (version 2)
    let (_, mut r2) = s.read_record(&vid, &item_id).unwrap();
    r2.item.title = "Example Site Updated".into();
    r2.item.password = Some(SECRET_V2.into());
    s.update_record(&vid, &item_id, r2).unwrap();

    // 1. gitpass history lists versions without secrets
    let out = gp(&root, &["history", "Example Site Updated"], None);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("version 2 (current)"), "stdout: {stdout}");
    assert!(stdout.contains("version 1"), "stdout: {stdout}");
    assert!(!stdout.contains(SECRET_V1), "secret v1 leaked in history output");
    assert!(!stdout.contains(SECRET_V2), "secret v2 leaked in history output");

    // 2. gitpass history --json produces valid JSON
    let out_json = gp(&root, &["history", &item_id, "--json"], None);
    assert!(out_json.status.success());
    let json_text = String::from_utf8_lossy(&out_json.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_text).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["version"], 2);
    assert_eq!(parsed[0]["current"], true);
    assert_eq!(parsed[1]["version"], 1);
    assert_eq!(parsed[1]["current"], false);
    assert!(!json_text.contains(SECRET_V1));
    assert!(!json_text.contains(SECRET_V2));

    // 3. gitpass restore --version 1 restores older snapshot
    let out_restore = gp(&root, &["restore", &item_id, "--version", "1"], None);
    assert!(out_restore.status.success(), "stderr: {}", String::from_utf8_lossy(&out_restore.stderr));
    let restore_msg = String::from_utf8_lossy(&out_restore.stdout);
    assert!(restore_msg.contains("restored"), "stdout: {restore_msg}");

    // Verify restored content using `gitpass get --json --reveal`
    let out_get = gp(&root, &["get", "Personal", &item_id, "--json", "--reveal"], None);
    assert!(out_get.status.success());
    let get_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_get.stdout)).unwrap();
    assert_eq!(get_json["title"], "Example Site");
    let pw_field = get_json["fields"].as_array().unwrap().iter().find(|f| f["label"] == "password").unwrap();
    assert_eq!(pw_field["value"], SECRET_V1);

    // Verify history now includes version 3 (current) and pre-restore states
    let out_hist2 = gp(&root, &["history", &item_id, "--json"], None);
    assert!(out_hist2.status.success());
    let parsed2: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_hist2.stdout)).unwrap();
    assert_eq!(parsed2.len(), 3);
    assert_eq!(parsed2[0]["version"], 3);
    assert_eq!(parsed2[0]["current"], true);

    // 4. Unknown version returns error
    let out_err = gp(&root, &["restore", &item_id, "--version", "999"], None);
    assert!(!out_err.status.success());
    assert!(String::from_utf8_lossy(&out_err.stderr).contains("no version 999 in item history"));

    // 5. Soft-deleted item returns error
    let s2 = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    s2.soft_delete(&vid, &item_id).unwrap();
    let out_del = gp(&root, &["restore", &item_id, "--version", "1", "--vault", "Personal"], None);
    assert!(!out_del.status.success());
    assert!(String::from_utf8_lossy(&out_del.stderr).contains("cannot restore version of deleted item"));
}

#[test]
fn show_password_history_masked_and_revealed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Test Service".into();
    it.username = Some("bob@example.invalid".into());
    it.password = Some("pw-initial-secret".into());
    let mut r1 = ItemRecord::new(it);
    r1.category = Category::Login;
    let item_id = s.add_record(&vid, r1).unwrap();

    // Update password to trigger history entry
    let (_, mut r2) = s.read_record(&vid, &item_id).unwrap();
    r2.item.password = Some("pw-updated-current".into());
    s.update_record(&vid, &item_id, r2).unwrap();

    // 1. gitpass show <item> --password-history lists masked values by default
    let out_masked = gp(&root, &["show", &item_id, "--password-history"], None);
    assert!(out_masked.status.success(), "stderr: {}", String::from_utf8_lossy(&out_masked.stderr));
    let masked_str = String::from_utf8_lossy(&out_masked.stdout);
    assert!(masked_str.contains("••••••••••"), "stdout: {masked_str}");
    assert!(!masked_str.contains("pw-initial-secret"), "should mask secrets");

    // 2. gitpass show <item> --password-history --reveal shows the real historical password
    let out_reveal = gp(&root, &["show", &item_id, "--password-history", "--reveal"], None);
    assert!(out_reveal.status.success(), "stderr: {}", String::from_utf8_lossy(&out_reveal.stderr));
    let reveal_str = String::from_utf8_lossy(&out_reveal.stdout);
    assert!(reveal_str.contains("pw-initial-secret"), "stdout: {reveal_str}");

    // 3. gitpass show <item> --password-history --json
    let out_json = gp(&root, &["show", &item_id, "--password-history", "--json"], None);
    assert!(out_json.status.success(), "stderr: {}", String::from_utf8_lossy(&out_json.stderr));
    let json_val: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(json_val.len(), 1);
    assert_eq!(json_val[0]["password"], "••••••••••");
    assert!(!json_val[0]["replaced_at"].as_str().unwrap().is_empty());

    // 4. gitpass show <item> --password-history --json --reveal
    let out_json_rev = gp(&root, &["show", &item_id, "--password-history", "--json", "--reveal"], None);
    assert!(out_json_rev.status.success());
    let json_rev_val: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_json_rev.stdout)).unwrap();
    assert_eq!(json_rev_val[0]["password"], "pw-initial-secret");

    // 5. gitpass get Personal <item> --password-history also works
    let out_get_hist = gp(&root, &["get", "Personal", &item_id, "--password-history", "--reveal"], None);
    assert!(out_get_hist.status.success());
    assert!(String::from_utf8_lossy(&out_get_hist.stdout).contains("pw-initial-secret"));
}
