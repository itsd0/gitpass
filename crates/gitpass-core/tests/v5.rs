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
        Category::Passkey => vec![sec("main", vec![f("rp_id", Text, "example.invalid"), f("credential_id", Text, "AAAA"), f("private_key", Concealed, "FAKE")])],
        Category::BankAccount => vec![sec("main", vec![f("bank_name", Text, "Example Bank"), f("account_number", Concealed, "1234567890"), f("pin", Concealed, "1234")])],
        Category::Server => vec![sec("main", vec![f("url", Url, "https://srv.example.invalid"), f("username", Text, "root"), f("password", Concealed, "FAKE-srv-pass")])],
        _ => vec![],
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
fn restore_version_restores_snapshot_and_is_append_only() {
    let (_d, _sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::ApiCredential, "api v1", sample(Category::ApiCredential))).unwrap();
    // v2: change title & key
    let (_, mut r) = s.read_record(&v, &id).unwrap();
    r.item.title = "api v2".into();
    r.apply_sections(&[sec("api", vec![f("key", FieldKind::Concealed, "FAKE-key-v2")])]);
    s.update_record(&v, &id, r).unwrap();

    // v3: change title & key again
    let (_, mut r) = s.read_record(&v, &id).unwrap();
    r.item.title = "api v3".into();
    r.apply_sections(&[sec("api", vec![f("key", FieldKind::Concealed, "FAKE-key-v3")])]);
    s.update_record(&v, &id, r).unwrap();

    let (v3_ver, v3_rec) = s.read_record(&v, &id).unwrap();
    assert_eq!(v3_ver, 3);
    assert_eq!(v3_rec.history.len(), 2);

    // Restore v1
    let new_ver = s.restore_version(&v, &id, 1).unwrap();
    assert_eq!(new_ver, 4);

    let (v4_ver, v4_rec) = s.read_record(&v, &id).unwrap();
    assert_eq!(v4_ver, 4);
    assert_eq!(v4_rec.item.title, "api v1");
    assert_eq!(v4_rec.find_field("key").unwrap().value.as_deref(), Some("FAKE-KEY"));
    // Pre-restore state (v3) is retained in history, so history now has 3 entries: v1, v2, v3
    assert_eq!(v4_rec.history.len(), 3);
    assert_eq!(v4_rec.history[2].version, 3);
    assert_eq!(v4_rec.history[2].snapshot.item.title, "api v3");

    // Restore v3 (the pre-restore state, simulating Undo)
    let undo_ver = s.restore_version(&v, &id, 3).unwrap();
    assert_eq!(undo_ver, 5);
    let (v5_ver, v5_rec) = s.read_record(&v, &id).unwrap();
    assert_eq!(v5_ver, 5);
    assert_eq!(v5_rec.item.title, "api v3");
    assert_eq!(v5_rec.find_field("key").unwrap().value.as_deref(), Some("FAKE-key-v3"));
    assert_eq!(v5_rec.history.len(), 4);

    // Errors cleanly for unknown version
    let err_unknown = s.restore_version(&v, &id, 999).unwrap_err();
    assert!(err_unknown.to_string().contains("no version 999 in item history"));

    // Errors cleanly for deleted item
    s.soft_delete(&v, &id).unwrap();
    let err_deleted = s.restore_version(&v, &id, 1).unwrap_err();
    assert!(err_deleted.to_string().contains("cannot restore version of deleted item"));
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
    let (_, mut r_orig) = s.read_record(&v, &id).unwrap();
    let orig_created = r_orig.created_at;
    r_orig.tags = vec!["prod".into()];
    s.update_record(&v, &id, r_orig).unwrap();
    let (_, r_updated) = s.read_record(&v, &id).unwrap();
    assert_eq!(r_updated.history.len(), 2);

    let dup = s.copy_item_to(&v, &id, &v).unwrap();
    assert_eq!(s.list_records(&v).unwrap().len(), 2);
    let (_, r_dup) = s.read_record(&v, &dup).unwrap();
    assert!(r_dup.history.is_empty(), "duplicate item must start with empty history");

    let cp = s.copy_items_to(&[(v.clone(), id.clone())], &v2).unwrap();
    let (_, r_cp) = s.read_record(&v2, &cp[0]).unwrap();
    assert!(r_cp.history.is_empty(), "copied items must start with empty history");

    let nid = s.move_item(&v, &id, &v2).unwrap();
    assert!(s.read_record(&v, &id).is_err());
    let (_, r) = s.read_record(&v2, &nid).unwrap();
    assert_eq!(r.history.len(), 2, "moved item must retain history");
    assert_eq!(r.created_at, orig_created, "moved item must retain created_at");
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

