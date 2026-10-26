//! v4 data features: importers, export round trip, audit, HIBP (mocked). All fixture data is FAKE.
use gitpass_core::audit::{self, AuditOpts, Finding, PwnedRangeClient};
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat, ImportOpts};
use gitpass_core::store::{self, Item, Session};
use std::cell::RefCell;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

fn vault() -> (tempfile::TempDir, Session, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "fake-master-pw", fast()).unwrap();
    let s = Session::unlock(d.path(), "fake-master-pw", &sk).unwrap();
    let v = s.create_vault("test").unwrap();
    (d, s, v)
}

fn item(title: &str, pw: Option<&str>, url: Option<&str>) -> Item {
    Item { title: title.into(), username: Some("fakeuser".into()), password: pw.map(Into::into), url: url.map(Into::into),
        notes: None, totp: None, updated_at: store::now() }
}

fn by_title<'a>(items: &'a [Item], t: &str) -> &'a Item { items.iter().find(|i| i.title == t).unwrap() }

const BW_JSON: &str = r#"{"encrypted":false,"folders":[{"id":"f1","name":"Fake Folder"}],"items":[
 {"id":"1","folderId":"f1","type":1,"name":"Example Login","notes":"fake note","revisionDate":"2020-01-02T03:04:05.000Z",
  "fields":[{"name":"PIN","value":"0000","type":0}],
  "login":{"username":"alice@example.invalid","password":"FAKE-pw-1","totp":"JBSWY3DPEHPK3PXP",
           "uris":[{"match":null,"uri":"https://login.example.invalid"},{"uri":"https://alt.example.invalid"}]}},
 {"id":"2","type":2,"name":"Fake Secure Note","notes":"just text","secureNote":{"type":0}},
 {"id":"3","type":3,"name":"Fake Card","card":{"cardholderName":"Test Person","brand":"Visa","number":"4111111111111111"}}
]}"#;

