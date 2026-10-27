//! v4 data features: importers, export round trip, audit, HIBP (mocked). All fixture data is FAKE.
use gitpass_core::audit::{self, AuditOpts, Finding, PwnedRangeClient};
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat, ImportOpts};
use gitpass_core::store::{self, Item, Session};
use std::cell::RefCell;
use std::collections::BTreeSet;

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

fn by_title<'a>(items: &'a [gitpass_core::merge::ItemRecord], t: &str) -> &'a Item { items.iter().find(|i| i.item.title == t).map(|r| &r.item).unwrap() }
fn rec<'a>(items: &'a [gitpass_core::merge::ItemRecord], t: &str) -> &'a gitpass_core::merge::ItemRecord { items.iter().find(|i| i.item.title == t).unwrap() }

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
    assert_eq!(n, "fake note");
    let r = rec(&p.items, "Example Login");
    assert_eq!(r.tags, vec!["Fake Folder"]);
    assert_eq!(r.find_field("custom1").and_then(|f| f.value).as_deref(), Some("0000"));
    assert_eq!(r.find_field("url2").and_then(|f| f.value).as_deref(), Some("https://alt.example.invalid"));
    let card = rec(&p.items, "Fake Card");
    assert_eq!(card.category, gitpass_core::item::Category::CreditCard);
    assert_eq!(card.find_field("type").and_then(|f| f.value).as_deref(), Some("Visa"));
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
    assert!(n.contains("hello") && n.contains("fields: Recovery: abc"), "{n}");
    assert_eq!(rec(&p.items, "Fake Mail").tags, vec!["Work"]);
    assert_eq!(rec(&p.items, "Fake Note").category, gitpass_core::item::Category::SecureNote);
}

#[test]
fn import_1password_csv_case_insensitive_and_bom() {
    let csv = "\u{feff}Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
               Fake Shop,https://shop.example.invalid,carol,FAKE-pw-3,otpauth://totp/x?secret=JBSWY3DPEHPK3PXP,false,false,shopping,n1\n";
    let p = import::parse(ImportFormat::OnePasswordCsv, csv, None).unwrap();
    let i = &p.items[0].item;
    assert_eq!(i.title, "Fake Shop");
    assert_eq!(i.url.as_deref(), Some("https://shop.example.invalid"));
    assert!(i.totp.as_deref().unwrap().starts_with("otpauth://"));
    assert_eq!(i.notes.as_deref(), Some("n1"));
    assert_eq!(p.items[0].tags, vec!["shopping"]);
    // lower-case / older header variants
    let p2 = import::parse(ImportFormat::OnePasswordCsv, "title,website,username,password,notesPlain\nX,https://x.invalid,u,p,nn\n", None).unwrap();
    assert_eq!(p2.items[0].item.url.as_deref(), Some("https://x.invalid"));
    assert_eq!(p2.items[0].item.notes.as_deref(), Some("nn"));
}

#[test]
fn import_keepassxc_csv() {
    let csv = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\",\"Icon\",\"Last Modified\",\"Created\"\n\
               \"Root/Fake\",\"Fake Forum\",\"dave\",\"FAKE-pw-4\",\"https://forum.example.invalid\",\"multi\nline\",\"\",\"0\",\"2021-06-01T00:00:00Z\",\"2021-01-01T00:00:00Z\"\n";
    let p = import::parse(ImportFormat::KeepassCsv, csv, None).unwrap();
    let i = &p.items[0].item;
    assert_eq!((i.title.as_str(), i.password.as_deref(), i.totp.as_deref()), ("Fake Forum", Some("FAKE-pw-4"), None));
    assert_eq!(i.updated_at, "1622505600");
    let n = i.notes.as_deref().unwrap();
    assert!(n.contains("multi\nline") && !n.contains("Group") && !n.contains("Icon"), "{n}");
}

