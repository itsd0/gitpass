//! Import every sample export under tests/fixtures/import (FAKE data only) into a fresh temp vault
//! and check the stored records field by field.
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat, ImportOpts};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Session};
use std::path::PathBuf;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn fixture(name: &str) -> Vec<u8> { std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import").join(name)).unwrap() }

/// Import `file` as `fmt` into a fresh vault; returns (report, stored records).
fn import_fixture(fmt: ImportFormat, file: &str) -> (import::ImportReport, Vec<ItemRecord>) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "fake-master-pw", fast()).unwrap();
    let s = Session::unlock(d.path(), "fake-master-pw", &sk).unwrap();
    let v = s.create_vault("import-test").unwrap();
    let other = s.create_vault("other").unwrap();
    let parsed = import::parse_bytes(fmt, &fixture(file), None).unwrap();
    let rep = import::import_items(&s, &v, parsed, ImportOpts::default()).unwrap();
    assert!(s.list_records(&other).unwrap().is_empty(), "import must only write into the selected vault");
    let recs = s.list_records(&v).unwrap().into_iter().map(|(_, _, r)| r).collect();
    (rep, recs)
}

fn by<'a>(recs: &'a [ItemRecord], title: &str) -> &'a ItemRecord { recs.iter().find(|r| r.item.title == title).unwrap_or_else(|| panic!("no item {title}")) }
fn field(r: &ItemRecord, id: &str) -> Option<String> { r.find_field(id).and_then(|f| f.value) }
fn kind(r: &ItemRecord, id: &str) -> FieldKind { r.find_field(id).unwrap().kind }
const OTP_URI: &str = "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";

/// The shared "Example Mail" login (commas, quotes, newline in notes, TOTP) and the Burmese login.
fn check_common(recs: &[ItemRecord], with_totp_2: bool) {
    let m = by(recs, "Example Mail");
    assert_eq!(m.category, Category::Login);
    assert_eq!(m.item.username.as_deref(), Some("alice@example.com"));
    assert_eq!(m.item.password.as_deref(), Some("Fake-P@ss, \"quoted\""));
    assert_eq!(m.item.url.as_deref(), Some("https://mail.example.com/login"));
    assert!(m.item.notes.as_deref().unwrap().starts_with("Line one, with comma\nLine \"two\""), "{:?}", m.item.notes);
    // TOTP is stored in the legacy `totp` slot and surfaces in the GUI view as a Totp field with id "totp"
    assert_eq!(m.item.totp.as_deref(), Some(OTP_URI));
    assert_eq!(kind(m, "totp"), FieldKind::Totp);
    gitpass_core::totp::parse(m.item.totp.as_deref().unwrap()).unwrap();
    let b = by(recs, "ဘဏ် အကောင့်");
    assert_eq!(b.item.username.as_deref(), Some("မောင်မောင်"));
    assert_eq!(b.item.password.as_deref(), Some("ပစ်စကား123"));
    assert_eq!(b.item.url.as_deref().map(|u| u.trim_end_matches('/')), Some("https://bank.example.mm"));
    assert!(b.item.notes.as_deref().unwrap().starts_with("မြန်မာစာ မှတ်စု"));
    if with_totp_2 { gitpass_core::totp::parse(b.item.totp.as_deref().unwrap()).unwrap(); }
}

#[test]
fn bitwarden_json_fixture() {
    let (rep, recs) = import_fixture(ImportFormat::BitwardenJson, "bitwarden.json");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len()), (6, 5, 1));
    check_common(&recs, true);
    let m = by(&recs, "Example Mail");
    assert!(m.favorite);
    assert_eq!(m.tags, vec!["Work"]);
    assert_eq!(field(m, "url2").as_deref(), Some("https://example.com"));
    let pin = m.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.label == "Recovery PIN").unwrap();
    assert_eq!((pin.kind, pin.value.as_deref()), (FieldKind::Concealed, Some("4242"))); // hidden field stays concealed
    assert_eq!(m.item.updated_at, "1741944413");
    assert_eq!(by(&recs, "ဘဏ် အကောင့်").tags, vec!["မိသားစု"]);
    let c = by(&recs, "Fake Visa");
    assert_eq!(c.category, Category::CreditCard);
    assert_eq!(field(c, "number").as_deref(), Some("4111111111111111"));
    assert_eq!(kind(c, "number"), FieldKind::Concealed);
    assert_eq!(field(c, "cvv").as_deref(), Some("123"));
    assert_eq!(field(c, "expiry").as_deref(), Some("07/2030"));
    assert_eq!(field(c, "cardholder").as_deref(), Some("Alice Example"));
    assert!(c.item.notes.is_none(), "card data must not leak into plaintext notes");
    let i = by(&recs, "Alice ID");
    assert_eq!(i.category, Category::Identity);
    assert_eq!((field(i, "first").as_deref(), field(i, "last").as_deref()), (Some("Alice"), Some("Example")));
    assert_eq!(field(i, "email").as_deref(), Some("alice@example.com"));
    assert_eq!(field(i, "address").as_deref(), Some("1 Fake St\nYangon 11181\nMM"));
    assert_eq!(field(i, "passport").as_deref(), Some("MA000000"));
    let n = by(&recs, "Wi-Fi notes");
    assert_eq!(n.category, Category::SecureNote);
    assert_eq!(n.item.notes.as_deref(), Some("SSID: FakeNet\nKey: not-real"));
}

