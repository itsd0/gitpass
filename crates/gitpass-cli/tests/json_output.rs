//! Integration tests for `gitpass list`, `find`, and `get` with `--json`.
//! Checks JSON shapes, confirms secret values never appear unless `--reveal`,
//! validates ambiguous-query error handling to stderr, and tests `--include-archived`.
//! All test data is FAKE.

use gitpass_core::item::{Category, Field, FieldKind, Section};
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

struct TestVault {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    id_login: String,
    id_card: String,
    id_work1: String,
    id_work2: String,
    id_archived: String,
    id_deleted: String,
}

const SECRET_PW: &str = "FAKE-login-pw-987!";
const SECRET_TOTP: &str = "otpauth://totp/fake:alice?secret=JBSWY3DPEHPK3PXP";
const SECRET_NOTES: &str = "FAKE-secret-note-recovery-codes";
const SECRET_CARD_NUM: &str = "4111222233334444";
const SECRET_CVV: &str = "999";
const SECRET_ARCHIVED_PW: &str = "FAKE-archived-pw-111";
const SECRET_DELETED_PW: &str = "FAKE-deleted-pw-222";

fn setup_vault() -> TestVault {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    // 1. Standard login item with all secrets, tags, favorite
    let mut it1 = Item::default();
    it1.title = "Personal GitHub".into();
    it1.username = Some("alice-dev".into());
    it1.password = Some(SECRET_PW.into());
    it1.url = Some("https://github.com".into());
    it1.totp = Some(SECRET_TOTP.into());
    it1.notes = Some(SECRET_NOTES.into());
    it1.updated_at = "1793000000".into();
    let mut rec1 = ItemRecord::new(it1);
    rec1.tags = vec!["dev".into(), "vcs".into()];
    rec1.favorite = true;
    let id_login = s.add_record(&vid, rec1).unwrap();

    // 2. Credit Card item with custom sections/fields
    let mut it2 = Item::default();
    it2.title = "Bank of Example Card".into();
    it2.updated_at = "1793000100".into();
    let mut rec2 = ItemRecord::new(it2);
    rec2.category = Category::CreditCard;
    rec2.sections = vec![Section {
        id: "card_info".into(),
        title: Some("Card Details".into()),
        fields: vec![
            Field { id: "cardholder".into(), label: "cardholder name".into(), kind: FieldKind::Text, value: Some("Alice Doe".into()), removed: false },
            Field { id: "number".into(), label: "card number".into(), kind: FieldKind::Concealed, value: Some(SECRET_CARD_NUM.into()), removed: false },
            Field { id: "cvv".into(), label: "security code".into(), kind: FieldKind::Concealed, value: Some(SECRET_CVV.into()), removed: false },
            Field { id: "expiry".into(), label: "expires".into(), kind: FieldKind::MonthYear, value: Some("09/29".into()), removed: false },
        ],
    }];
    rec2.tags = vec!["finance".into()];
    let id_card = s.add_record(&vid, rec2).unwrap();

    // 3. Ambiguous items (matching "Work" query with equal score)
    let mut it3a = Item::default();
    it3a.title = "Work Email".into();
    it3a.username = Some("alice@company.invalid".into());
    it3a.password = Some("FAKE-work-email-pw".into());
    let id_work1 = s.add_record(&vid, ItemRecord::new(it3a)).unwrap();

    let mut it3b = Item::default();
    it3b.title = "Work Slack".into();
    it3b.username = Some("alice@company.invalid".into());
    it3b.password = Some("FAKE-work-slack-pw".into());
    let id_work2 = s.add_record(&vid, ItemRecord::new(it3b)).unwrap();

    // 4. Archived item
    let mut it4 = Item::default();
    it4.title = "Old Legacy Intranet".into();
    it4.password = Some(SECRET_ARCHIVED_PW.into());
    let mut rec4 = ItemRecord::new(it4);
    rec4.archived = true;
    let id_archived = s.add_record(&vid, rec4).unwrap();

    // 5. Soft-deleted item
    let mut it5 = Item::default();
    it5.title = "Deleted Test Service".into();
    it5.password = Some(SECRET_DELETED_PW.into());
    let mut rec5 = ItemRecord::new(it5);
    rec5.deleted_at = Some(1793000500);
    let id_deleted = s.add_record(&vid, rec5).unwrap();

    TestVault {
        _dir: dir,
        root,
        id_login,
        id_card,
        id_work1,
        id_work2,
        id_archived,
        id_deleted,
    }
}