#[test]
fn import_chrome_csv() {
    let csv = "name,url,username,password,note\nlogin.example.invalid,https://login.example.invalid/,erin,FAKE-pw-5,\n,https://noname.example.invalid/,f,g,\n";
    let p = import::parse(ImportFormat::ChromeCsv, csv, None).unwrap();
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].item.username.as_deref(), Some("erin"));
    assert_eq!(p.items[1].item.title, "https://noname.example.invalid/"); // falls back to url
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
    let back: Vec<Item> = import::parse(ImportFormat::GitpassJson, &json, None).unwrap().items.into_iter().map(|r| r.item.clone()).collect();
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
    let rt = by_title(&back, "Round, \"Trip\"");
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
    let back: Vec<Item> = import::parse(ImportFormat::GitpassJson, &enc, Some("fake export passphrase")).unwrap().items.into_iter().map(|r| r.item.clone()).collect();
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

#[test]
fn audit_entropy_edge_cases_unicode_long_and_empty() {
    // Empty input: no character classes, no length.
    assert_eq!(audit::entropy_bits(""), 0.0);
    // Pool sizes: 26 lowercase + 26 uppercase + 10 digits + 33 other ASCII.
    assert!((audit::entropy_bits("a") - 26f64.log2()).abs() < 1e-9);
    assert!((audit::entropy_bits("aA1!") - 4.0 * 95f64.log2()).abs() < 1e-9);
    // Symbols contribute their pool once, not per character.
    assert!((audit::entropy_bits("!!!") - 3.0 * 33f64.log2()).abs() < 1e-9);
    // Non-ASCII characters share one 100-character pool, counted per char not per UTF-8 byte
    // ("秘密" is 2 chars / 6 bytes).
    assert!((audit::entropy_bits("秘密") - 2.0 * 100f64.log2()).abs() < 1e-9);
    // Very long input stays finite and linear in length.
    let long = "a".repeat(10_000);
    let bits = audit::entropy_bits(&long);
    assert!(bits.is_finite() && (bits - 10_000.0 * 26f64.log2()).abs() < 1e-6, "{bits}");
}

#[test]
fn audit_weakness_edge_cases_unicode_long_and_empty() {
    let o = AuditOpts::default();
    // audit() skips empty passwords; this is only the direct contract.
    assert_eq!(audit::weakness("", &o), Some("too short (0 < 12)".into()));
    // Per-character entropy: a 2-char non-ASCII secret is low entropy despite 6 UTF-8 bytes.
    let per_char = AuditOpts { min_length: 1, ..Default::default() };
    assert_eq!(audit::weakness("秘密", &per_char), Some("low estimated entropy (~13 bits)".into()));
    // 12 Cyrillic chars clear both the length threshold and the naive 100-char pool estimate.
    assert_eq!(audit::weakness("парольпароль", &o), None);
    // Very long mixed input: enough distinct chars, no common substring, huge estimate.
    let long = "Xq7#".repeat(2_500);
    assert_eq!(long.chars().count(), 10_000);
    assert_eq!(audit::weakness(&long, &o), None);
    // Long Unicode input is still caught by the distinct-character rule.
    assert_eq!(audit::weakness(&"密".repeat(20), &o), Some("too few distinct characters".into()));
}

#[test]
fn audit_reuse_spans_vaults_and_ignores_empty_passwords() {
    let now = 2_000_000_000u64;
    let shared = "Xq7#vR2!mK9@pL4$wZ";
    let mk = |title: &str, pw: Option<&str>| { let mut it = item(title, pw, None); it.updated_at = (now - 10 * 86400).to_string(); it };
    let rows = vec![
        ("vault-1".to_string(), "a".to_string(), mk("A", Some(shared))),
        ("vault-2".to_string(), "b".to_string(), mk("B", Some(shared))),
        ("vault-2".to_string(), "c".to_string(), mk("C", Some(shared))),
        ("vault-1".to_string(), "d".to_string(), mk("D", Some("Another#Strong9Pass!x"))),
        ("vault-1".to_string(), "e1".to_string(), mk("E1", Some(""))),
        ("vault-2".to_string(), "e2".to_string(), mk("E2", Some(""))),
        ("vault-1".to_string(), "n1".to_string(), mk("N1", None)),
    ];
    let r = audit::audit(&rows, &AuditOpts { now_secs: now, ..Default::default() }, None).unwrap();
    let findings = |v: &str, id: &str| r.per_item.iter().find(|x| x.vault == v && x.id == id).map(|x| x.findings.clone()).unwrap_or_default();
    let group = |v: &str, id: &str| findings(v, id).into_iter().find_map(|x| match x {
        Finding::Reused { group, others } => Some((group, others)),
        _ => None,
    });
    // One group of three across two vaults; each item sees the other two.
    let a = group("vault-1", "a").expect("A is reused");
    assert_eq!(a.1, 2);
    assert_eq!(group("vault-2", "b"), Some(a));
    assert_eq!(group("vault-2", "c"), Some(a));
    // A different password and empty/missing passwords are never grouped.
    assert_eq!(group("vault-1", "d"), None);
    assert_eq!(group("vault-1", "e1"), None);
    assert_eq!(group("vault-2", "e2"), None);
    assert_eq!(group("vault-1", "n1"), None);
    assert_eq!((r.reused, r.reused_groups), (3, 1));
    // Findings never contain the shared password.
    assert!(!format!("{:?}", r.per_item).contains(shared));
}