#[test]
fn bitwarden_csv_fixture() {
    let (rep, recs) = import_fixture(ImportFormat::BitwardenCsv, "bitwarden.csv");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len()), (5, 4, 1));
    check_common(&recs, true);
    let m = by(&recs, "Example Mail");
    assert!(m.favorite);
    assert_eq!(m.tags, vec!["Work"]);
    assert!(m.item.notes.as_deref().unwrap().contains("fields: Recovery PIN: 4242"));
    assert_eq!(by(&recs, "Wi-Fi notes").category, Category::SecureNote);
    assert!(by(&recs, "No folder").tags.is_empty());
}

#[test]
fn onepassword_csv_fixture() {
    let (rep, recs) = import_fixture(ImportFormat::OnePasswordCsv, "1password.csv");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len()), (5, 4, 1));
    check_common(&recs, false);
    let m = by(&recs, "Example Mail");
    assert!(m.favorite);
    assert_eq!(m.tags, vec!["Work", "Email"]);
    assert!(!m.item.notes.as_deref().unwrap().contains("Tags:"));
    let old = by(&recs, "Old login");
    assert!(old.archived);
    assert_eq!(old.item.notes.as_deref(), Some("archived item"));
    let e = by(&recs, "Empty fields");
    assert!(e.item.username.is_none() && e.item.password.is_none() && e.item.notes.is_none());
}

#[test]
fn onepassword_1pux_fixture() {
    let (rep, recs) = import_fixture(ImportFormat::OnePassword1pux, "1password.1pux");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len()), (11, 10, 1));
    check_common(&recs, false);
    let m = by(&recs, "Example Mail");
    assert!(m.favorite);
    assert_eq!(m.tags, vec!["Private", "Work", "Email"]);
    assert_eq!(m.item.updated_at, "1741944413");
    assert_eq!(m.created_at, Some(1704164645));
    assert_eq!(field(m, "url1").as_deref(), Some("https://example.com"));
    let pin = m.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.label == "Recovery PIN").unwrap();
    assert_eq!((pin.kind, pin.value.as_deref()), (FieldKind::Concealed, Some("4242")));
    let c = by(&recs, "Fake Visa");
    assert_eq!(c.category, Category::CreditCard);
    assert_eq!(field(c, "number").as_deref(), Some("4111111111111111"));
    assert_eq!(field(c, "cvv").as_deref(), Some("123"));
    assert_eq!(field(c, "expiry").as_deref(), Some("07/2030"));
    assert_eq!(field(c, "cardholder").as_deref(), Some("Alice Example"));
    let i = by(&recs, "Alice ID");
    assert_eq!(i.category, Category::Identity);
    assert_eq!(field(i, "first").as_deref(), Some("Alice"));
    assert_eq!(field(i, "birth").as_deref(), Some("1990-01-01"));
    assert_eq!(field(i, "email").as_deref(), Some("alice@example.com"));
    assert_eq!(field(i, "address").as_deref(), Some("1 Fake St\nYangon\n11181\nmm"));
    assert_eq!(by(&recs, "Plain note").category, Category::SecureNote);
    let p = by(&recs, "Old password");
    assert_eq!((p.category, p.item.password.as_deref(), p.archived), (Category::Login, Some("old-fake-password"), true));
    let w = by(&recs, "Home Wi-Fi");
    assert_eq!(w.category, Category::Wifi);
    assert_eq!((field(w, "ssid").as_deref(), field(w, "password").as_deref()), (Some("FakeNet"), Some("not-a-real-wifi-pass")));
    let k = by(&recs, "Fake SSH key");
    assert_eq!(k.category, Category::SshKey);
    assert!(field(k, "private_key").unwrap().contains("FAKEFAKEFAKE"));
    assert_eq!(field(k, "public_key").as_deref(), Some("ssh-ed25519 AAAAFAKE"));
    let a = by(&recs, "Fake API");
    assert_eq!(a.category, Category::ApiCredential);
    assert_eq!(field(a, "credential").as_deref(), Some("fake-api-token-000"));
    let d = by(&recs, "Passport scan");
    assert_eq!(d.category, Category::Document);
    assert!(d.item.notes.as_deref().unwrap().contains("fake-passport.pdf"));
}

