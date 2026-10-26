//! Format v5: typed items, back-compat with a real v0.4 vault fixture, soft delete, history,
//! attachments and section field merge. All data is FAKE.
use gitpass_core::crypto::{KdfParams, SecretKey};
use gitpass_core::item::{Category, Field, FieldKind, Section, HISTORY_LIMIT, PURGE_AFTER_SECS};
use gitpass_core::merge::{merge_records, ItemRecord};
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use std::fs;
use std::path::Path;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }

fn vault() -> (tempfile::TempDir, SecretKey, Session, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "fake-pw", fast()).unwrap();
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    let v = s.create_vault("v5").unwrap();
    (d, sk, s, v)
}
fn f(id: &str, kind: FieldKind, v: &str) -> Field { Field { id: id.into(), label: id.into(), kind, value: Some(v.into()), removed: false } }
fn sec(id: &str, fields: Vec<Field>) -> Section { Section { id: id.into(), title: Some(id.into()), fields } }
fn rec(cat: Category, title: &str, sections: Vec<Section>) -> ItemRecord {
    let mut it = Item::default(); it.title = title.into();
    let mut r = ItemRecord::new(it);
    r.category = cat;
    r.apply_sections(&sections);
    r
}
fn sample(cat: Category) -> Vec<Section> {
    use FieldKind::*;
    match cat {
        Category::Login => vec![sec("login", vec![f("username", Text, "alice"), f("password", Concealed, "FAKE-pw")]), sec("extra", vec![f("pin", Concealed, "0000")])],
        Category::CreditCard => vec![sec("card", vec![f("number", Concealed, "4111111111111111"), f("expiry", MonthYear, "12/30"), f("cvv", Concealed, "123"), f("holder", Text, "Test Person")])],
        Category::SecureNote => vec![sec("note", vec![f("body", Multiline, "line1\nline2")])],
        Category::Identity => vec![sec("id", vec![f("name", Text, "Test Person"), f("email", Email, "t@example.invalid"), f("phone", Phone, "+1 555 0100"), f("dob", Date, "2000-01-01")])],
        Category::ApiCredential => vec![sec("api", vec![f("key", Concealed, "FAKE-KEY"), f("host", Url, "https://api.example.invalid")])],
        Category::Document => vec![sec("doc", vec![f("desc", Text, "scan")])],
        Category::SshKey => vec![sec("ssh", vec![f("private", Concealed, "-----FAKE KEY-----"), f("public", Text, "ssh-ed25519 AAAA fake")])],
        Category::Wifi => vec![sec("wifi", vec![f("ssid", Text, "FakeNet"), f("wifi_password", Concealed, "FAKE-wifi")])],
    }
}