#[test]
fn mfa_site_list_parsing_and_lookup() {
    let sites = audit::parse_mfa_site_list(
        "# comment\nExample.TEST\ttotp,passkey\nWWW.example.test\tpasskey\nfoo.test\ttotp\nbad line without tab\nempty.test\t\n");
    assert_eq!(sites.len(), 2);
    assert_eq!((sites[0].0.as_str(), sites[0].1.totp, sites[0].1.passkey), ("example.test", true, true));
    assert_eq!((sites[1].0.as_str(), sites[1].1.totp, sites[1].1.passkey), ("foo.test", true, false));
    let hit = audit::lookup_mfa_site(&sites, "https://Login.Example.test:8443/signin?q=1");
    assert_eq!(hit.map(|(d, s)| (d, s.totp, s.passkey)), Some(("example.test", true, true)));
    assert!(audit::lookup_mfa_site(&sites, "https://example.test.").is_some());
    assert!(audit::lookup_mfa_site(&sites, "user:fake@sub.foo.test").is_some());
    assert!(audit::lookup_mfa_site(&sites, "https://notexample.test").is_none());
    assert!(audit::lookup_mfa_site(&sites, "https://example.test.evil.invalid").is_none());
    assert!(audit::lookup_mfa_site(&sites, "   ").is_none());
}

const BARE_PUBLIC_SUFFIXES: &[&str] = &[
    "com", "org", "net", "edu", "gov", "mil", "int", "io", "me", "tv", "us", "uk", "de", "jp", "ca", "au",
    "co.uk", "org.uk", "gov.uk", "ac.uk", "com.au", "net.au", "org.au", "co.jp", "co.nz", "com.br",
    "github.io", "gitlab.io", "pages.dev", "vercel.app", "herokuapp.com", "azurewebsites.net", "cloudfront.net",
];

fn is_bare_public_suffix(domain: &str) -> bool {
    let d = domain.trim().trim_matches('.').to_ascii_lowercase();
    if !d.contains('.') { return true; } // bare TLD (e.g. "com", "io")
    BARE_PUBLIC_SUFFIXES.iter().any(|&s| d == s)
}

#[test]
fn reject_bare_public_suffixes() {
    for bare in ["com", "co.uk", "github.io", "pages.dev", "herokuapp.com"] {
        assert!(is_bare_public_suffix(bare), "expected {bare} to be rejected as bare public suffix");
    }
    for valid in ["google.com", "live.com", "microsoftonline.com", "mysite.github.io", "sub.example.co.uk"] {
        assert!(!is_bare_public_suffix(valid), "expected {valid} to be accepted as registrable domain");
    }
}

