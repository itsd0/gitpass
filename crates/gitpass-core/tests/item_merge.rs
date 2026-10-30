//! Session-level duplicate merge (issue #396). All data is FAKE.
use gitpass_core::crypto::KdfParams;
use gitpass_core::item::{AttachmentMeta, Category};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session, UnlockOpts};

fn fast() -> KdfParams {
    KdfParams {
        alg: "argon2id".into(),
        m_kib: 8192,
        t: 1,
        p: 1,
    }
}
fn opts() -> UnlockOpts {
    UnlockOpts {
        accept_rollback: false,
        target_kdf: fast(),
    }
}

fn vault() -> (
    tempfile::TempDir,
    gitpass_core::crypto::SecretKey,
    Session,
    String,
) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "fake-pw", fast()).unwrap();
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &opts()).unwrap();
    let v = s.create_vault("v").unwrap();
    (d, sk, s, v)
}

fn login(title: &str, password: &str, url: &str) -> ItemRecord {
    let mut it = Item::default();
    it.title = title.into();
    it.username = Some("alice".into());
    it.password = Some(password.into());
    it.url = Some(url.into());
    it.updated_at = "1700000000".into();
    let mut r = ItemRecord::new(it);
    r.category = Category::Login;
    r.created_at = Some(1700000000);
    r
}

#[test]
fn merge_combines_and_trashes_other_in_one_commit() {
    let (_d, _sk, s, v) = vault();
    let mut keep = login("Example", "FAKE-keep", "https://a.example.invalid");
    keep.tags = vec!["work".into()];
    let keep_id = s.add_record(&v, keep).unwrap();
    let mut other = login("Example", "FAKE-other", "https://b.example.invalid");
    other.tags = vec!["personal".into()];
    other.item.totp = Some("FAKE-totp".into());
    other.item.notes = Some("other note".into());
    let other_id = s.add_record(&v, other).unwrap();

    let epoch_before = s.epoch().unwrap();
    let out = s.merge_duplicate_items(&v, &keep_id, &other_id).unwrap();
    assert_eq!(out.keep_version_before + 1, out.keep_version);
    // Both records written in one manifest commit.
    assert_eq!(s.epoch().unwrap(), epoch_before + 1);

    let (_, merged) = s.read_record(&v, &keep_id).unwrap();
    assert_eq!(merged.item.password.as_deref(), Some("FAKE-keep"));
    assert!(merged
        .password_history
        .iter()
        .any(|e| e.value == "FAKE-other"));
    assert_eq!(merged.tags, vec!["work", "personal"]);
    assert_eq!(merged.item.totp.as_deref(), Some("FAKE-totp"));
    assert_eq!(merged.item.notes.as_deref(), Some("other note"));
    assert!(merged
        .sections
        .iter()
        .flat_map(|s| &s.fields)
        .any(|f| f.label == "website (merged)"));

    let (_, trashed) = s.read_record(&v, &other_id).unwrap();
    assert!(trashed.deleted_at.is_some());

    // Undo path: restore the discarded item, then restore the kept item
    // to its pre-merge version.
    s.restore(&v, &other_id).unwrap();
    let (_, restored) = s.read_record(&v, &other_id).unwrap();
    assert!(restored.deleted_at.is_none());
    let new_v = s
        .restore_version(&v, &keep_id, out.keep_version_before)
        .unwrap();
    assert_eq!(new_v, out.keep_version + 1);
    let (_, undone) = s.read_record(&v, &keep_id).unwrap();
    assert_eq!(undone.item.password.as_deref(), Some("FAKE-keep"));
    assert!(undone.tags == vec!["work"]);
}

#[test]
fn merge_copies_attachments_and_rejects_bad_inputs() {
    let (_d, _sk, s, v) = vault();
    let keep_id = s.add_record(&v, login("Example", "FAKE-pw", "")).unwrap();
    let other_id = s.add_record(&v, login("Example", "FAKE-pw", "")).unwrap();
    let meta: AttachmentMeta = s
        .add_attachment(&v, &other_id, "scan.txt", "text/plain", b"FAKE-attachment")
        .unwrap();

    let out = s.merge_duplicate_items(&v, &keep_id, &other_id).unwrap();
    assert_eq!(out.report.attachments_added, 1);
    let (_, merged) = s.read_record(&v, &keep_id).unwrap();
    assert_eq!(merged.attachments.len(), 1);
    assert_eq!(merged.attachments[0].name, "scan.txt");
    assert_ne!(merged.attachments[0].id, meta.id);
    let bytes = s
        .read_attachment(&v, &keep_id, &merged.attachments[0].id)
        .unwrap();
    assert_eq!(bytes, b"FAKE-attachment");

    // The discarded item's own blob still works after restore.
    s.restore(&v, &other_id).unwrap();
    assert_eq!(
        s.read_attachment(&v, &other_id, &meta.id).unwrap(),
        b"FAKE-attachment"
    );

    // Bad inputs.
    assert!(s.merge_duplicate_items(&v, &keep_id, &keep_id).is_err());
    let mut note = Item::default();
    note.title = "Note".into();
    let mut note_rec = ItemRecord::new(note);
    note_rec.category = Category::SecureNote;
    let note_id = s.add_record(&v, note_rec).unwrap();
    assert!(s.merge_duplicate_items(&v, &keep_id, &note_id).is_err());
    assert!(s.merge_duplicate_items(&v, &keep_id, "no-such-id").is_err());
}