#[test]
fn bulk_ops_use_one_commit_each() {
    let (_d, _sk, s, v) = vault();
    let v2 = s.create_vault("other").unwrap();
    let ids: Vec<String> = (0..5).map(|i| s.add_record(&v, rec(Category::Login, &format!("i{i}"), sample(Category::Login))).unwrap()).collect();
    let t: Vec<(String, String)> = ids.iter().map(|i| (v.clone(), i.clone())).collect();
    let e0 = s.epoch().unwrap();
    // duplicates are ignored
    let mut dup = t.clone(); dup.push(t[0].clone());
    assert_eq!(s.mutate_records(&dup, |r| r.favorite = true).unwrap(), 5);
    assert_eq!(s.epoch().unwrap(), e0 + 1);
    for i in &ids { let (ver, r) = s.read_record(&v, i).unwrap(); assert!(r.favorite); assert_eq!(ver, 2); }
    // no-op change writes nothing
    assert_eq!(s.mutate_records(&t, |r| r.favorite = true).unwrap(), 0);
    assert_eq!(s.epoch().unwrap(), e0 + 1);
    // tag + trash
    assert_eq!(s.mutate_records(&t[..3], |r| { r.tags.push("bulk".into()); r.deleted_at = Some(1); }).unwrap(), 3);
    assert_eq!(s.epoch().unwrap(), e0 + 2);
    assert_eq!(s.read_record(&v, &ids[0]).unwrap().1.tags, vec!["bulk".to_string()]);
    // a missing target aborts before writing anything
    let mut bad = t.clone(); bad.push((v.clone(), "nope".into()));
    assert!(s.mutate_records(&bad, |r| r.archived = true).is_err());
    assert!(!s.read_record(&v, &ids[0]).unwrap().1.archived);
    assert_eq!(s.epoch().unwrap(), e0 + 2);
    assert!(s.remove_items(&bad).is_err());
    assert!(s.read_record(&v, &ids[4]).is_ok());
    // copy: one commit for records
    let cp = s.copy_items_to(&t[3..], &v2).unwrap();
    assert_eq!(cp.len(), 2);
    assert_eq!(s.epoch().unwrap(), e0 + 3);
    let (ver, r) = s.read_record(&v2, &cp[0]).unwrap();
    assert_eq!((ver, r.item.title.as_str(), r.favorite), (1, "i3", true));
    assert!(r.history.is_empty());
    // move: copy commit + remove commit; items already in the target are skipped
    let mv = s.move_items(&[(v.clone(), ids[3].clone()), (v2.clone(), cp[1].clone())], &v2).unwrap();
    assert_eq!(mv.len(), 1);
    assert_eq!(s.epoch().unwrap(), e0 + 5);
    assert!(s.read_record(&v, &ids[3]).is_err());
    assert_eq!(s.read_record(&v2, &mv[0].2).unwrap().1.item.title, "i3");
    // purge three in one commit
    assert_eq!(s.remove_items(&t[..3]).unwrap(), 3);
    assert_eq!(s.epoch().unwrap(), e0 + 6);
    for i in &ids[..3] { assert!(s.read_record(&v, i).is_err()); }
    assert!(s.read_record(&v, &ids[4]).is_ok());
}

