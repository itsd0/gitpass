//! Integration tests for `gitpass attach` (add, ls, get, rm).
//! Tests temp vaults and temp homes only. All test data is FAKE.

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

#[test]
fn attach_add_ls_get_rm_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Personal Bank Account".into();
    it.username = Some("alice@example.invalid".into());
    let mut r = ItemRecord::new(it);
    r.category = Category::Login;
    let item_id = s.add_record(&vid, r).unwrap();

    // 1. Initially ls shows no attachments
    let out_empty = gp(&root, &["attach", "ls", "Personal", "Personal Bank Account"], None);
    assert!(out_empty.status.success());
    assert!(String::from_utf8_lossy(&out_empty.stdout).contains("no attachments"));

    let out_empty_json = gp(&root, &["attach", "ls", "Personal", &item_id, "--json"], None);
    assert!(out_empty_json.status.success());
    let arr: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_empty_json.stdout)).unwrap();
    assert_eq!(arr.len(), 0);

    // 2. Add an attachment
    let pdf_data = b"%PDF-1.4 fake binary statement data";
    let src_pdf = dir.path().join("statement.pdf");
    std::fs::write(&src_pdf, pdf_data).unwrap();

    let out_add1 = gp(&root, &["attach", "add", "Personal", "Personal Bank Account", src_pdf.to_str().unwrap()], None);
    assert!(out_add1.status.success(), "stderr: {}", String::from_utf8_lossy(&out_add1.stderr));
    let msg1 = String::from_utf8_lossy(&out_add1.stdout);
    assert!(msg1.contains("added attachment 'statement.pdf'"));
    assert!(msg1.contains("Personal Bank Account"));

    // 3. Add a second attachment
    let txt_data = b"sample backup text";
    let src_txt = dir.path().join("backup.txt");
    std::fs::write(&src_txt, txt_data).unwrap();

    let out_add2 = gp(&root, &["attach", "add", "Personal", &item_id, src_txt.to_str().unwrap()], None);
    assert!(out_add2.status.success());

    // 4. List attachments in human text
    let out_ls = gp(&root, &["attach", "ls", "Personal", "Personal Bank Account"], None);
    assert!(out_ls.status.success());
    let ls_text = String::from_utf8_lossy(&out_ls.stdout);
    assert!(ls_text.contains("statement.pdf"));
    assert!(ls_text.contains("backup.txt"));

    // 5. List attachments in JSON
    let out_json = gp(&root, &["attach", "ls", "Personal", &item_id, "--json"], None);
    assert!(out_json.status.success());
    let atts: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(atts.len(), 2);
    let pdf_meta = atts.iter().find(|a| a["name"] == "statement.pdf").unwrap();
    assert_eq!(pdf_meta["size"], pdf_data.len());
    assert_eq!(pdf_meta["mime"], "application/pdf");
    let pdf_id = pdf_meta["id"].as_str().unwrap().to_string();
    let _ = &pdf_id;

    let txt_meta = atts.iter().find(|a| a["name"] == "backup.txt").unwrap();
    assert_eq!(txt_meta["size"], txt_data.len());
    assert_eq!(txt_meta["mime"], "text/plain");
    let txt_id = txt_meta["id"].as_str().unwrap().to_string();

    // 6. Get attachment by name
    let out_dest = dir.path().join("downloaded_statement.pdf");
    let out_get1 = gp(&root, &["attach", "get", "Personal", &item_id, "statement.pdf", "-o", out_dest.to_str().unwrap()], None);
    assert!(out_get1.status.success(), "stderr: {}", String::from_utf8_lossy(&out_get1.stderr));
    assert_eq!(std::fs::read(&out_dest).unwrap(), pdf_data);

    // 7. Get refuses to overwrite existing file without --force
    let out_get_no_force = gp(&root, &["attach", "get", "Personal", &item_id, "statement.pdf", "-o", out_dest.to_str().unwrap()], None);
    assert!(!out_get_no_force.status.success());
    assert!(String::from_utf8_lossy(&out_get_no_force.stderr).contains("already exists (use --force to overwrite)"));

    // 8. Get with --force overwrites successfully
    std::fs::write(&out_dest, b"corrupted").unwrap();
    let out_get_force = gp(&root, &["attach", "get", "Personal", &item_id, "statement.pdf", "-o", out_dest.to_str().unwrap(), "--force"], None);
    assert!(out_get_force.status.success());
    assert_eq!(std::fs::read(&out_dest).unwrap(), pdf_data);

    // 9. Get by attachment ID
    let out_dest2 = dir.path().join("downloaded_backup.txt");
    let out_get_by_id = gp(&root, &["attach", "get", "Personal", &item_id, &txt_id, "-o", out_dest2.to_str().unwrap()], None);
    assert!(out_get_by_id.status.success());
    assert_eq!(std::fs::read(&out_dest2).unwrap(), txt_data);

    // 10. Remove attachment by name
    let out_rm1 = gp(&root, &["attach", "rm", "Personal", &item_id, "statement.pdf"], None);
    assert!(out_rm1.status.success(), "stderr: {}", String::from_utf8_lossy(&out_rm1.stderr));
    let rm_msg = String::from_utf8_lossy(&out_rm1.stdout);
    assert!(rm_msg.contains("removed attachment 'statement.pdf'"));

    // Verify statement.pdf is gone
    let out_ls2 = gp(&root, &["attach", "ls", "Personal", &item_id, "--json"], None);
    let atts2: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_ls2.stdout)).unwrap();
    assert_eq!(atts2.len(), 1);
    assert_eq!(atts2[0]["name"], "backup.txt");

    // 11. Remove remaining attachment by ID
    let out_rm2 = gp(&root, &["attach", "rm", "Personal", &item_id, &txt_id], None);
    assert!(out_rm2.status.success());

    let out_ls3 = gp(&root, &["attach", "ls", "Personal", &item_id, "--json"], None);
    let atts3: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out_ls3.stdout)).unwrap();
    assert_eq!(atts3.len(), 0);

    // 12. Removing or getting non-existent attachment returns clear error
    let out_rm_err = gp(&root, &["attach", "rm", "Personal", &item_id, "statement.pdf"], None);
    assert!(!out_rm_err.status.success());
    assert!(String::from_utf8_lossy(&out_rm_err.stderr).contains("no attachment matches 'statement.pdf'"));

    let out_get_err = gp(&root, &["attach", "get", "Personal", &item_id, "statement.pdf"], None);
    assert!(!out_get_err.status.success());
    assert!(String::from_utf8_lossy(&out_get_err.stderr).contains("no attachment matches 'statement.pdf'"));

    // 13. Adding non-existent file returns error
    let out_add_missing = gp(&root, &["attach", "add", "Personal", &item_id, "nonexistent_file_path.pdf"], None);
    assert!(!out_add_missing.status.success());
    assert!(String::from_utf8_lossy(&out_add_missing.stderr).contains("does not exist"));
}