#[test]
fn every_category_round_trips() {
    let (d, sk, s, v) = vault();
    let mut ids = vec![];
    for c in Category::ALL {
        let mut r = rec(c, c.as_str(), sample(c));
        r.tags = vec!["t1".into(), c.as_str().into()];
        r.favorite = c == Category::Wifi;
        ids.push((c, s.add_record(&v, r).unwrap()));
    }
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    for (c, id) in ids {
        let (_, r) = s.read_record(&v, &id).unwrap();
        assert_eq!(r.category, c);
        assert_eq!(r.tags, vec!["t1".to_string(), c.as_str().to_string()]);
        assert_eq!(r.favorite, c == Category::Wifi);
        assert!(r.created_at.is_some());
        let want: Vec<(String, Option<String>)> = sample(c).iter().flat_map(|s| s.fields.iter().map(|f| (f.id.clone(), f.value.clone()))).collect();
        let got: Vec<(String, Option<String>)> = r.view_sections().iter().flat_map(|s| s.fields.iter().map(|f| (f.id.clone(), f.value.clone()))).collect();
        assert_eq!(got, want, "{c:?}");
        if c == Category::Login {
            assert_eq!(r.item.username.as_deref(), Some("alice"));
            assert_eq!(r.item.password.as_deref(), Some("FAKE-pw"));
            // login fields stay in the legacy slots -> old readers / CLI still see them
            assert!(r.sections.iter().all(|s| s.fields.iter().all(|f| f.id != "password")));
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let t = dst.join(e.file_name());
        if e.path().is_dir() { copy_dir(&e.path(), &t) } else { fs::copy(e.path(), t).unwrap(); }
    }
}

/// Fixture produced by gitpass v0.4.0 (format.json v4 after a password change, manifest v4 with identity).
#[test]
fn reads_real_v4_vault_and_upgrades_on_write() {
    let d = tempfile::tempdir().unwrap();
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v4_vault"), d.path());
    let r = d.path();
    let sk = SecretKey::parse(fs::read_to_string(r.join("secret.key.txt")).unwrap().trim()).unwrap();
    assert_eq!(store::read_format(r).unwrap().version, 4);
    let s = Session::unlock_device(r, "fixture-pw", &sk, &opts(), Some("fixture-dev")).unwrap();
    assert!(s.accepted_problems.is_empty());
    let (vid, meta) = s.list_vaults().unwrap().remove(0);
    assert_eq!(meta.name, "legacy");
    assert_eq!(meta.icon, None);
    let items = s.list_records(&vid).unwrap();
    assert_eq!(items.len(), 1);
    let (iid, _, rec) = &items[0];
    assert_eq!(rec.category, Category::Login);
    assert_eq!(rec.item.password.as_deref(), Some("FAKE-pw-2"));
    assert!(rec.history.is_empty() && rec.created_at.is_none());
    let view = rec.view_sections();
    assert_eq!(view[0].id, "login");
    assert_eq!(view[0].fields.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["username", "password", "totp"]);
    // write with v5 features; format.json is NOT bumped (keyset not re-wrapped)
    s.set_favorite(&vid, iid, true).unwrap();
    let mut it = rec.item.clone(); it.password = Some("FAKE-pw-3".into());
    s.update_item(&vid, iid, &it).unwrap();
    assert_eq!(store::read_format(r).unwrap().version, 4);
    let s = Session::unlock_device(r, "fixture-pw", &sk, &opts(), None).unwrap();
    let (_, r2) = s.read_record(&vid, iid).unwrap();
    assert!(r2.favorite);
    assert_eq!(r2.history.last().unwrap().snapshot.item.password.as_deref(), Some("FAKE-pw-2"));
}

#[test]
fn soft_delete_restore_purge() {
    let (d, sk, s, v) = vault();
    let a = s.add_record(&v, rec(Category::SecureNote, "a", sample(Category::SecureNote))).unwrap();
    let b = s.add_record(&v, rec(Category::SecureNote, "b", sample(Category::SecureNote))).unwrap();
    s.soft_delete(&v, &a).unwrap();
    assert!(s.read_record(&v, &a).unwrap().1.deleted_at.is_some());
    s.restore(&v, &a).unwrap();
    assert!(s.read_record(&v, &a).unwrap().1.deleted_at.is_none());
    s.soft_delete(&v, &a).unwrap();
    s.soft_delete(&v, &b).unwrap();
    let del = s.read_record(&v, &a).unwrap().1.deleted_at.unwrap();
    // not yet expired
    assert!(s.purge_expired(del + PURGE_AFTER_SECS - 1).unwrap().is_empty());
    assert_eq!(s.purge_expired(del + PURGE_AFTER_SECS + 10).unwrap().len(), 2);
    assert!(s.read_record(&v, &a).is_err());
    // explicit purge
    let c = s.add_record(&v, rec(Category::Wifi, "c", sample(Category::Wifi))).unwrap();
    s.purge_item(&v, &c).unwrap();
    assert!(s.list_records(&v).unwrap().is_empty());
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    assert!(s.accepted_problems.is_empty());
}

#[test]
fn purge_on_unlock_after_30_days() {
    let (d, sk, s, v) = vault();
    let a = s.add_record(&v, rec(Category::SecureNote, "old", sample(Category::SecureNote))).unwrap();
    let mut r = s.read_record(&v, &a).unwrap().1;
    r.deleted_at = Some(store::now_secs() - PURGE_AFTER_SECS - 5);
    s.update_record(&v, &a, r).unwrap();
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    assert!(s.list_records(&v).unwrap().is_empty());
}

#[test]
fn history_keeps_last_n_versions() {
    let (_d, _sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::ApiCredential, "api", sample(Category::ApiCredential))).unwrap();
    for i in 0..(HISTORY_LIMIT + 5) {
        let (_, mut r) = s.read_record(&v, &id).unwrap();
        r.apply_sections(&[sec("api", vec![f("key", FieldKind::Concealed, &format!("FAKE-{i}"))])]);
        s.update_record(&v, &id, r).unwrap();
    }
    let (ver, r) = s.read_record(&v, &id).unwrap();
    assert_eq!(r.history.len(), HISTORY_LIMIT);
    // meta-only change does not add history
    s.set_favorite(&v, &id, true).unwrap();
    assert_eq!(s.read_record(&v, &id).unwrap().1.history.len(), HISTORY_LIMIT);
    let prev = s.item_version(&v, &id, ver - 1).unwrap();
    assert_eq!(prev.find_field("key").unwrap().value.as_deref(), Some(format!("FAKE-{}", HISTORY_LIMIT + 3).as_str()));
    assert!(s.item_version(&v, &id, 1).is_err(), "oldest versions dropped");
}

