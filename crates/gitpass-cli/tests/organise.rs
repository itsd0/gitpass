//! Integration tests for item organisation CLI commands:
//! `edit` (--tag, --untag, --favorite, --unfavorite), `archive`, `unarchive`, `undelete`, and `mv`.
//! Tests use temp vaults and temp homes only. All test data is FAKE.

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

const SECRET_PW: &str = "FAKE-super-secret-password-123";

fn setup_two_vaults() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Work"], None).status.success());
    (dir, root)
}

#[test]
fn edit_tags_and_favorite_workflow() {
    let (_dir, root) = setup_two_vaults();
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Example Login".into();
    it.username = Some("alice@example.invalid".into());
    it.password = Some(SECRET_PW.into());
    let mut r = ItemRecord::new(it);
    r.category = Category::Login;
    r.tags = vec!["initial-tag".into()];
    let id = s.add_record(&vid, r).unwrap();

    // 1. Add repeatable tags and set favorite using positional vault + id
    let out1 = gp(
        &root,
        &["edit", "Personal", &id, "--tag", "prod", "--tag", "infra", "--favorite"],
        None,
    );
    assert!(out1.status.success(), "stderr: {}", String::from_utf8_lossy(&out1.stderr));
    assert!(String::from_utf8_lossy(&out1.stdout).contains(&format!("updated {id}")));

    // Verify via get --json
    let out_json1 = gp(&root, &["get", "Personal", &id, "--json"], None);
    assert!(out_json1.status.success());
    let d1: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json1.stdout)).unwrap();
    assert_eq!(d1["favorite"], true);
    let tags1: Vec<String> = d1["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(tags1.contains(&"initial-tag".to_string()));
    assert!(tags1.contains(&"prod".to_string()));
    assert!(tags1.contains(&"infra".to_string()));
    // Secrets must NEVER appear in stdout
    assert!(!String::from_utf8_lossy(&out1.stdout).contains(SECRET_PW));
    assert!(!String::from_utf8_lossy(&out_json1.stdout).contains(SECRET_PW));

    // 2. Remove tags and unfavorite using single item query with --vault and --json output
    let out2 = gp(
        &root,
        &["edit", "Example Login", "--vault", "Personal", "--untag", "initial-tag", "--untag", "infra", "--unfavorite", "--json"],
        None,
    );
    assert!(out2.status.success(), "stderr: {}", String::from_utf8_lossy(&out2.stderr));
    let d2: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out2.stdout)).unwrap();
    assert_eq!(d2["favorite"], false);
    let tags2: Vec<String> = d2["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(tags2, vec!["prod".to_string()]);
    assert!(!String::from_utf8_lossy(&out2.stdout).contains(SECRET_PW));
}

#[test]
fn archive_and_unarchive_workflow() {
    let (_dir, root) = setup_two_vaults();
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Old Service".into();
    it.password = Some(SECRET_PW.into());
    let mut r = ItemRecord::new(it);
    r.category = Category::Login;
    let id = s.add_record(&vid, r).unwrap();

    // 1. Archive the item
    let out_arch = gp(&root, &["archive", "Old Service", "--vault", "Personal"], None);
    assert!(out_arch.status.success(), "stderr: {}", String::from_utf8_lossy(&out_arch.stderr));
    assert!(String::from_utf8_lossy(&out_arch.stdout).contains(&format!("archived {id}")));

    // 2. Item is hidden from regular list unless --include-archived
    let out_list1 = gp(&root, &["list", "Personal", "--json"], None);
    assert!(out_list1.status.success());
    let arr1: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_list1.stdout)).unwrap();
    assert!(!arr1.iter().any(|i| i["id"] == id));

    let out_list2 = gp(&root, &["list", "Personal", "--include-archived", "--json"], None);
    assert!(out_list2.status.success());
    let arr2: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_list2.stdout)).unwrap();
    assert!(arr2.iter().any(|i| i["id"] == id));

    // 3. Unarchive the item with --json
    let out_unarch = gp(&root, &["unarchive", &id, "--vault", "Personal", "--json"], None);
    assert!(out_unarch.status.success(), "stderr: {}", String::from_utf8_lossy(&out_unarch.stderr));
    let d: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_unarch.stdout)).unwrap();
    assert_eq!(d["archived"], false);

    // 4. Unarchive on an item that is NOT archived returns a clear error
    let out_unarch_err = gp(&root, &["unarchive", &id, "--vault", "Personal"], None);
    assert!(!out_unarch_err.status.success());
    assert!(String::from_utf8_lossy(&out_unarch_err.stderr).contains("is not archived"));

    // Check secrecy
    assert!(!String::from_utf8_lossy(&out_arch.stdout).contains(SECRET_PW));
    assert!(!String::from_utf8_lossy(&out_unarch.stdout).contains(SECRET_PW));
}