#[test]
fn bulk_move_and_purge_keep_attachments_right() {
    let (_d, _sk, s, v) = vault();
    let v2 = s.create_vault("other").unwrap();
    let a = s.add_record(&v, rec(Category::Document, "doc", sample(Category::Document))).unwrap();
    let at = s.add_attachment(&v, &a, "scan.txt", "text/plain", b"FAKE scan").unwrap();
    let b = s.add_record(&v, rec(Category::SecureNote, "note", sample(Category::SecureNote))).unwrap();
    let mv = s.move_items(&[(v.clone(), a.clone()), (v.clone(), b.clone())], &v2).unwrap();
    assert_eq!(mv.len(), 2);
    let (_, r) = s.read_record(&v2, &mv[0].2).unwrap();
    assert_eq!(s.read_attachment(&v2, &mv[0].2, &r.attachments[0].id).unwrap(), b"FAKE scan");
    assert!(s.read_attachment(&v, &a, &at.id).is_err());
    let t: Vec<(String, String)> = mv.iter().map(|m| (v2.clone(), m.2.clone())).collect();
    assert_eq!(s.remove_items(&t).unwrap(), 2);
    assert!(s.read_attachment(&v2, &mv[0].2, &r.attachments[0].id).is_err());
}

#[test]
fn remove_attachment_deletes_file_and_manifest_and_records_history() {
    let (d, sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::Login, "item-with-files", sample(Category::Login))).unwrap();

    let data_a = b"fake-file-a-contents";
    let data_b = b"fake-file-b-contents";
    let att_a = s.add_attachment(&v, &id, "a.txt", "text/plain", data_a).unwrap();
    let att_b = s.add_attachment(&v, &id, "b.pdf", "application/pdf", data_b).unwrap();

    let fp_a = d.path().join("vaults").join(&v).join("files").join(format!("{}.enc", att_a.id));
    let fp_b = d.path().join("vaults").join(&v).join("files").join(format!("{}.enc", att_b.id));
    assert!(fp_a.exists());
    assert!(fp_b.exists());
    assert_eq!(s.read_attachment(&v, &id, &att_a.id).unwrap(), data_a);
    assert_eq!(s.read_attachment(&v, &id, &att_b.id).unwrap(), data_b);

    let m1 = s.load_manifest().unwrap();
    assert!(m1.vaults[&v].files.contains_key(&att_a.id));
    assert!(m1.vaults[&v].files.contains_key(&att_b.id));

    let (v_before, r_before) = s.read_record(&v, &id).unwrap();
    assert_eq!(r_before.attachments.len(), 2);

    // Remove attachment A
    s.remove_attachment(&v, &id, &att_a.id).unwrap();

    // 1. Blob removed from disk
    assert!(!fp_a.exists(), "attachment file must be removed from disk");
    assert!(fp_b.exists(), "other attachment must remain");

    // 2. Manifest entry removed
    let m2 = s.load_manifest().unwrap();
    assert!(!m2.vaults[&v].files.contains_key(&att_a.id), "manifest must drop removed file");
    assert!(m2.vaults[&v].files.contains_key(&att_b.id));

    // 3. Decrypting removed attachment fails
    assert!(s.read_attachment(&v, &id, &att_a.id).is_err());
    assert_eq!(s.read_attachment(&v, &id, &att_b.id).unwrap(), data_b);

    // 4. Record updated and prior version recorded in history
    let (v_after, r_after) = s.read_record(&v, &id).unwrap();
    assert_eq!(v_after, v_before + 1);
    assert_eq!(r_after.attachments.len(), 1);
    assert_eq!(r_after.attachments[0].id, att_b.id);
    assert!(!r_after.history.is_empty(), "history must record prior edit");
    let last_hist = r_after.history.last().unwrap();
    assert_eq!(last_hist.version, v_before);

    // 5. Removing already-removed or non-existent attachment fails cleanly
    let err = s.remove_attachment(&v, &id, &att_a.id).unwrap_err();
    assert!(err.to_string().contains("item has no attachment"));

    // 6. Remove attachment B (now 0 attachments)
    s.remove_attachment(&v, &id, &att_b.id).unwrap();
    assert!(!fp_b.exists());
    let (_, r_empty) = s.read_record(&v, &id).unwrap();
    assert!(r_empty.attachments.is_empty());

    // 7. Adding file exceeding limit produces clear error
    let huge = vec![0u8; gitpass_core::item::MAX_ATTACHMENT_BYTES + 1];
    let err_size = s.add_attachment(&v, &id, "huge.dat", "application/octet-stream", &huge).unwrap_err();
    assert!(err_size.to_string().contains("exceeds 25 MiB limit"), "{err_size}");

    // 8. Re-unlocking session verifies clean integrity
    let s_unlocked = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    assert_eq!(s_unlocked.list_records(&v).unwrap().len(), 1);
}