#[test]
fn import_bitwarden_json() {
    let p = import::parse(ImportFormat::BitwardenJson, BW_JSON, None).unwrap();
    assert_eq!(p.items.len(), 3);
    let l = by_title(&p.items, "Example Login");
    assert_eq!(l.username.as_deref(), Some("alice@example.invalid"));
    assert_eq!(l.password.as_deref(), Some("FAKE-pw-1"));
    assert_eq!(l.url.as_deref(), Some("https://login.example.invalid"));
    assert_eq!(l.totp.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(l.updated_at, "1577934245");
    let n = l.notes.as_deref().unwrap();
    for s in ["fake note", "Folder: Fake Folder", "PIN: 0000", "Additional URL: https://alt.example.invalid"] { assert!(n.contains(s), "{n}"); }
    assert!(by_title(&p.items, "Fake Card").notes.as_deref().unwrap().contains("brand: Visa"));
    assert!(import::parse(ImportFormat::BitwardenJson, r#"{"encrypted":true,"items":[]}"#, None).is_err());
}

#[test]
fn import_bitwarden_csv() {
    let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
               Work,1,login,Fake Mail,hello,\"Recovery: abc\",0,https://mail.example.invalid,bob,FAKE-pw-2,JBSWY3DPEHPK3PXP\n\
               ,,note,Fake Note,secret text,,0,,,,\n";
    let p = import::parse(ImportFormat::BitwardenCsv, csv, None).unwrap();
    assert_eq!(p.items.len(), 2);
    let m = by_title(&p.items, "Fake Mail");
    assert_eq!((m.username.as_deref(), m.password.as_deref(), m.totp.as_deref()), (Some("bob"), Some("FAKE-pw-2"), Some("JBSWY3DPEHPK3PXP")));
    let n = m.notes.as_deref().unwrap();
    assert!(n.contains("hello") && n.contains("folder: Work") && n.contains("fields: Recovery: abc"), "{n}");
    assert!(by_title(&p.items, "Fake Note").notes.as_deref().unwrap().contains("secure note"));
}

#[test]
fn import_1password_csv_case_insensitive_and_bom() {
    let csv = "\u{feff}Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
               Fake Shop,https://shop.example.invalid,carol,FAKE-pw-3,otpauth://totp/x?secret=JBSWY3DPEHPK3PXP,false,false,shopping,n1\n";
    let p = import::parse(ImportFormat::OnePasswordCsv, csv, None).unwrap();
    let i = &p.items[0];
    assert_eq!(i.title, "Fake Shop");
    assert_eq!(i.url.as_deref(), Some("https://shop.example.invalid"));
    assert!(i.totp.as_deref().unwrap().starts_with("otpauth://"));
    assert!(i.notes.as_deref().unwrap().contains("Tags: shopping"));
    // lower-case / older header variants
    let p2 = import::parse(ImportFormat::OnePasswordCsv, "title,website,username,password,notesPlain\nX,https://x.invalid,u,p,nn\n", None).unwrap();
    assert_eq!(p2.items[0].url.as_deref(), Some("https://x.invalid"));
    assert_eq!(p2.items[0].notes.as_deref(), Some("nn"));
}

#[test]
fn import_keepassxc_csv() {
    let csv = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\",\"Icon\",\"Last Modified\",\"Created\"\n\
               \"Root/Fake\",\"Fake Forum\",\"dave\",\"FAKE-pw-4\",\"https://forum.example.invalid\",\"multi\nline\",\"\",\"0\",\"2021-06-01T00:00:00Z\",\"2021-01-01T00:00:00Z\"\n";
    let p = import::parse(ImportFormat::KeepassCsv, csv, None).unwrap();
    let i = &p.items[0];
    assert_eq!((i.title.as_str(), i.password.as_deref(), i.totp.as_deref()), ("Fake Forum", Some("FAKE-pw-4"), None));
    assert_eq!(i.updated_at, "1622505600");
    let n = i.notes.as_deref().unwrap();
    assert!(n.contains("multi\nline") && n.contains("Group: Root/Fake") && !n.contains("Icon"), "{n}");
}

#[test]
fn import_chrome_csv() {
    let csv = "name,url,username,password,note\nlogin.example.invalid,https://login.example.invalid/,erin,FAKE-pw-5,\n,https://noname.example.invalid/,f,g,\n";
    let p = import::parse(ImportFormat::ChromeCsv, csv, None).unwrap();
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].username.as_deref(), Some("erin"));
    assert_eq!(p.items[1].title, "https://noname.example.invalid/"); // falls back to url
    assert!(import::parse(ImportFormat::ChromeCsv, "foo,bar\n1,2\n", None).is_err());
}

#[test]
fn import_into_vault_dedup_dry_run() {
    let (_d, s, v) = vault();
    let csv = "name,url,username,password\nA,https://a.invalid,u,p1\nA,https://a.invalid,u,p2\nB,https://b.invalid,u,p3\n";
    let parse = || import::parse(ImportFormat::ChromeCsv, csv, None).unwrap();
    let r = import::import_items(&s, &v, parse(), ImportOpts { dry_run: true, allow_duplicates: false }).unwrap();
    assert_eq!((r.parsed, r.added, r.duplicates.len()), (3, 2, 1));
    assert_eq!(s.list_items(&v).unwrap().len(), 0);
    let r = import::import_items(&s, &v, parse(), ImportOpts::default()).unwrap();
    assert_eq!((r.added, r.duplicates.len()), (2, 1));
    assert_eq!(s.list_items(&v).unwrap().len(), 2);
    // re-import: all duplicates of existing items
    let r = import::import_items(&s, &v, parse(), ImportOpts::default()).unwrap();
    assert_eq!((r.added, r.duplicates.len()), (0, 3));
    let r = import::import_items(&s, &v, parse(), ImportOpts { dry_run: false, allow_duplicates: true }).unwrap();
    assert_eq!(r.added, 3);
    assert_eq!(s.list_items(&v).unwrap().len(), 5);
}

fn sample_items() -> Vec<Item> {
    let mut a = item("Round, \"Trip\"", Some("FAKE-pw,\"q\"\n"), Some("https://rt.invalid"));
    a.notes = Some("line1\nline2".into()); a.totp = Some("JBSWY3DPEHPK3PXP".into()); a.updated_at = "1600000000".into();
    vec![a, item("Second", None, None)]
}

#[test]
fn export_import_roundtrip_plain_json_and_csv() {
    let (_d, s, v) = vault();
    for it in sample_items() { s.add_item(&v, &it).unwrap(); }
    let items: Vec<Item> = s.list_items(&v).unwrap().into_iter().map(|(_, i)| i).collect();
    let json = import::export_json(&items).unwrap();
    let back = import::parse(ImportFormat::GitpassJson, &json, None).unwrap().items;
    assert_eq!(back, items);
    let (_d2, s2, v2) = vault();
    let r = import::import_items(&s2, &v2, import::parse(ImportFormat::GitpassJson, &json, None).unwrap(), ImportOpts::default()).unwrap();
    assert_eq!(r.added, 2);
    let mut got: Vec<Item> = s2.list_items(&v2).unwrap().into_iter().map(|(_, i)| i).collect();
    let mut want = items.clone();
    got.sort_by(|a, b| a.title.cmp(&b.title)); want.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(got, want);
    // CSV export is readable back through the tolerant 1password-csv mapper (title/username/password/url/notes/totp)
    let csv = import::export_csv(&items).unwrap();
    let back = import::parse(ImportFormat::OnePasswordCsv, &csv, None).unwrap().items;
    let rt = back.iter().find(|i| i.title == "Round, \"Trip\"").unwrap();
    assert_eq!(rt.password.as_deref(), Some("FAKE-pw,\"q\"")); // CSV path trims surrounding whitespace
    assert!(rt.notes.as_deref().unwrap().starts_with("line1\nline2"));
}

#[test]
fn export_import_roundtrip_encrypted() {
    let items = sample_items();
    let enc = import::export_encrypted(&items, "fake export passphrase", &fast()).unwrap();
    assert!(import::is_encrypted_export(&enc));
    assert!(!enc.contains("FAKE-pw") && !enc.contains("Round"));
    assert!(import::parse(ImportFormat::GitpassJson, &enc, None).is_err());
    assert!(import::parse(ImportFormat::GitpassJson, &enc, Some("wrong passphrase!")).is_err());
    let back = import::parse(ImportFormat::GitpassJson, &enc, Some("fake export passphrase")).unwrap().items;
    assert_eq!(back, items);
    // tamper
    let mut v: serde_json::Value = serde_json::from_str(&enc).unwrap();
    v["kdf"]["t"] = 2.into();
    assert!(import::parse(ImportFormat::GitpassJson, &v.to_string(), Some("fake export passphrase")).is_err());
    assert!(import::export_encrypted(&items, "short", &fast()).is_err());
}

fn rows(items: Vec<Item>) -> Vec<(String, String, Item)> {
    items.into_iter().enumerate().map(|(i, it)| ("v".into(), format!("id{i}"), it)).collect()
}

#[test]
fn audit_weak_reused_old_missing_totp() {
    let now = 2_000_000_000u64;
    let strong = "Xq7#vR2!mK9@pL4$wZ";
    let mut old = item("Old", Some("Tr0ub4dor&3-Horse-Battery"), None); old.updated_at = (now - 400 * 86400).to_string();
    let mut fresh_totp = item("WithTotp", Some("Another#Strong9Pass!x"), Some("https://t.invalid")); fresh_totp.totp = Some("JBSWY3DPEHPK3PXP".into());
    let mut items = vec![
        item("Short", Some("abc12"), None),
        item("Common", Some("MyPassword2024!!"), None),
        item("ReuseA", Some(strong), Some("https://a.invalid")),
        item("ReuseB", Some(strong), None),
        old, fresh_totp,
    ];
    for it in items.iter_mut() { if it.title != "Old" { it.updated_at = (now - 10 * 86400).to_string(); } }
    let o = AuditOpts { now_secs: now, ..Default::default() };
    let r = audit::audit(&rows(items), &o, None).unwrap();
    let f = |t: &str| r.per_item.iter().find(|x| x.title == t).map(|x| x.findings.clone()).unwrap_or_default();
    assert!(matches!(f("Short")[0], Finding::Weak { .. }));
    assert!(matches!(f("Common")[0], Finding::Weak { .. }));
    assert!(f("ReuseA").iter().any(|x| matches!(x, Finding::Reused { others: 1, .. })));
    assert!(f("ReuseA").contains(&Finding::MissingTotp));
    assert!(f("ReuseB").iter().any(|x| matches!(x, Finding::Reused { .. })));
    assert!(f("Old").contains(&Finding::Old { days: 400 }));
    assert!(f("WithTotp").is_empty());
    assert_eq!((r.weak, r.reused, r.reused_groups, r.old, r.missing_totp, r.pwned), (2, 2, 1, 1, 1, 0));
    // findings never contain passwords
    assert!(!format!("{:?}", r.per_item).contains(strong));
    // custom age threshold
    let r = audit::audit(&rows(vec![item("x", Some(strong), None)]), &AuditOpts { max_age_days: 0, now_secs: store::now().parse::<u64>().unwrap() + 2 * 86400, ..Default::default() }, None).unwrap();
    assert_eq!(r.old, 1);
}

struct MockHibp { body: String, calls: RefCell<Vec<String>> }
impl PwnedRangeClient for MockHibp {
    fn range(&self, prefix: &str) -> anyhow::Result<String> { self.calls.borrow_mut().push(prefix.into()); Ok(self.body.clone()) }
}

#[test]
fn hibp_k_anonymity_with_mock() {
    // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
    let mock = MockHibp {
        body: "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:3861493\r\nFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF:0\r\n".into(),
        calls: RefCell::new(vec![]),
    };
    let mut cache = Default::default();
    assert_eq!(audit::pwned_count(&mock, "password", &mut cache).unwrap(), 3861493);
    assert_eq!(mock.calls.borrow().as_slice(), ["5BAA6"]);
    // second lookup same prefix is cached; unknown password -> 0
    assert_eq!(audit::pwned_count(&mock, "password", &mut cache).unwrap(), 3861493);
    assert_eq!(mock.calls.borrow().len(), 1);
    let none = MockHibp { body: "0018A45C4D1DEF81644B54AB7F969B88D65:0\n".into(), calls: RefCell::new(vec![]) };
    assert_eq!(audit::pwned_count(&none, "Xq7#vR2!mK9@pL4$wZ", &mut Default::default()).unwrap(), 0);
    assert_eq!(none.calls.borrow()[0].len(), 5);
    // through audit()
    let r = audit::audit(&rows(vec![item("pw", Some("password"), None), item("ok", Some("Xq7#vR2!mK9@pL4$wZ"), None)]), &AuditOpts::default(), Some(&mock)).unwrap();
    assert_eq!(r.pwned, 1);
    assert!(r.per_item.iter().find(|x| x.title == "pw").unwrap().findings.contains(&Finding::Pwned { count: 3861493 }));
    // client errors propagate
    struct Fail; impl PwnedRangeClient for Fail { fn range(&self, _: &str) -> anyhow::Result<String> { anyhow::bail!("offline") } }
    assert!(audit::audit(&rows(vec![item("pw", Some("x"), None)]), &AuditOpts::default(), Some(&Fail)).is_err());
}

#[test]
fn entropy_and_iso() {
    assert!(audit::entropy_bits("aaaa") < audit::entropy_bits("aA1!"));
    assert_eq!(import::iso_to_epoch("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(import::iso_to_epoch("2000-03-01"), Some(951868800));
    assert_eq!(import::iso_to_epoch("garbage"), None);
}
