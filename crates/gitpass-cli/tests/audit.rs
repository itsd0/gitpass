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
    let secret_cvv = "999";
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

    // Run `gitpass audit --json`
    let out = gp(&root, &["audit", "--json"], None);
    assert!(out.status.success(), "audit failed: {}", String::from_utf8_lossy(&out.stderr));
    let json_text = String::from_utf8_lossy(&out.stdout).to_string();

    // Verify secrets never appear in output
    assert!(!json_text.contains(secret_card_num), "leaked card number");
    assert!(!json_text.contains(secret_cvv), "leaked CVV");
    assert!(!json_text.contains(secret_api_key), "leaked API key");

    // Verify findings carry titles and IDs only
    let v: serde_json::Value = serde_json::from_str(&json_text).expect("valid JSON from audit --json");
    assert_eq!(v["expired"], 1);
    assert_eq!(v["expiring_soon"], 1);

    let per_item = v["per_item"].as_array().expect("per_item array");
    let item1 = per_item.iter().find(|i| i["id"] == id1).expect("id1 present");
    assert_eq!(item1["title"], "Expired Visa Card");
    assert_eq!(item1["findings"][0]["expired"]["on"], "01/20");

    let item2 = per_item.iter().find(|i| i["id"] == id2).expect("id2 present");
    assert_eq!(item2["title"], "Expiring Stripe Key");
    assert!(item2["findings"][0]["expiring_soon"].is_object());

    // Verify text output
    let text_out = gp(&root, &["audit", "--expiring-days", "30"], None);
    assert!(text_out.status.success());
    let text = String::from_utf8_lossy(&text_out.stdout);
    assert!(text.contains("expired: 1"));
    assert!(text.contains("expiring soon (<=30d): 1"));
    assert!(text.contains("Expired Visa Card"));
    assert!(text.contains("expired on 01/20"));
    assert!(text.contains("Expiring Stripe Key"));
}