#[test]
fn removed_attachment_stays_removed_after_concurrent_edit() {
    let (d, _sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::Login, "shared", sample(Category::Login))).unwrap();
    let keep = s.add_attachment(&v, &id, "keep.txt", "text/plain", b"FAKE keep").unwrap();
    let drop = s.add_attachment(&v, &id, "drop.txt", "text/plain", b"FAKE drop").unwrap();
    let (_, base) = s.read_record(&v, &id).unwrap();
    assert!(base.removed_attachments.is_empty());
    assert!(!serde_json::to_string(&base).unwrap().contains("removed_attachments"), "no tombstone key when empty");

    // Device B (remote): edits the item without knowing about the removal.
    let mut remote = base.clone();
    remote.tags = vec!["edited-on-b".into()];
    gitpass_core::merge::stamp_ext(Some(&base), &mut remote, base.xclocks.values().max().copied().unwrap_or(0) + 1000);
    assert_eq!(remote.attachments.len(), 2);

    // Device A (local): removes drop.txt — record, blob and manifest change in one commit.
    let epoch_before = s.load_manifest().unwrap().epoch;
    s.remove_attachment(&v, &id, &drop.id).unwrap();
    assert_eq!(s.load_manifest().unwrap().epoch, epoch_before + 1, "remove_attachment must be a single commit");
    let (_, local) = s.read_record(&v, &id).unwrap();
    assert_eq!(local.removed_attachments, vec![drop.id.clone()]);
    assert!(!d.path().join("vaults").join(&v).join("files").join(format!("{}.enc", drop.id)).exists());

    // A applies B's edit: the edit lands, the removed attachment does not come back.
    s.merge_item(&v, &id, &remote).unwrap();
    let (_, m) = s.read_record(&v, &id).unwrap();
    assert_eq!(m.tags, vec!["edited-on-b".to_string()]);
    assert_eq!(m.attachments.iter().map(|a| a.id.clone()).collect::<Vec<_>>(), vec![keep.id.clone()]);
    assert!(m.removed_attachments.contains(&drop.id));
    assert_eq!(s.read_attachment(&v, &id, &keep.id).unwrap(), b"FAKE keep");

    // B applies A's record: the tombstone removes drop.txt there too.
    let (out, rep) = merge_records(&remote, &local);
    assert!(rep.changed_local());
    assert_eq!(out.attachments.iter().map(|a| a.id.clone()).collect::<Vec<_>>(), vec![keep.id.clone()]);
    assert!(out.removed_attachments.contains(&drop.id));
    assert_eq!(out.tags, vec!["edited-on-b".to_string()]);

    // A later plain edit on A keeps the tombstone.
    let (_, cur) = s.read_record(&v, &id).unwrap();
    let mut it = cur.item.clone();
    it.title = "renamed".into();
    s.update_item(&v, &id, &it).unwrap();
    let (_, after) = s.read_record(&v, &id).unwrap();
    assert!(after.removed_attachments.contains(&drop.id));
    assert_eq!(after.attachments.len(), 1);
}