#[test]
fn longest_match_wins_over_first_sorted_match() {
    // accounts.example.test is more specific than example.test
    let sites = audit::parse_mfa_site_list("example.test\ttotp\naccounts.example.test\ttotp,passkey\n");
    let hit_sub = audit::lookup_mfa_site(&sites, "https://accounts.example.test/signin");
    assert_eq!(hit_sub.map(|(d, s)| (d, s.totp, s.passkey)), Some(("accounts.example.test", true, true)));

    let hit_apex = audit::lookup_mfa_site(&sites, "https://example.test/login");
    assert_eq!(hit_apex.map(|(d, s)| (d, s.totp, s.passkey)), Some(("example.test", true, false)));

    let hit_other = audit::lookup_mfa_site(&sites, "https://other.example.test/login");
    assert_eq!(hit_other.map(|(d, s)| (d, s.totp, s.passkey)), Some(("example.test", true, false)));
}

#[test]
fn passkey_suppression_across_subdomains_and_registrable_domain() {
    let now = 2_000_000_000u64;
    let sites = audit::parse_mfa_site_list("google.com\ttotp,passkey\n");
    let strong = "Xq7#vR2!mK9@pL4$wZ";
    let items = vec![
        item("ApexLogin", Some(strong), Some("https://google.com/login")),
        item("SubLogin", Some(strong), Some("https://mail.google.com/")),
    ];
    let mut passkeys = BTreeSet::new();
    // Passkey registered under accounts.google.com
    passkeys.insert("accounts.google.com".to_string());
    let o = AuditOpts { now_secs: now, ..Default::default() };
    let r = audit::audit_with_sites(&rows(items), &passkeys, &sites, &o, None).unwrap();
    let f = |t: &str| r.per_item.iter().find(|x| x.title == t).map(|x| x.findings.clone()).unwrap_or_default();
    // Both logins are under google.com, so stored passkey on accounts.google.com suppresses passkey advice for both.
    assert!(!f("ApexLogin").iter().any(|x| matches!(x, Finding::PasskeyAvailable { .. })));
    assert!(!f("SubLogin").iter().any(|x| matches!(x, Finding::PasskeyAvailable { .. })));
}