#[test]
fn undelete_workflow() {
    let (_dir, root) = setup_two_vaults();
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it1 = Item::default();
    it1.title = "Active Note".into();
    it1.password = Some(SECRET_PW.into());
    let mut r1 = ItemRecord::new(it1);
    r1.category = Category::SecureNote;
    let id_active = s.add_record(&vid, r1).unwrap();

    let mut it2 = Item::default();
    it2.title = "Deleted Login".into();
    it2.password = Some(SECRET_PW.into());
    let mut r2 = ItemRecord::new(it2);
    r2.category = Category::Login;
    let id_deleted = s.add_record(&vid, r2).unwrap();
    s.soft_delete(&vid, &id_deleted).unwrap();

    // 1. Attempting to undelete an active item produces a clear error
    let out_err = gp(&root, &["undelete", "Active Note", "--vault", "Personal"], None);
    assert!(!out_err.status.success());
    let err_msg = String::from_utf8_lossy(&out_err.stderr);
    assert!(err_msg.contains("is not deleted"), "stderr: {err_msg}");

    // 2. Attempting to undelete nonexistent item
    let out_missing = gp(&root, &["undelete", "Nonexistent Item", "--vault", "Personal"], None);
    assert!(!out_missing.status.success());
    assert!(String::from_utf8_lossy(&out_missing.stderr).contains("no deleted item matches"));

    // 3. Undelete the deleted item with --json
    let out_undel = gp(&root, &["undelete", "Deleted Login", "--vault", "Personal", "--json"], None);
    assert!(out_undel.status.success(), "stderr: {}", String::from_utf8_lossy(&out_undel.stderr));
    let d: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_undel.stdout)).unwrap();
    assert_eq!(d["deleted_at"], serde_json::Value::Null);
    assert_eq!(d["title"], "Deleted Login");

    // 4. Now the restored item is visible in normal list
    let out_list = gp(&root, &["list", "Personal", "--json"], None);
    let arr: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    assert!(arr.iter().any(|i| i["id"] == id_deleted));
    assert!(arr.iter().any(|i| i["id"] == id_active));

    // Secrecy check
    assert!(!String::from_utf8_lossy(&out_undel.stdout).contains(SECRET_PW));
}

