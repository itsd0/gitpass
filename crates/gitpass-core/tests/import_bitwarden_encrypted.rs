//! Password-protected Bitwarden JSON imports. Fixtures are FAKE and were produced by an independent
//! implementation of Bitwarden's export format (KDF -> HKDF-Expand("enc"/"mac") -> AES-256-CBC +
//! HMAC-SHA256), so this also checks compatibility of the derivation details.
use gitpass_core::import::{self, ImportFormat};
use serde_json::{json, Value};
use std::path::PathBuf;

const PASSWORD: &str = "fake-export-password";
const PBKDF2: &str = "bitwarden-password-protected.json";
const ARGON2ID: &str = "bitwarden-password-protected-argon2id.json";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import").join(name)).unwrap()
}

fn parse(name: &str, password: Option<&str>) -> anyhow::Result<import::Parsed> {
    import::parse(ImportFormat::BitwardenJson, &fixture(name), password)
}

fn parse_json(doc: &Value, password: Option<&str>) -> anyhow::Result<import::Parsed> {
    import::parse(ImportFormat::BitwardenJson, &doc.to_string(), password)
}

fn error_of(result: anyhow::Result<import::Parsed>) -> String {
    match result { Ok(_) => panic!("expected an error"), Err(e) => e.to_string() }
}

fn check_items(p: &import::Parsed) {
    assert_eq!(p.items.len(), 2);
    let m = p.items.iter().find(|r| r.item.title == "Example Mail").expect("login");
    assert_eq!(m.category, gitpass_core::item::Category::Login);
    assert_eq!(m.item.username.as_deref(), Some("alice@example.com"));
    assert_eq!(m.item.password.as_deref(), Some("FAKE-BW-Pass-1"));
    assert_eq!(m.item.url.as_deref(), Some("https://mail.example.com/login"));
    assert!(m.item.notes.as_deref().unwrap().starts_with("Fake notes line one\nFake notes line two"));
    assert_eq!(m.tags, vec!["Fake Folder"]);
    assert!(m.favorite);
    gitpass_core::totp::parse(m.item.totp.as_deref().unwrap()).unwrap();
    assert!(p.items.iter().any(|r| r.item.title == "Fake Secure Note"));
}

#[test]
fn decrypts_pbkdf2_password_protected_export() {
    check_items(&parse(PBKDF2, Some(PASSWORD)).unwrap());
}

#[test]
fn decrypts_argon2id_password_protected_export() {
    check_items(&parse(ARGON2ID, Some(PASSWORD)).unwrap());
}

#[test]
fn parse_bytes_accepts_the_export_password() {
    let data = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import").join(ARGON2ID)).unwrap();
    let p = import::parse_bytes(ImportFormat::BitwardenJson, &data, Some(PASSWORD)).unwrap();
    check_items(&p);
}

#[test]
fn detects_password_protected_exports_and_reports_when_a_password_is_needed() {
    for name in [PBKDF2, ARGON2ID] {
        let raw = fixture(name);
        assert!(import::bitwarden_json_needs_password(&raw), "{name}");
        assert_eq!(import::detect_format(&raw), Some(ImportFormat::BitwardenJson), "{name}");
        assert_eq!(import::detect_format_bytes(raw.as_bytes()), Some(ImportFormat::BitwardenJson), "{name}");
    }
    // An unencrypted Bitwarden export needs no password.
    let plain = fixture("bitwarden.json");
    assert!(!import::bitwarden_json_needs_password(&plain));
    assert_eq!(import::detect_format(&plain), Some(ImportFormat::BitwardenJson));
}

#[test]
fn missing_export_password_is_a_distinct_error() {
    for password in [None, Some("")] {
        let e = error_of(parse(PBKDF2, password));
        assert!(e.contains("export password is required"), "{e}");
    }
}

#[test]
fn wrong_password_is_a_distinct_error() {
    let e = error_of(parse(PBKDF2, Some("not-the-export-password")));
    assert!(e.contains("wrong export password"), "{e}");
    assert!(!e.contains(PASSWORD), "the password must never appear in errors: {e}");
}

#[test]
fn tampered_payload_is_a_distinct_error() {
    let mut v: Value = serde_json::from_str(&fixture(PBKDF2)).unwrap();
    let data = v["data"].as_str().unwrap();
    let (head, mac) = data.rsplit_once('|').unwrap();
    // Flip one MAC byte (still valid base64) so the payload MAC check fails but the password check passes.
    let mut mac = mac.as_bytes().to_vec();
    mac[0] = if mac[0] == b'A' { b'B' } else { b'A' };
    let flipped = String::from_utf8(mac).unwrap();
    v["data"] = json!(format!("{head}|{flipped}"));
    let e = error_of(parse_json(&v, Some(PASSWORD)));
    assert!(e.contains("integrity check"), "{e}");
}

#[test]
fn account_encrypted_export_is_a_distinct_error() {
    let doc = json!({
        "encrypted": true,
        "passwordProtected": false,
        "salt": "ZmFrZQ==",
        "kdfType": 0,
        "kdfIterations": 5000,
        "encKeyValidation_DO_NOT_EDIT": "2.AAAA|BBBB|CCCC",
        "data": "2.AAAA|BBBB|CCCC"
    });
    assert!(!import::bitwarden_json_needs_password(&doc.to_string()));
    let e = error_of(parse_json(&doc, Some(PASSWORD)));
    assert!(e.contains("account key"), "{e}");
}

#[test]
fn rejects_out_of_range_kdf_settings() {
    let tamper = |k: &str, value: Value| {
        let mut v: Value = serde_json::from_str(&fixture(PBKDF2)).unwrap();
        v[k] = value;
        error_of(parse_json(&v, Some(PASSWORD)))
    };
    let e = tamper("kdfIterations", json!(1));
    assert!(e.contains("unsupported PBKDF2 iteration count"), "{e}");
    let e = tamper("kdfIterations", json!(2_000_001));
    assert!(e.contains("unsupported PBKDF2 iteration count"), "{e}");

    let mut v: Value = serde_json::from_str(&fixture(ARGON2ID)).unwrap();
    for (key, value) in [("kdfIterations", json!(11)), ("kdfMemory", json!(4096)), ("kdfParallelism", json!(0))] {
        let mut doc = v.clone();
        doc[key] = value;
        let e = error_of(parse_json(&doc, Some(PASSWORD)));
        assert!(e.contains("unsupported Argon2id settings"), "{key}: {e}");
    }
    v["kdfType"] = json!(7);
    let e = error_of(parse_json(&v, Some(PASSWORD)));
    assert!(e.contains("unsupported KDF type 7"), "{e}");
}

#[test]
fn rejects_missing_and_malformed_fields() {
    let tamper = |k: &str, value: Value| {
        let mut v: Value = serde_json::from_str(&fixture(PBKDF2)).unwrap();
        v[k] = value;
        error_of(parse_json(&v, Some(PASSWORD)))
    };
    assert!(tamper("salt", json!("")).contains("\"salt\""));
    let e = tamper("data", json!("3.not-a-type-2-enc-string"));
    assert!(e.contains("integrity check"), "{e}");
    let e = tamper("data", json!("2.bm90LWJhc2U2NA==|!!!|$$$"));
    assert!(e.contains("integrity check"), "{e}");
}