#[test]
fn restore_version_keeps_attachment_tombstones() {
    let (_d, _sk, s, v) = vault();
    let id = s.add_record(&v, rec(Category::Login, "restore-tombstone", sample(Category::Login))).unwrap();
    let att = s.add_attachment(&v, &id, "a.txt", "text/plain", b"fake-a").unwrap();
    let (_, stale) = s.read_record(&v, &id).unwrap();
    assert_eq!(stale.attachments.len(), 1);
    s.remove_attachment(&v, &id, &att.id).unwrap();
    s.restore_version(&v, &id, 1).unwrap();
    let (_, restored) = s.read_record(&v, &id).unwrap();
    assert!(restored.attachments.is_empty());
    assert!(restored.removed_attachments.contains(&att.id), "restore must keep the tombstone");
    let (m, _) = merge_records(&restored, &stale);
    assert!(!m.attachments.iter().any(|a| a.id == att.id), "merge must not resurrect a removed attachment");
    let (m2, _) = merge_records(&stale, &restored);
    assert!(!m2.attachments.iter().any(|a| a.id == att.id));
}


#[test]
fn bank_account_and_server_roundtrip_merge_and_compat() {
    let (_d, _sk, s, v) = vault();

    // 1. Bank Account creation, store round-trip, export/import
    let bank_rec = rec(Category::BankAccount, "Chase Checking", Category::BankAccount.field_template());
    let bank_sec = bank_rec.view_sections();
    assert_eq!(bank_sec[0].id, "main");
    assert!(bank_sec[0].fields.iter().any(|f| f.id == "account_number" && f.kind == FieldKind::Concealed));
    assert!(bank_sec[0].fields.iter().any(|f| f.id == "pin" && f.kind == FieldKind::Concealed));
    assert!(bank_sec[0].fields.iter().any(|f| f.id == "routing_number"));

    let bank_id = s.add_record(&v, bank_rec.clone()).unwrap();
    let (_, read_bank) = s.read_record(&v, &bank_id).unwrap();
    assert_eq!(read_bank.category, Category::BankAccount);
    assert_eq!(read_bank.item.title, "Chase Checking");

    // 2. Server creation, store round-trip, export/import
    let server_rec = rec(Category::Server, "Production DB", Category::Server.field_template());
    let srv_sec = server_rec.view_sections();
    assert_eq!(srv_sec[0].id, "main");
    assert!(srv_sec[0].fields.iter().any(|f| f.id == "password" && f.kind == FieldKind::Concealed));
    assert!(srv_sec[0].fields.iter().any(|f| f.id == "admin_console_url"));

    let server_id = s.add_record(&v, server_rec.clone()).unwrap();
    let (_, read_srv) = s.read_record(&v, &server_id).unwrap();
    assert_eq!(read_srv.category, Category::Server);
    assert_eq!(read_srv.item.title, "Production DB");

    // 3. gitpass-json export / import round-trip
    let exp = gitpass_core::import::export_records_json(&[read_bank.clone(), read_srv.clone()]).unwrap();
    let parsed = gitpass_core::import::parse_bytes(gitpass_core::import::ImportFormat::GitpassJson, exp.as_bytes(), None).unwrap();
    assert_eq!(parsed.items.len(), 2);
    assert_eq!(parsed.items[0].category, Category::BankAccount);
    assert_eq!(parsed.items[1].category, Category::Server);

    // 4. Backward compatibility: unknown future category deserializes as Category::Other and merges without data loss
    let future_json = r#"{
        "title": "Future Item",
        "updated_at": "1700000000",
        "category": "future_quantum_key",
        "sections": [{
            "id": "main",
            "title": null,
            "fields": [
                { "id": "qkey", "label": "quantum key", "kind": "concealed", "value": "SECRET-Q-999" }
            ]
        }],
        "tags": ["quantum"]
    }"#;
    let future_rec: ItemRecord = serde_json::from_str(future_json).unwrap();
    assert_eq!(future_rec.category, Category::Other);
    assert_eq!(future_rec.item.title, "Future Item");
    assert_eq!(future_rec.sections[0].fields[0].value.as_deref(), Some("SECRET-Q-999"));

    let mut local_rec = future_rec.clone();
    local_rec.item.title = "Future Item Renamed".into();
    let (merged, _) = merge_records(&local_rec, &future_rec);
    assert_eq!(merged.item.title, "Future Item Renamed");
    assert_eq!(merged.sections[0].fields[0].value.as_deref(), Some("SECRET-Q-999"));
    assert_eq!(merged.tags, vec!["quantum".to_string()]);
}