#[test]
fn move_item_across_vaults_preserves_history_and_attachments() {
    let (_dir, root) = setup_two_vaults();
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid_from = s.resolve_vault("Personal").unwrap();
    let vid_to = s.resolve_vault("Work").unwrap();

    // Create item in Personal
    let mut it = Item::default();
    it.title = "Important Server".into();
    it.password = Some("FAKE-v1-password".into());
    let mut r = ItemRecord::new(it);
    r.category = Category::Server;
    r.tags = vec!["ops".into()];
    r.favorite = true;
    let id_from = s.add_record(&vid_from, r).unwrap();

    // Add an attachment
    let at_bytes = b"ssh-ed25519 FAKE_SSH_KEY_DATA";
    s.add_attachment(&vid_from, &id_from, "id_ed25519.pub", "text/plain", at_bytes).unwrap();

    // Make an edit to generate version history
    let out_edit = gp(
        &root,
        &["edit", "Personal", &id_from, "--notes", "Updated note for version 2"],
        None,
    );
    assert!(out_edit.status.success());

    // Verify history has version 3, 2 and 1 before move
    let out_hist1 = gp(&root, &["history", &id_from, "--vault", "Personal", "--json"], None);
    assert!(out_hist1.status.success());
    let h1: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_hist1.stdout)).unwrap();
    assert_eq!(h1.len(), 3);

    // 1. Move to same vault should error
    let out_same = gp(&root, &["mv", &id_from, "--vault", "Personal", "--to", "Personal"], None);
    assert!(!out_same.status.success());
    assert!(String::from_utf8_lossy(&out_same.stderr).contains("already in vault"));

    // 2. Move to nonexistent vault should error
    let out_no_vault = gp(&root, &["mv", &id_from, "--vault", "Personal", "--to", "Nonexistent"], None);
    assert!(!out_no_vault.status.success());
    assert!(String::from_utf8_lossy(&out_no_vault.stderr).contains("no vault named"));

    // 3. Move from Personal to Work with --json
    let out_mv = gp(&root, &["mv", "Important Server", "--vault", "Personal", "--to", "Work", "--json"], None);
    assert!(out_mv.status.success(), "stderr: {}", String::from_utf8_lossy(&out_mv.stderr));
    let moved_doc: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_mv.stdout)).unwrap();
    let id_to = moved_doc["id"].as_str().unwrap().to_string();
    assert_eq!(moved_doc["title"], "Important Server");
    assert_eq!(moved_doc["favorite"], true);
    assert_eq!(moved_doc["tags"], serde_json::json!(["ops"]));

    // 4. Item no longer exists in Personal
    let out_get_old = gp(&root, &["get", "Personal", &id_from], None);
    assert!(!out_get_old.status.success());

    // 5. Item exists in Work
    let out_get_new = gp(&root, &["get", "Work", &id_to], None);
    assert!(out_get_new.status.success());

    // 6. History is preserved in destination vault (including previous versions)
    let out_hist2 = gp(&root, &["history", &id_to, "--vault", "Work", "--json"], None);
    assert!(out_hist2.status.success(), "stderr: {}", String::from_utf8_lossy(&out_hist2.stderr));
    let h2: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_hist2.stdout)).unwrap();
    assert_eq!(h2.len(), 3, "history entries must be kept after move");
    assert_eq!(h2.len(), h1.len(), "moved item must have identical history length to original");

    // 7. Attachments are preserved and accessible
    let out_att = gp(&root, &["attach", "ls", "Work", &id_to, "--json"], None);
    assert!(out_att.status.success());
    let atts: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_att.stdout)).unwrap();
    assert_eq!(atts.len(), 1);
    assert_eq!(atts[0]["name"], "id_ed25519.pub");

    // 8. Test Session directly: s.read_attachment in target vault succeeds
    let s_after = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let fetched = s_after.read_attachment(&vid_to, &id_to, atts[0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(fetched, at_bytes);

    // Secrecy check
    assert!(!String::from_utf8_lossy(&out_mv.stdout).contains("FAKE-v1-password"));
}

#[test]
fn pick_flag_resolves_ambiguous_targets() {
    let (_dir, root) = setup_two_vaults();
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it1 = Item::default();
    it1.title = "Github Personal".into();
    let r1 = ItemRecord::new(it1);
    let id1 = s.add_record(&vid, r1).unwrap();

    let mut it2 = Item::default();
    it2.title = "Github Enterprise".into();
    let r2 = ItemRecord::new(it2);
    let id2 = s.add_record(&vid, r2).unwrap();

    // Ambiguous without --pick
    let out_amb = gp(&root, &["edit", "Github", "--vault", "Personal", "--tag", "test"], None);
    assert!(!out_amb.status.success());
    assert!(String::from_utf8_lossy(&out_amb.stderr).contains("use --pick"));

    // Success with --pick 1
    let out_pick1 = gp(&root, &["edit", "Github", "--vault", "Personal", "--pick", "1", "--tag", "first"], None);
    assert!(out_pick1.status.success());

    // Success with --pick 2
    let out_pick2 = gp(&root, &["edit", "Github", "--vault", "Personal", "--pick", "2", "--tag", "second"], None);
    assert!(out_pick2.status.success());

    // Verify tag on id1
    let out_g1 = gp(&root, &["get", "Personal", &id1, "--json"], None);
    let d1: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_g1.stdout)).unwrap();
    // One of them is first, one is second depending on search score ranking
    let tags1: Vec<String> = d1["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();

    let out_g2 = gp(&root, &["get", "Personal", &id2, "--json"], None);
    let d2: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_g2.stdout)).unwrap();
    let tags2: Vec<String> = d2["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();

    assert!(tags1.contains(&"first".to_string()) || tags1.contains(&"second".to_string()));
    assert!(tags2.contains(&"first".to_string()) || tags2.contains(&"second".to_string()));
    assert_ne!(tags1, tags2);
}