#[test]
fn attachments_round_trip_and_tamper_detected() {
    let (d, sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::Document, "doc", sample(Category::Document))).unwrap();
    let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let a = s.add_attachment(&v, &id, "scan.pdf", "application/pdf", &data).unwrap();
    assert_eq!(a.size, 5000);
    assert_eq!(s.read_attachment(&v, &id, &a.id).unwrap(), data);
    assert_eq!(s.load_manifest().unwrap().format, store::MANIFEST_FORMAT_V5);
    let fp = d.path().join("vaults").join(&v).join("files").join(format!("{}.enc", a.id));
    let raw = fs::read(&fp).unwrap();
    assert!(raw.len() > 8192, "padded to bucket");
    // too big
    assert!(s.add_attachment(&v, &id, "big", "x", &vec![0u8; gitpass_core::item::MAX_ATTACHMENT_BYTES + 1]).is_err());
    // survives vault key rotation
    s.rotate_vault(&v).unwrap();
    assert_eq!(s.read_attachment(&v, &id, &a.id).unwrap(), data);
    // tamper: flip ciphertext
    let mut blob: store::FileBlob = serde_json::from_slice(&fs::read(&fp).unwrap()).unwrap();
    let mut ct = blob.envelope.ct.into_bytes();
    ct[10] = if ct[10] == b'A' { b'B' } else { b'A' };
    blob.envelope.ct = String::from_utf8(ct).unwrap();
    fs::write(&fp, serde_json::to_vec_pretty(&blob).unwrap()).unwrap();
    assert!(s.read_attachment(&v, &id, &a.id).is_err());
    let e = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap_err();
    assert!(e.to_string().contains("files"), "{e}");
}

#[test]
fn attachment_moved_to_other_item_rejected() {
    let (d, _sk, s, v) = vault();
    let a1 = s.add_record(&v, rec(Category::Document, "a", vec![])).unwrap();
    let a2 = s.add_record(&v, rec(Category::Document, "b", vec![])).unwrap();
    let x = s.add_attachment(&v, &a1, "x", "text/plain", b"secret-a").unwrap();
    // claim the file in a2's record: AAD binds item id -> fails
    let (_, mut r) = s.read_record(&v, &a2).unwrap();
    r.attachments.push(x.clone());
    s.update_record(&v, &a2, r).unwrap();
    assert!(s.read_attachment(&v, &a2, &x.id).is_err());
    let _ = d;
}

#[test]
fn move_and_duplicate_with_attachments() {
    let (_d, _sk, s, v) = vault();
    let v2 = s.create_vault_full("other", None, Some("briefcase".into()), Some("#ff0000".into())).unwrap();
    assert_eq!(s.vault_meta(&v2).unwrap().color.as_deref(), Some("#ff0000"));
    let id = s.add_record(&v, rec(Category::SshKey, "ssh", sample(Category::SshKey))).unwrap();
    let at = s.add_attachment(&v, &id, "id.pub", "text/plain", b"ssh-ed25519 fake").unwrap();
    let dup = s.copy_item_to(&v, &id, &v).unwrap();
    assert_eq!(s.list_records(&v).unwrap().len(), 2);
    let nid = s.move_item(&v, &id, &v2).unwrap();
    assert!(s.read_record(&v, &id).is_err());
    let (_, r) = s.read_record(&v2, &nid).unwrap();
    assert_eq!(r.find_field("private").unwrap().value.as_deref(), Some("-----FAKE KEY-----"));
    assert_eq!(s.read_attachment(&v2, &nid, &r.attachments[0].id).unwrap(), b"ssh-ed25519 fake");
    assert_ne!(r.attachments[0].id, at.id);
    assert!(s.read_record(&v, &dup).is_ok());
}

