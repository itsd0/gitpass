//! Vault rename / meta update / delete. Temp dirs only; all data FAKE.
use gitpass_core::crypto::*;
use gitpass_core::store::{self, Item, Session, UnlockOpts, VaultMeta};

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn item(t: &str) -> Item { let mut i = Item::default(); i.title = t.into(); i.password = Some("pw-FAKE".into()); i.updated_at = "1".into(); i }
fn meta(n: &str) -> VaultMeta { VaultMeta { name: n.into(), description: Some("d".into()), icon: Some("briefcase".into()), color: Some("#ff0000".into()) } }

#[test]
fn rename_and_update_meta_survives_relock() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
    let a = s.create_vault("A").unwrap(); let _b = s.create_vault("B").unwrap();
    let iid = s.add_item(&a, &item("x")).unwrap();
    s.update_vault_meta(&a, meta("  Work  ")).unwrap();
    assert!(s.update_vault_meta(&a, meta("B")).is_err(), "duplicate name rejected");
    assert!(s.update_vault_meta(&a, meta("  ")).is_err(), "empty name rejected");
    drop(s);
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
    let m = s.vault_meta(&a).unwrap();
    assert_eq!((m.name.as_str(), m.icon.as_deref(), m.color.as_deref()), ("Work", Some("briefcase"), Some("#ff0000")));
    assert_eq!(s.resolve_vault("Work").unwrap(), a);
    assert!(s.read_record(&a, &iid).is_ok());
}

#[test]
fn delete_vault_removes_files_and_blocks_last() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
    let a = s.create_vault("A").unwrap(); let b = s.create_vault("B").unwrap();
    s.add_item(&a, &item("x")).unwrap();
    s.delete_vault(&a).unwrap();
    assert!(!d.path().join("vaults").join(&a).exists());
    assert!(s.delete_vault(&a).is_err(), "already gone");
    assert!(s.delete_vault(&b).is_err(), "last vault can't be deleted");
    drop(s);
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap(); // manifest still verifies
    assert_eq!(s.list_vaults().unwrap().iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(), vec![b]);
}
