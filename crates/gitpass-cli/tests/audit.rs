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
fn audit_json_includes_change_password_url_per_item() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let mut with_url = Item::default();
    with_url.title = "Fake Forum".into();
    with_url.url = Some("http://forum.example.com/login?token=fake-secret#frag".into());
    with_url.password = Some("123456".into());
    let id_url = s.add_record(&vid, ItemRecord::new(with_url)).unwrap();

    let mut no_url = Item::default();
    no_url.title = "Fake Local Login".into();
    no_url.password = Some("123456".into());
    let id_no = s.add_record(&vid, ItemRecord::new(no_url)).unwrap();

    let out = gp(&root, &["audit", "--json"], None);
    assert!(out.status.success(), "audit failed: {}", String::from_utf8_lossy(&out.stderr));
    let json_text = String::from_utf8_lossy(&out.stdout).to_string();
    // Only the derived URL is emitted: the path/query/fragment (and its secret) never leak.
    assert!(!json_text.contains("token=fake-secret"));
    let v: serde_json::Value = serde_json::from_str(&json_text).expect("valid JSON from audit --json");
    let per_item = v["per_item"].as_array().expect("per_item array");
    let with_url = per_item.iter().find(|i| i["id"] == id_url).expect("item with URL present");
    assert_eq!(with_url["change_password_url"], "https://forum.example.com/.well-known/change-password");
    assert_eq!(with_url["title"], "Fake Forum");
    let no_url = per_item.iter().find(|i| i["id"] == id_no).expect("item without URL present");
    assert!(no_url.get("change_password_url").is_none(), "no derived URL for a login without a URL");
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

#[test]
fn audit_cli_flags_passwords_containing_username_or_site_name() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    // 1. Password contains username (long, high entropy password)
    let mut it1 = Item::default();
    it1.title = "GitHub Account".into();
    it1.username = Some("alice".into());
    it1.url = Some("https://login.github.com".into());
    it1.password = Some("github-alice-2024!".into());
    let id1 = s.add_record(&vid, ItemRecord::new(it1)).unwrap();

    // 2. Password contains site name (long, high entropy password)
    let mut it2 = Item::default();
    it2.title = "Bank Account".into();
    it2.username = Some("bob".into());
    it2.url = Some("https://sub.portal.my-bank.co.uk".into());
    it2.password = Some("my-bank-2024-strong!".into());
    let id2 = s.add_record(&vid, ItemRecord::new(it2)).unwrap();

    // 3. Strong clean password
    let mut it3 = Item::default();
    it3.title = "Clean Account".into();
    it3.username = Some("carol".into());
    it3.url = None;
    it3.password = Some("Xq7#vR2!mK9@pL4$wZ".into());
    let _id3 = s.add_record(&vid, ItemRecord::new(it3)).unwrap();

    // Test text output
    let out = gp(&root, &["audit"], None);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("weak: 2"));
    assert!(text.contains("GitHub Account"));
    assert!(text.contains("weak password: contains the username"));
    assert!(text.contains("Bank Account"));
    assert!(text.contains("weak password: contains the site name"));
    assert!(!text.contains("Clean Account"));

    // Test JSON output
    let out_json = gp(&root, &["audit", "--json"], None);
    assert!(out_json.status.success());
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(v["weak"], 2);
    let per_item = v["per_item"].as_array().unwrap();

    let item1 = per_item.iter().find(|i| i["id"] == id1).expect("item1 found");
    let weak1 = item1["findings"].as_array().unwrap().iter().find(|f| f.get("weak").is_some()).unwrap();
    assert_eq!(weak1["weak"]["reason"], "contains the username");

    let item2 = per_item.iter().find(|i| i["id"] == id2).expect("item2 found");
    let weak2 = item2["findings"].as_array().unwrap().iter().find(|f| f.get("weak").is_some()).unwrap();
    assert_eq!(weak2["weak"]["reason"], "contains the site name");
}

#[test]
fn audit_help_lists_every_finding_kind() {
    let d = tempfile::tempdir().unwrap();
    let out = gp(&d.path().join("vault"), &["audit", "--help"], None);
    assert!(out.status.success(), "audit --help failed: {}", String::from_utf8_lossy(&out.stderr));
    let help = String::from_utf8_lossy(&out.stdout);
    for kind in gitpass_core::audit::FINDING_KINDS {
        assert!(help.contains(kind), "audit --help does not mention {kind}:\n{help}");
    }
    for flag in ["--hibp", "--max-age-days", "--expiring-days", "--include-ignored", "--json"] {
        assert!(help.contains(flag), "audit --help does not mention {flag}");
    }
    assert!(help.contains("username or site name"), "help should mention the username/site-name rule");
}


#[test]
fn test_get_shows_password_changed_at_and_audit_old_passwords() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(root.join("secret.key")).unwrap()).unwrap();
    let s = Session::unlock(&root, "fake-master-pw-123", &sk).unwrap();
    let vid = s.resolve_vault("Personal").unwrap();

    let past_epoch = 1_600_000_000u64; // 2020-09-13T12:26:40Z
    let mut it = Item::default();
    it.title = "Old Service".into();
    it.username = Some("alice".into());
    it.password = Some("VerySecurePassword2020!".into());
    it.url = Some("https://service.example.com".into());
    it.updated_at = past_epoch.to_string();

    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(past_epoch);
    let id = s.add_record(&vid, rec).unwrap();

    // 1. gitpass get human-readable shows password changed
    let out_get = gp(&root, &["get", "Personal", &id], None);
    assert!(out_get.status.success());
    let get_text = String::from_utf8_lossy(&out_get.stdout);
    assert!(get_text.contains("password changed: 2020-09-13T12:26:40Z"), "expected password changed in text output:\n{get_text}");

    // 2. gitpass get --json exposes password_changed_at without secrets
    let out_json = gp(&root, &["get", "Personal", &id, "--json"], None);
    assert!(out_json.status.success());
    let get_val: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(get_val["password_changed_at"], "2020-09-13T12:26:40Z");
    assert!(!String::from_utf8_lossy(&out_json.stdout).contains("VerySecurePassword2020!"));

    // 3. Update notes (simulate item update with unchanged password)
    let (_, mut rec_read) = s.read_record(&vid, &id).unwrap();
    rec_read.item.notes = Some("note updated recently".into());
    s.update_record(&vid, &id, rec_read).unwrap();

    // password_changed_at remains 2020-09-13
    let out_json_after_note = gp(&root, &["get", "Personal", &id, "--json"], None);
    let val_after_note: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json_after_note.stdout)).unwrap();
    assert_eq!(val_after_note["password_changed_at"], "2020-09-13T12:26:40Z");

    // 4. Update password to a new value
    let (_, mut rec_read2) = s.read_record(&vid, &id).unwrap();
    rec_read2.item.password = Some("BrandNewSecurePassword2026!".into());
    s.update_record(&vid, &id, rec_read2).unwrap();

    let out_json_after_pw = gp(&root, &["get", "Personal", &id, "--json"], None);
    let val_after_pw: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json_after_pw.stdout)).unwrap();
    let pw_changed = val_after_pw["password_changed_at"].as_str().unwrap();
    assert_ne!(pw_changed, "2020-09-13T12:26:40Z");
}