fn gp_in(root: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(args)
        .current_dir(cwd)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn attach_get_without_output_never_writes_outside_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());
    let sk = SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();
    let mut it = Item::default();
    it.title = "Synced Item".into();
    let item_id = s.add_record(&vid, ItemRecord::new(it)).unwrap();

    // Names as they could arrive from another (malicious or buggy) device via sync.
    let abs_target = dir.path().join("abs_target.txt");
    let traversal = s.add_attachment(&vid, &item_id, "../../evil_authorized_keys", "text/plain", b"FAKE traversal").unwrap();
    let absolute = s.add_attachment(&vid, &item_id, abs_target.to_str().unwrap(), "text/plain", b"FAKE absolute").unwrap();
    let dotdot = s.add_attachment(&vid, &item_id, "..", "text/plain", b"FAKE dotdot").unwrap();

    let cwd = dir.path().join("work").join("sub");
    std::fs::create_dir_all(&cwd).unwrap();

    let o = gp_in(&root, &cwd, &["attach", "get", "Personal", &item_id, &traversal.id]);
    assert!(o.status.success(), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(cwd.join("evil_authorized_keys")).unwrap(), b"FAKE traversal");
    assert!(!dir.path().join("evil_authorized_keys").exists(), "../ name must not escape cwd");

    let o = gp_in(&root, &cwd, &["attach", "get", "Personal", &item_id, &absolute.id]);
    assert!(o.status.success(), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(cwd.join("abs_target.txt")).unwrap(), b"FAKE absolute");
    assert!(!abs_target.exists(), "absolute name must not be honoured");

    let o = gp_in(&root, &cwd, &["attach", "get", "Personal", &item_id, &dotdot.id]);
    assert!(o.status.success(), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(cwd.join("attachment")).unwrap(), b"FAKE dotdot");
}