#[test]
fn rotation_reminder_survives_edit_restore_copy_merge_and_export() {
    use gitpass_core::merge::{merge_records, RotationStatus, ROTATION_DAYS};
    let (_d, _sk, s, v) = vault();
    let mut r = rec(Category::Login, "FAKE Bank", sample(Category::Login));
    r.created_at = Some(1_000_000);
    let id = s.add_record(&v, r).unwrap();
    // only the documented intervals are accepted
    assert!(s.set_rotation(&v, &id, Some(45)).is_err());
    assert_eq!(ROTATION_DAYS, [30, 90, 180, 365]);
    s.set_rotation(&v, &id, Some(90)).unwrap();
    assert_eq!(s.read_record(&v, &id).unwrap().1.rotate_every_days, Some(90));

    // a normal edit keeps it; so does restoring an older version
    let (_, mut cur) = s.read_record(&v, &id).unwrap();
    cur.item.notes = Some("edited".into());
    s.update_record(&v, &id, cur).unwrap();
    assert_eq!(s.read_record(&v, &id).unwrap().1.rotate_every_days, Some(90));
    let (_, cur) = s.read_record(&v, &id).unwrap();
    let ver = cur.history.last().unwrap().version;
    s.restore_version(&v, &id, ver).unwrap();
    assert_eq!(s.read_record(&v, &id).unwrap().1.rotate_every_days, Some(90));

    // copying to another vault keeps it
    let v2 = s.create_vault("other").unwrap();
    let copied = s.copy_items_to(&[(v.clone(), id.clone())], &v2).unwrap();
    assert_eq!(s.read_record(&v2, &copied[0]).unwrap().1.rotate_every_days, Some(90));

    // a newer clock on the remote side wins the merge, in both directions (set and clear)
    let (_, base) = s.read_record(&v, &id).unwrap();
    let mut remote = base.clone();
    remote.rotate_every_days = Some(30);
    gitpass_core::merge::stamp_ext(Some(&base), &mut remote, 9_000_000_000_000);
    let (merged, _) = merge_records(&base, &remote);
    assert_eq!(merged.rotate_every_days, Some(30));
    let mut cleared = remote.clone();
    cleared.rotate_every_days = None;
    gitpass_core::merge::stamp_ext(Some(&remote), &mut cleared, 9_000_000_000_001);
    let (merged, _) = merge_records(&remote, &cleared);
    assert_eq!(merged.rotate_every_days, None);

    // clearing through the session API
    s.set_rotation(&v, &id, None).unwrap();
    assert_eq!(s.read_record(&v, &id).unwrap().1.rotate_every_days, None);

    // gitpass-json export / import keeps it, and rotation_status reads it
    let mut e = rec(Category::Login, "FAKE Mail", sample(Category::Login));
    e.rotate_every_days = Some(180);
    e.created_at = Some(1_000_000);
    e.item.updated_at = "1000000".into();
    let json = gitpass_core::import::export_records_json(&[e]).unwrap();
    assert!(json.contains("\"rotate_every_days\": 180"), "{json}");
    let parsed = gitpass_core::import::parse_bytes(gitpass_core::import::ImportFormat::GitpassJson, json.as_bytes(), None).unwrap();
    let back = &parsed.items[0];
    assert_eq!(back.rotate_every_days, Some(180));
    assert!(matches!(back.rotation_status(1_000_000 + 200 * 86400), Some(RotationStatus::Due { every_days: 180, days: 200 })));
    assert!(matches!(back.rotation_status(1_000_000 + 170 * 86400), Some(RotationStatus::Soon { every_days: 180, in_days: 10 })));
    assert_eq!(back.rotation_status(1_000_000 + 100 * 86400), None);
}
