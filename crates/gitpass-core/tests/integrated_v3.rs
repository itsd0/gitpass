//! Cross-feature tests for the integrated 0.3.0 tree: devices + field merge + search/TOTP on one vault.
use gitpass_core::crypto::*;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_core::{fuzzy_find, merge_items, totp_now, ItemRecord};

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }

#[test]
fn fresh_v2_vault_gets_devices_clocks_and_search() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    // init writes format.json v2 / manifest v2; first unlock creates devices.enc and upgrades the manifest
    let s = Session::unlock_device(r, PW, &sk, &opts(), Some("laptop")).unwrap();
    assert_eq!(s.fmt.version, 2);
    assert!(r.join("devices.enc").exists());
    assert_eq!(s.load_manifest().unwrap().format, store::FORMAT_VERSION);
    let vid = s.create_vault("personal").unwrap();
    let mut it = Item::default();
    it.title = "GitHub".into(); it.username = Some("octo".into()); it.password = Some("a".into());
    it.totp = Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()); it.updated_at = store::now();
    let iid = s.add_item(&vid, &it).unwrap();
    let (_, rec) = s.read_record(&vid, &iid).unwrap();
    assert!(rec.clocks.is_some());
    // remote edits password; local merge picks it up
    let mut remote_item = rec.item.clone(); remote_item.password = Some("b".into());
    let mut c = rec.clocks.unwrap(); c.set("password", c.get("password") + 10);
    let remote = ItemRecord { item: remote_item, clocks: Some(c) };
    let (m, _) = merge_items(&rec, &remote);
    assert_eq!(m.item.password.as_deref(), Some("b"));
    s.merge_item(&vid, &iid, &remote).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("b"));
    let hits = fuzzy_find("gthb", s.list_items(&vid).unwrap());
    assert_eq!(hits[0].1, iid);
    let (code, _) = totp_now(&gitpass_core::totp::parse(it.totp.as_deref().unwrap()).unwrap());
    assert_eq!(code.len(), 6);
    // still unlocks via the legacy unlock_with entry point (default device name)
    drop(s);
    Session::unlock_with(r, PW, &sk, &opts()).unwrap();
}