#[test]
fn list_and_find_json_schema_and_secrecy() {
    let v = setup_vault();

    // `gitpass list Personal --json`
    let out = gp(&v.root, &["list", "Personal", "--json"], None);
    assert!(out.status.success(), "list failed: {}", String::from_utf8_lossy(&out.stderr));
    let raw = String::from_utf8_lossy(&out.stdout).to_string();

    // Prove NO secrets ever appear in list output
    for secret in [SECRET_PW, SECRET_TOTP, SECRET_NOTES, SECRET_CARD_NUM, SECRET_CVV, SECRET_ARCHIVED_PW, SECRET_DELETED_PW] {
        assert!(!raw.contains(secret), "secret '{secret}' leaked in list --json!");
    }

    // Archived and deleted items excluded by default
    assert!(!raw.contains("Old Legacy Intranet"));
    assert!(!raw.contains("Deleted Test Service"));

    // Verify JSON shape
    let val: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON array");
    let arr = val.as_array().expect("root is array");
    assert_eq!(arr.len(), 4); // login, card, work1, work2

    let login = arr.iter().find(|i| i["id"] == v.id_login).expect("login item present");
    assert_eq!(login["title"], "Personal GitHub");
    assert_eq!(login["category"], "login");
    assert_eq!(login["username"], "alice-dev");
    assert_eq!(login["url"], "https://github.com");
    assert_eq!(login["favorite"], true);
    assert_eq!(login["tags"].as_array().unwrap().len(), 2);
    assert_eq!(login["updated_at"], "2026-10-26T07:33:20Z");

    let card = arr.iter().find(|i| i["id"] == v.id_card).expect("card present");
    assert_eq!(card["category"], "credit_card");
    assert_eq!(card["title"], "Bank of Example Card");
    assert_eq!(card["username"], serde_json::Value::Null);

    // Test `gitpass list Personal --include-archived --json`
    let out_inc = gp(&v.root, &["list", "Personal", "--include-archived", "--json"], None);
    assert!(out_inc.status.success());
    let raw_inc = String::from_utf8_lossy(&out_inc.stdout);
    assert!(raw_inc.contains("Old Legacy Intranet"));
    assert!(raw_inc.contains("Deleted Test Service"));
    let val_inc: serde_json::Value = serde_json::from_str(&raw_inc).unwrap();
    assert_eq!(val_inc.as_array().unwrap().len(), 6);

    // Test `gitpass list Personal --query work --json`
    let out_q = gp(&v.root, &["list", "Personal", "--query", "work", "--json"], None);
    assert!(out_q.status.success());
    let val_q: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_q.stdout)).unwrap();
    let arr_q = val_q.as_array().unwrap();
    assert_eq!(arr_q.len(), 2);
    assert!(arr_q.iter().any(|i| i["id"] == v.id_work1));
    assert!(arr_q.iter().any(|i| i["id"] == v.id_work2));

    // Test `gitpass find Personal work --json`
    let out_f = gp(&v.root, &["find", "Personal", "work", "--json"], None);
    assert!(out_f.status.success());
    let val_f: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_f.stdout)).unwrap();
    let arr_f = val_f.as_array().unwrap();
    assert_eq!(arr_f.len(), 2);
    assert!(!String::from_utf8_lossy(&out_f.stdout).contains(SECRET_PW));
}

