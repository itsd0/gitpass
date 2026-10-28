//! `gitpass audit` with text and JSON output on a temp vault. All data FAKE.
use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args).env("GITPASS_PATH", root).env("GITPASS_PASSWORD", "fake-master-pw-123").env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut ch = c.spawn().unwrap();
    { let mut i = ch.stdin.take().unwrap(); if let Some(s) = stdin { i.write_all(s.as_bytes()).unwrap(); } }
    ch.wait_with_output().unwrap()
}

#[test]
fn audit_json_includes_new_findings_and_titles_only() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let secret_card_num = "4111222233334444";
    let secret_cvv = "999888";
    let secret_api_key = "sk_test_SUPER_SECRET_KEY_12345";

    // 1. Expired credit card (past month: 01/20)
    let mut it1 = Item::default();
    it1.title = "Expired Visa Card".into();
    let mut rec1 = ItemRecord::new(it1);
    rec1.category = Category::CreditCard;
    rec1.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "number".into(), label: "number".into(), kind: FieldKind::Concealed, value: Some(secret_card_num.into()), removed: false },
            Field { id: "cvv".into(), label: "verification number".into(), kind: FieldKind::Concealed, value: Some(secret_cvv.into()), removed: false },
            Field { id: "expiry".into(), label: "expiry date".into(), kind: FieldKind::MonthYear, value: Some("01/20".into()), removed: false },
        ],
    }];
    let id1 = s.add_record(&vid, rec1).unwrap();

    // 2. Expiring soon API credential (within next 10 days)
    let now = store::now().parse::<u64>().unwrap_or_else(|_| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    });
    let soon_ts = now + 10 * 86400;
    let mut it2 = Item::default();
    it2.title = "Expiring Stripe Key".into();
    let mut rec2 = ItemRecord::new(it2);
    rec2.category = Category::ApiCredential;
    rec2.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "credential".into(), label: "credential".into(), kind: FieldKind::Concealed, value: Some(secret_api_key.into()), removed: false },
            Field { id: "expires".into(), label: "expires".into(), kind: FieldKind::Date, value: Some(soon_ts.to_string()), removed: false },
        ],
    }];
    let id2 = s.add_record(&vid, rec2).unwrap();

    // 3. Duplicate logins (same normalized site + username)
    let secret_login_pw = "SuperStrongFakePw123!#";
    let mut it3 = Item::default();
    it3.title = "Example Login 1".into();
    it3.username = Some("alice".into());
    it3.url = Some("https://example.com/login".into());
    it3.password = Some(secret_login_pw.into());
    let id3 = s.add_record(&vid, ItemRecord::new(it3)).unwrap();

    let mut it4 = Item::default();
    it4.title = "Example Login 2".into();
    it4.username = Some("Alice".into());
    it4.url = Some("http://www.example.com/other".into());
    it4.password = Some("AnotherStrongFakePw456!#".into());
    let id4 = s.add_record(&vid, ItemRecord::new(it4)).unwrap();

    // Run `gitpass audit --json`
    let out = gp(&root, &["audit", "--json"], None);
    assert!(out.status.success(), "audit failed: {}", String::from_utf8_lossy(&out.stderr));
    let json_text = String::from_utf8_lossy(&out.stdout).to_string();

    // Verify secrets never appear in output
    assert!(!json_text.contains(secret_card_num), "leaked card number");
    assert!(!json_text.contains(secret_cvv), "leaked CVV");
    assert!(!json_text.contains(secret_api_key), "leaked API key");
    assert!(!json_text.contains(secret_login_pw), "leaked login password");

    // Verify findings carry titles and IDs only
    let v: serde_json::Value = serde_json::from_str(&json_text).expect("valid JSON from audit --json");
    assert_eq!(v["expired"], 1);
    assert_eq!(v["expiring_soon"], 1);
    assert_eq!(v["duplicates"], 2);

    let per_item = v["per_item"].as_array().expect("per_item array");
    let item1 = per_item.iter().find(|i| i["id"] == id1).expect("id1 present");
    assert_eq!(item1["title"], "Expired Visa Card");
    assert_eq!(item1["findings"][0]["expired"]["on"], "01/20");

    let item2 = per_item.iter().find(|i| i["id"] == id2).expect("id2 present");
    assert_eq!(item2["title"], "Expiring Stripe Key");
    assert!(item2["findings"][0]["expiring_soon"].is_object());

    let item3 = per_item.iter().find(|i| i["id"] == id3).expect("id3 present");
    assert_eq!(item3["title"], "Example Login 1");
    let dup_finding3 = item3["findings"].as_array().unwrap().iter().find(|f| f.get("duplicate").is_some()).expect("duplicate finding on id3");
    assert_eq!(dup_finding3["duplicate"]["group"], "example.com/alice");

    let item4 = per_item.iter().find(|i| i["id"] == id4).expect("id4 present");
    assert_eq!(item4["title"], "Example Login 2");
    let dup_finding4 = item4["findings"].as_array().unwrap().iter().find(|f| f.get("duplicate").is_some()).expect("duplicate finding on id4");
    assert_eq!(dup_finding4["duplicate"]["group"], "example.com/alice");

    // Verify text output
    let text_out = gp(&root, &["audit", "--expiring-days", "30"], None);
    assert!(text_out.status.success());
    let text = String::from_utf8_lossy(&text_out.stdout);
    assert!(text.contains("expired: 1"));
    assert!(text.contains("expiring soon (<=30d): 1"));
    assert!(text.contains("duplicates: 2"));
    assert!(text.contains("Expired Visa Card"));
    assert!(text.contains("expired on 01/20"));
    assert!(text.contains("Expiring Stripe Key"));
    assert!(text.contains("Example Login 1"));
    assert!(text.contains("Example Login 2"));
    assert!(text.contains("duplicate login for example.com/alice"));
    assert!(!text.contains(secret_login_pw));
    assert!(!text.contains("/login"));
}

#[test]
fn audit_cli_respects_ignored_findings_and_flag() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut it = Item::default();
    it.title = "Internal Router".into();
    it.url = Some("http://public-router.example.com".into());
    it.password = Some("SuperSecurePassword123!".into());
    let mut rec = ItemRecord::new(it);
    rec.ignore_finding("insecure_url");
    let id = s.add_record(&vid, rec).unwrap();

    // 1. Regular audit: ignored count is 1, insecure_url is 0, item is NOT listed under active findings
    let out = gp(&root, &["audit"], None);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("insecure URLs: 0"));
    assert!(text.contains("ignored: 1"));
    assert!(!text.contains("saved with unencrypted HTTP URL"));
    assert!(!text.contains("ignored findings ("));

    // 2. Audit with --include-ignored: ignored finding details are printed
    let out_inc = gp(&root, &["audit", "--include-ignored"], None);
    assert!(out_inc.status.success());
    let text_inc = String::from_utf8_lossy(&out_inc.stdout);
    assert!(text_inc.contains("insecure URLs: 0"));
    assert!(text_inc.contains("ignored: 1"));
    assert!(text_inc.contains("ignored findings (1):"));
    assert!(text_inc.contains("Internal Router"));
    assert!(text_inc.contains("saved with unencrypted HTTP URL (public-router.example.com) (ignored)"));

    // 3. JSON output
    let out_json = gp(&root, &["audit", "--json"], None);
    assert!(out_json.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(v["insecure_urls"], 0);
    assert_eq!(v["ignored"], 1);
    let ignored_items = v["ignored_per_item"].as_array().unwrap();
    assert_eq!(ignored_items.len(), 1);
    assert_eq!(ignored_items[0]["id"], id);
}
