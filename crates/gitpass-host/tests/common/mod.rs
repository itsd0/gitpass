#![allow(dead_code)]
use gitpass_core::crypto::KdfParams;
use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_host::handler::{Host, HostConfig};
use serde_json::Value;
use std::time::Duration;

pub fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
pub const PW: &str = "correct horse battery";

pub struct V5 { pub dir: tempfile::TempDir, pub host: Host, pub vid: String, pub login: String, pub card: String, pub ident: String, pub note: String }

fn f(id: &str, kind: FieldKind, v: &str) -> Field { Field { id: id.into(), label: id.into(), kind, value: Some(v.into()), removed: false } }
fn rec(cat: Category, title: &str, fields: Vec<Field>) -> ItemRecord {
    let mut it = Item::default(); it.title = title.into();
    let mut r = ItemRecord::new(it);
    r.category = cat;
    r.apply_sections(&[Section { id: "main".into(), title: None, fields }]);
    r
}

/// Temp v5 vault (FAKE data) with a login, a card, an identity and a secure note.
pub fn v5_vault() -> V5 {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();
    let sk = store::init(&root, PW, fast()).unwrap();
    let skf = d.path().join("secret.key");
    std::fs::write(&skf, format!("{}\n", sk.encode())).unwrap();
    let s = Session::unlock_with(&root, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let li = Item { title: "GitHub".into(), username: Some("leeky".into()), password: Some("FAKE-gh-pw".into()),
        url: Some("https://github.com/login".into()), notes: Some("fake note".into()), totp: Some("JBSWY3DPEHPK3PXP".into()), updated_at: String::new() };
    let login = format!("{vid}:{}", s.add_record(&vid, ItemRecord::new(li)).unwrap());
    let card = format!("{vid}:{}", s.add_record(&vid, rec(Category::CreditCard, "Visa", vec![
        f("number", FieldKind::Concealed, "4111 1111 1111 1234"), f("expiry", FieldKind::MonthYear, "12/30"),
        f("cvv", FieldKind::Concealed, "123"), f("cardholder", FieldKind::Text, "Test Person")])).unwrap());
    let ident = format!("{vid}:{}", s.add_record(&vid, rec(Category::Identity, "Me", vec![
        f("name", FieldKind::Text, "Test Person"), f("email", FieldKind::Email, "t@example.invalid"), f("phone", FieldKind::Phone, "+1 555 0100")])).unwrap());
    let note = format!("{vid}:{}", s.add_record(&vid, rec(Category::SecureNote, "Wifi notes", vec![f("body", FieldKind::Multiline, "hello")])).unwrap());
    drop(s);
    let host = Host::new(HostConfig { vault_path: root, secret_key_path: skf, idle_timeout: Duration::from_secs(300), target_kdf: fast() });
    V5 { dir: d, host, vid, login, card, ident, note }
}

pub fn ok(h: &mut Host, v: Value) -> Value {
    let r = h.handle(&v);
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r["result"].clone()
}
pub fn err(h: &mut Host, v: Value) -> (String, String) {
    let r = h.handle(&v);
    assert_eq!(r["ok"], false, "{v} -> {r}");
    (r["error"].as_str().unwrap().into(), r["message"].as_str().unwrap_or("").into())
}