#[test]
fn get_json_without_reveal_masks_secrets() {
    let v = setup_vault();

    // 1. Get Login item without --reveal
    let out = gp(&v.root, &["get", "Personal", &v.id_login, "--json"], None);
    assert!(out.status.success(), "get failed: {}", String::from_utf8_lossy(&out.stderr));
    let raw = String::from_utf8_lossy(&out.stdout).to_string();

    // Prove concealed / secret values do not appear in output
    assert!(!raw.contains(SECRET_PW), "password leaked without --reveal");
    assert!(!raw.contains(SECRET_TOTP), "TOTP secret leaked without --reveal");

    let val: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON object");
    assert_eq!(val["id"], v.id_login);
    assert_eq!(val["title"], "Personal GitHub");
    assert_eq!(val["category"], "login");
    assert_eq!(val["username"], "alice-dev");
    assert_eq!(val["url"], "https://github.com");
    assert_eq!(val["favorite"], true);
    assert_eq!(val["archived"], false);
    assert_eq!(val["updated_at"], "2026-10-26T07:33:20Z");
    assert_eq!(val["notes"], SECRET_NOTES);

    let fields = val["fields"].as_array().expect("fields array");
    let pw_field = fields.iter().find(|f| f["label"] == "password").expect("password field");
    assert_eq!(pw_field["kind"], "concealed");
    assert_eq!(pw_field["value"], serde_json::Value::Null);

    let totp_field = fields.iter().find(|f| f["label"] == "one-time password").expect("totp field");
    assert_eq!(totp_field["kind"], "totp");
    assert_eq!(totp_field["value"], serde_json::Value::Null);

    let user_field = fields.iter().find(|f| f["label"] == "username").expect("username field");
    assert_eq!(user_field["kind"], "text");
    assert_eq!(user_field["value"], "alice-dev");

    // 2. Get Login item WITH --reveal
    let out_rev = gp(&v.root, &["get", "Personal", &v.id_login, "--json", "--reveal"], None);
    assert!(out_rev.status.success());
    let raw_rev = String::from_utf8_lossy(&out_rev.stdout);
    assert!(raw_rev.contains(SECRET_PW));
    assert!(raw_rev.contains(SECRET_TOTP));

    let val_rev: serde_json::Value = serde_json::from_str(&raw_rev).unwrap();
    let fields_rev = val_rev["fields"].as_array().unwrap();
    let pw_rev = fields_rev.iter().find(|f| f["label"] == "password").unwrap();
    assert_eq!(pw_rev["value"], SECRET_PW);
    let totp_rev = fields_rev.iter().find(|f| f["label"] == "one-time password").unwrap();
    assert_eq!(totp_rev["value"], SECRET_TOTP);

    // 3. Get Credit Card item without --reveal
    let out_card = gp(&v.root, &["get", "Personal", &v.id_card, "--json"], None);
    assert!(out_card.status.success());
    let raw_card = String::from_utf8_lossy(&out_card.stdout);
    assert!(!raw_card.contains(SECRET_CARD_NUM));
    assert!(!raw_card.contains(SECRET_CVV));

    let val_card: serde_json::Value = serde_json::from_str(&raw_card).unwrap();
    let card_fields = val_card["fields"].as_array().unwrap();
    let num_f = card_fields.iter().find(|f| f["label"] == "card number").unwrap();
    assert_eq!(num_f["kind"], "concealed");
    assert_eq!(num_f["value"], serde_json::Value::Null);

    let holder_f = card_fields.iter().find(|f| f["label"] == "cardholder name").unwrap();
    assert_eq!(holder_f["kind"], "text");
    assert_eq!(holder_f["value"], "Alice Doe");

    // 4. Get Credit Card item WITH --reveal
    let out_card_rev = gp(&v.root, &["get", "Personal", &v.id_card, "--json", "--reveal"], None);
    assert!(out_card_rev.status.success());
    let raw_card_rev = String::from_utf8_lossy(&out_card_rev.stdout);
    assert!(raw_card_rev.contains(SECRET_CARD_NUM));
    assert!(raw_card_rev.contains(SECRET_CVV));
}

#[test]
fn get_json_rejects_password_only_combination() {
    let v = setup_vault();

    let out = gp(&v.root, &["get", "Personal", &v.id_login, "--json", "--password-only"], None);
    assert!(!out.status.success(), "expected failure combining --json and --password-only");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot combine --json and --password-only"), "{err}");
}

