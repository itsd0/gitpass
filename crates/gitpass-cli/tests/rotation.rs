//! `gitpass edit --rotate-every` and the `rotation_due` / `rotation_soon` audit findings on a temp
//! vault. All data FAKE.
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args).env("GITPASS_PATH", root).env("GITPASS_PASSWORD", "fake-master-pw-123").env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut ch = c.spawn().unwrap();
    { let mut i = ch.stdin.take().unwrap(); i.write_all(b"").unwrap(); }
    ch.wait_with_output().unwrap()
}

fn audit_json(root: &Path) -> serde_json::Value {
    let out = gp(root, &["audit", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn edit_rotate_every_sets_clears_and_drives_audit_findings() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"]).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"]).status.success());
    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let changed = now - 100 * 86400; // password last changed 100 days ago
    let mut it = Item::default();
    it.title = "FAKE Bank".into();
    it.username = Some("alice@example.invalid".into());
    it.password = Some("FAKE-Rotation-Pass-9f3Kd!x".into());
    it.url = Some("https://bank.example.invalid".into());
    it.updated_at = changed.to_string();
    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(changed);
    let id = s.add_record(&vid, rec).unwrap();

    // unset by default: no rotation findings
    let r = audit_json(&root);
    assert_eq!((r["rotation_due"].as_u64(), r["rotation_soon"].as_u64()), (Some(0), Some(0)));

    // an unsupported interval is rejected and changes nothing
    let bad = gp(&root, &["edit", "Personal", &id, "--rotate-every", "45d"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--rotate-every must be"));

    // 90d on a 100-day-old password: due
    let ok = gp(&root, &["edit", "Personal", &id, "--rotate-every", "90d"]);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    let r = audit_json(&root);
    assert_eq!(r["rotation_due"], 1);
    let kinds: Vec<String> = r["per_item"][0]["findings"].as_array().unwrap().iter().filter_map(|f| f.get("rotation_due").map(|_| "rotation_due".to_string())).collect();
    assert_eq!(kinds, vec!["rotation_due"]);
    let text = String::from_utf8_lossy(&gp(&root, &["audit"]).stdout).to_string();
    assert!(text.contains("due for rotation: password is 100 days old (reminder every 90 days)"), "{text}");
    let get = String::from_utf8_lossy(&gp(&root, &["get", "Personal", &id]).stdout).to_string();
    assert!(get.contains("rotate every: 90 days"), "{get}");
    let get_json: serde_json::Value = serde_json::from_slice(&gp(&root, &["get", "Personal", &id, "--json"]).stdout).unwrap();
    assert_eq!(get_json["rotate_every_days"], 90);

    // 180d: 80 days left => soon
    assert!(gp(&root, &["edit", "Personal", &id, "--rotate-every", "180"]).status.success());
    let r = audit_json(&root);
    assert_eq!((r["rotation_due"].as_u64(), r["rotation_soon"].as_u64()), (Some(0), Some(0)), "80 days left is outside the 14-day window");

    // clearing the reminder removes the field again
    assert!(gp(&root, &["edit", "Personal", &id, "--rotate-every", "none"]).status.success());
    let r = audit_json(&root);
    assert_eq!((r["rotation_due"].as_u64(), r["rotation_soon"].as_u64()), (Some(0), Some(0)));
    let get_json: serde_json::Value = serde_json::from_slice(&gp(&root, &["get", "Personal", &id, "--json"]).stdout).unwrap();
    assert!(get_json.get("rotate_every_days").is_none());
}

#[test]
fn rotation_soon_within_two_weeks_of_the_deadline() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"]).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"]).status.success());
    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let changed = now - 25 * 86400; // 25 days old, 30-day reminder => 5 days left
    let mut it = Item::default();
    it.title = "FAKE Email".into();
    it.password = Some("FAKE-Rotation-Pass-2q8Wm!z".into());
    it.url = Some("https://mail.example.invalid".into());
    it.updated_at = changed.to_string();
    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(changed);
    rec.rotate_every_days = Some(30);
    s.add_record(&vid, rec).unwrap();

    let r = audit_json(&root);
    assert_eq!((r["rotation_due"].as_u64(), r["rotation_soon"].as_u64()), (Some(0), Some(1)));
    let text = String::from_utf8_lossy(&gp(&root, &["audit"]).stdout).to_string();
    assert!(text.contains("rotate in 5 days (reminder every 30 days)"), "{text}");
}