#[test]
fn keepass_csv_fixtures() {
    let (rep, recs) = import_fixture(ImportFormat::KeepassCsv, "keepass.csv");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len(), rep.warnings.len()), (3, 2, 1, 1));
    check_common(&recs, false);
    assert_eq!(by(&recs, "Example Mail").tags, vec!["Work"]);
    assert_eq!(by(&recs, "ဘဏ် အကောင့်").tags, vec!["မိသားစု"]);
    assert_eq!(by(&recs, "Example Mail").item.updated_at, "1741944413");
    // KeePass 2 generic CSV headers
    let (rep, recs) = import_fixture(ImportFormat::KeepassCsv, "keepass2.csv");
    assert_eq!(rep.added, 2);
    let m = by(&recs, "Example Mail");
    assert_eq!((m.item.username.as_deref(), m.item.password.as_deref()), (Some("alice@example.com"), Some("Fake-P@ss, \"quoted\"")));
    assert_eq!(m.item.notes.as_deref(), Some("Line one, with comma\nLine \"two\""));
}

#[test]
fn chrome_csv_fixture() {
    let (rep, recs) = import_fixture(ImportFormat::ChromeCsv, "chrome.csv");
    assert_eq!((rep.parsed, rep.added, rep.duplicates.len()), (4, 3, 1));
    let m = by(&recs, "mail.example.com");
    assert_eq!(m.item.password.as_deref(), Some("Fake-P@ss, \"quoted\""));
    assert_eq!(m.item.notes.as_deref(), Some("Line one, with comma\nLine \"two\""));
    assert_eq!(by(&recs, "bank.example.mm").item.username.as_deref(), Some("မောင်မောင်"));
    assert!(by(&recs, "https://nouser.example.com/").item.username.is_none());
}

#[test]
fn malformed_files_error_cleanly() {
    assert!(import::parse_bytes(ImportFormat::BitwardenJson, &fixture("malformed.json"), None).is_err());
    assert!(import::parse_bytes(ImportFormat::GitpassJson, &fixture("malformed.json"), None).is_err());
    assert!(import::parse_bytes(ImportFormat::ChromeCsv, &fixture("malformed.csv"), None).is_err());
    assert!(import::parse_bytes(ImportFormat::OnePassword1pux, &fixture("malformed.json"), None).is_err());
    assert!(import::parse_bytes(ImportFormat::OnePassword1pux, &fixture("1password.csv"), None).is_err());
    // wrong format picked: a CSV fed to the JSON importer
    assert!(import::parse_bytes(ImportFormat::BitwardenJson, &fixture("chrome.csv"), None).is_err());
}

#[test]
fn gitpass_json_roundtrip_keeps_categories_tags_and_flags() {
    let (_, recs) = import_fixture(ImportFormat::OnePassword1pux, "1password.1pux");
    let json = import::export_records_json(&recs).unwrap();
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "fake-master-pw", fast()).unwrap();
    let s = Session::unlock(d.path(), "fake-master-pw", &sk).unwrap();
    let v = s.create_vault("rt").unwrap();
    let rep = import::import_items(&s, &v, import::parse(ImportFormat::GitpassJson, &json, None).unwrap(), ImportOpts::default()).unwrap();
    assert_eq!(rep.added, recs.len());
    let back: Vec<ItemRecord> = s.list_records(&v).unwrap().into_iter().map(|(_, _, r)| r).collect();
    for r in &recs {
        let b = by(&back, &r.item.title);
        assert_eq!((b.category, &b.tags, b.favorite, b.archived), (r.category, &r.tags, r.favorite, r.archived), "{}", r.item.title);
        assert_eq!((&b.item.username, &b.item.password, &b.item.url, &b.item.notes, &b.item.totp), (&r.item.username, &r.item.password, &r.item.url, &r.item.notes, &r.item.totp));
        assert_eq!(b.view_sections(), r.view_sections(), "{}", r.item.title);
    }
    // encrypted variant
    let enc = import::export_records_encrypted(&recs, "fake export passphrase", &fast()).unwrap();
    let p = import::parse(ImportFormat::GitpassJson, &enc, Some("fake export passphrase")).unwrap();
    assert_eq!(p.items.len(), recs.len());
}