#[test]
fn get_json_ambiguous_query_error_to_stderr() {
    let v = setup_vault();

    // Query "work" matches "Work Email" and "Work Slack"
    let out = gp(&v.root, &["get", "Personal", "work", "--json"], None);
    assert!(!out.status.success(), "expected failure on ambiguous query");

    // stdout must be completely empty so piping to jq does not receive partial / malformed data
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.is_empty(), "stdout was not empty: {stdout}");

    // stderr lists matching candidates and prompts for --pick
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("items match 'work'"), "stderr: {stderr}");
    assert!(stderr.contains("Work Email"), "stderr: {stderr}");
    assert!(stderr.contains("Work Slack"), "stderr: {stderr}");

    // Providing --pick resolves the ambiguity and outputs valid JSON to stdout
    let out_picked = gp(&v.root, &["get", "Personal", "work", "--json", "--pick", "1"], None);
    assert!(out_picked.status.success());
    let picked_val: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_picked.stdout))
        .expect("valid JSON with --pick");
    assert!(picked_val["title"] == "Work Email" || picked_val["title"] == "Work Slack");
}

#[test]
fn get_archived_and_deleted_requires_include_archived() {
    let v = setup_vault();

    // Archived item without flag fails
    let out1 = gp(&v.root, &["get", "Personal", &v.id_archived, "--json"], None);
    assert!(!out1.status.success());

    // Archived item with flag succeeds
    let out2 = gp(&v.root, &["get", "Personal", &v.id_archived, "--json", "--include-archived"], None);
    assert!(out2.status.success());
    let val2: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out2.stdout)).unwrap();
    assert_eq!(val2["archived"], true);

    // Deleted item without flag fails
    let out3 = gp(&v.root, &["get", "Personal", &v.id_deleted, "--json"], None);
    assert!(!out3.status.success());

    // Deleted item with flag succeeds
    let out4 = gp(&v.root, &["get", "Personal", &v.id_deleted, "--json", "--include-archived"], None);
    assert!(out4.status.success());
    let val4: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out4.stdout)).unwrap();
    assert!(val4["deleted_at"].is_string());
}

#[test]
fn list_sort_recent_and_last_used_tracking() {
    let v = setup_vault();

    // 1. Initial state: items have no last_used
    let out = gp(&v.root, &["list", "Personal", "--json"], None);
    assert!(out.status.success());
    let list: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    for it in &list {
        assert!(it.get("last_used").is_none());
    }

    // 2. Just viewing an item does NOT update last_used
    let view_out = gp(&v.root, &["get", "Personal", &v.id_login], None);
    assert!(view_out.status.success());
    let out = gp(&v.root, &["list", "Personal", "--json"], None);
    let list: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let login_summary = list.iter().find(|x| x["id"] == v.id_login).unwrap();
    assert!(login_summary.get("last_used").is_none());

    // 3. Revealing the password updates last_used
    let rev_out = gp(&v.root, &["get", "Personal", &v.id_login, "--password-only"], None);
    assert!(rev_out.status.success());
    let out = gp(&v.root, &["list", "Personal", "--json"], None);
    let list: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let login_summary = list.iter().find(|x| x["id"] == v.id_login).unwrap();
    assert!(login_summary["last_used"].as_u64().unwrap() > 0);

    // 4. Copying a field updates last_used for id_work2
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let copy_out = gp(&v.root, &["copy", "Personal", &v.id_work2, "--field", "username", "--quiet", "--clear-secs", "0"], None);
    assert!(copy_out.status.success());

    // 5. gitpass list --sort recent puts id_work2 first, then id_login, then others
    let recent_out = gp(&v.root, &["list", "Personal", "--sort", "recent", "--json"], None);
    assert!(recent_out.status.success());
    let list: Vec<serde_json::Value> = serde_json::from_str(&String::from_utf8_lossy(&recent_out.stdout)).unwrap();
    assert_eq!(list[0]["id"], v.id_work2);
    assert_eq!(list[1]["id"], v.id_login);

    // 6. get --json includes last_used_at
    let get_json_out = gp(&v.root, &["get", "Personal", &v.id_work2, "--json"], None);
    assert!(get_json_out.status.success());
    let detail: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&get_json_out.stdout)).unwrap();
    assert!(detail["last_used_at"].is_string());

    // 7. Verify no vault commit or sync was triggered
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&v.root)
        .output()
        .unwrap();
    let status_str = String::from_utf8_lossy(&status.stdout);
    assert!(!status_str.contains(".gitpass-state.json"));
}