#[test]
fn bundled_mfa_site_list_is_well_formed() {
    let src = include_str!("../data/mfa-sites.tsv");
    let data_lines = src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).count();
    let sites = audit::mfa_sites();
    assert_eq!(sites.len(), data_lines, "every bundled data line must parse");
    assert!(sites.len() >= 20);
    assert!(sites.iter().all(|(d, s)| !d.is_empty() && !d.contains('/') && (s.totp || s.passkey)));
    for (d, _) in sites {
        assert!(!is_bare_public_suffix(d), "bare public suffix or TLD in bundled list: {d}");
    }
    assert!(sites.iter().any(|(d, s)| d == "github.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "apple.com" && s.passkey && !s.totp));
    assert!(sites.iter().any(|(d, s)| d == "walmart.com" && s.passkey && !s.totp));
    assert!(sites.iter().any(|(d, s)| d == "live.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "microsoftonline.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "paypal.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "ebay.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "bestbuy.com" && s.totp && s.passkey));
    assert!(sites.iter().any(|(d, s)| d == "uber.com" && s.totp && s.passkey));
    assert!(!sites.iter().any(|(d, _)| d == "steamcommunity.com"), "steamcommunity.com should be dropped (non-standard TOTP)");
    let mut domains: Vec<&str> = sites.iter().map(|(d, _)| d.as_str()).collect();
    let n = domains.len(); domains.sort_unstable(); domains.dedup(); assert_eq!(domains.len(), n);
    // Bundled lookups: subdomains and case match, lookalikes do not.
    assert_eq!(audit::site_support("https://accounts.GitHub.com/login").map(|(d, _)| d), Some("github.com"));
    assert!(audit::site_support("https://github.com.evil.invalid").is_none());
}

#[test]
fn audit_flags_totp_and_passkey_availability_offline() {
    let now = 2_000_000_000u64;
    let sites = audit::parse_mfa_site_list("example.test\ttotp,passkey\npasskey.test\tpasskey\ntotp.test\ttotp\n");
    let strong = "Xq7#vR2!mK9@pL4$wZ";
    let mut with_totp = item("WithTotp", Some(strong), Some("https://example.test"));
    with_totp.totp = Some("JBSWY3DPEHPK3PXP".into());
    let mut passkey_item = item("PasskeySite", Some(strong), Some("https://passkey.test"));
    passkey_item.totp = Some("JBSWY3DPEHPK3PXP".into());
    let mut items = vec![
        item("TotpMissing", Some(strong), Some("https://login.example.test/signin")),
        with_totp,
        passkey_item,
        item("TotpOnly", Some(strong), Some("https://accounts.totp.test")),
        item("Lookalike", Some(strong), Some("https://example.test.evil.invalid")),
        item("Unknown", Some(strong), Some("https://unknown.invalid")),
        item("NoUrl", Some(strong), None),
    ];
    for it in items.iter_mut() { it.updated_at = (now - 10 * 86400).to_string(); }
    let mut passkeys = BTreeSet::new();
    passkeys.insert("passkey.test".to_string());
    let o = AuditOpts { now_secs: now, ..Default::default() };
    let r = audit::audit_with_sites(&rows(items), &passkeys, &sites, &o, None).unwrap();
    let f = |t: &str| r.per_item.iter().find(|x| x.title == t).map(|x| x.findings.clone()).unwrap_or_default();
    assert!(f("TotpMissing").contains(&Finding::TotpAvailable { site: "example.test".into() }));
    assert!(f("TotpMissing").contains(&Finding::PasskeyAvailable { site: "example.test".into() }));
    assert!(!f("TotpMissing").contains(&Finding::MissingTotp));
    // TOTP present: no TOTP advice, but a passkey is still suggested (no passkey recorded for example.test).
    assert!(!f("WithTotp").iter().any(|x| matches!(x, Finding::TotpAvailable { .. })));
    assert!(f("WithTotp").contains(&Finding::PasskeyAvailable { site: "example.test".into() }));
    // A stored passkey for the site suppresses the passkey advice.
    assert!(!f("PasskeySite").iter().any(|x| matches!(x, Finding::PasskeyAvailable { .. })));
    assert!(f("TotpOnly").contains(&Finding::TotpAvailable { site: "totp.test".into() }));
    assert!(!f("TotpOnly").iter().any(|x| matches!(x, Finding::PasskeyAvailable { .. })));
    assert!(!f("TotpOnly").contains(&Finding::MissingTotp));
    // Lookalikes / unknown sites get the generic missing-TOTP finding but no site advice.
    for t in ["Lookalike", "Unknown"] {
        assert!(f(t).contains(&Finding::MissingTotp));
        assert!(!f(t).iter().any(|x| matches!(x, Finding::TotpAvailable { .. } | Finding::PasskeyAvailable { .. })));
    }
    assert!(!f("NoUrl").iter().any(|x| matches!(x, Finding::MissingTotp | Finding::TotpAvailable { .. } | Finding::PasskeyAvailable { .. })));
    assert_eq!((r.totp_available, r.passkey_available, r.missing_totp), (2, 2, 2));
    // A passkey rp_id on a subdomain (login.example.test) also suppresses for parent domain (example.test).
    let mut sub_only = BTreeSet::new();
    sub_only.insert("login.example.test".to_string());
    let mut parent = item("ParentHost", Some(strong), Some("https://example.test"));
    parent.updated_at = (now - 10 * 86400).to_string();
    let r = audit::audit_with_sites(&rows(vec![parent.clone()]), &sub_only, &sites, &o, None).unwrap();
    let parent_findings = r.per_item.iter().find(|x| x.title == "ParentHost").map(|x| x.findings.clone()).unwrap_or_default();
    assert!(!parent_findings.iter().any(|x| matches!(x, Finding::PasskeyAvailable { .. })));
    // Unrelated passkey domain does not suppress passkey advice.
    let mut unrelated = BTreeSet::new();
    unrelated.insert("other.test".to_string());
    let r = audit::audit_with_sites(&rows(vec![parent]), &unrelated, &sites, &o, None).unwrap();
    let parent_findings = r.per_item.iter().find(|x| x.title == "ParentHost").map(|x| x.findings.clone()).unwrap_or_default();
    assert!(parent_findings.contains(&Finding::PasskeyAvailable { site: "example.test".into() }));
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