#[test]
fn field_merge_on_sections() {
    let (_d, _sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::CreditCard, "card", sample(Category::CreditCard))).unwrap();
    let (_, base) = s.read_record(&v, &id).unwrap();
    // remote edits cvv + adds a field + tags; local edits holder + deletes expiry
    let mut remote = base.clone();
    let mut secs = remote.view_sections();
    secs[0].fields.iter_mut().find(|f| f.id == "cvv").unwrap().value = Some("999".into());
    secs[0].fields.push(f("pin", FieldKind::Concealed, "4321"));
    remote.apply_sections(&secs);
    remote.tags = vec!["remote".into()];
    gitpass_core::merge::stamp_ext(Some(&base), &mut remote, base.xclocks.values().max().unwrap() + 1000);
    let mut local = base.clone();
    let mut ls = local.view_sections();
    ls[0].fields.iter_mut().find(|f| f.id == "holder").unwrap().value = Some("Local Name".into());
    ls[0].fields.retain(|f| f.id != "expiry");
    local.apply_sections(&ls);
    s.update_record(&v, &id, local).unwrap();
    let rep = s.merge_item(&v, &id, &remote).unwrap();
    assert!(!rep.has_conflicts(), "{rep:?}");
    assert!(rep.ext_from_remote.contains(&"f:cvv".to_string()));
    let (_, m) = s.read_record(&v, &id).unwrap();
    let get = |k: &str| m.find_field(k).and_then(|f| f.value);
    assert_eq!(get("cvv").as_deref(), Some("999"));
    assert_eq!(get("pin").as_deref(), Some("4321"));
    assert_eq!(get("holder").as_deref(), Some("Local Name"));
    assert!(m.find_field("expiry").is_none(), "local delete kept (higher clock)");
    assert_eq!(m.tags, vec!["remote".to_string()]);

    // equal clocks + different value -> conflict, local kept
    let mut a = base.clone();
    let mut b = base.clone();
    let mut sa = a.view_sections(); sa[0].fields.iter_mut().find(|f| f.id == "holder").unwrap().value = Some("A".into()); a.apply_sections(&sa);
    let mut sb = b.view_sections(); sb[0].fields.iter_mut().find(|f| f.id == "holder").unwrap().value = Some("B".into()); b.apply_sections(&sb);
    gitpass_core::merge::stamp_ext(Some(&base), &mut a, 9_000_000_000_000);
    gitpass_core::merge::stamp_ext(Some(&base), &mut b, 9_000_000_000_000);
    let (mr, rep) = merge_records(&a, &b);
    assert_eq!(rep.ext_conflicts, vec!["f:holder".to_string()]);
    assert_eq!(mr.find_field("holder").unwrap().value.as_deref(), Some("A"));
}

#[test]
fn legacy_item_without_v5_fields_merges_with_v5_record() {
    // v4 remote (no xclocks) vs v5 local: legacy fields still merge, v5 data preserved
    let mut base = Item::default(); base.title = "x".into(); base.password = Some("p".into()); base.updated_at = "100".into();
    let mut local = ItemRecord::with_clocks(base.clone(), Some(gitpass_core::merge::FieldClocks::all(100_000)));
    local.tags = vec!["keep".into()];
    gitpass_core::merge::stamp_ext(None, &mut local, 100_000);
    let mut rc = gitpass_core::merge::FieldClocks::all(100_000); rc.password = 200_000;
    let remote = { let mut it = base.clone(); it.password = Some("p2".into()); ItemRecord::with_clocks(it, Some(rc)) };
    let (m, rep) = merge_records(&local, &remote);
    assert_eq!(rep.from_remote, vec!["password"]);
    assert_eq!(m.item.password.as_deref(), Some("p2"));
    assert_eq!(m.tags, vec!["keep".to_string()]);
}
