//! Device registry (format v3) tests.
use gitpass_core::crypto::*;
use gitpass_core::device::{self, DeviceRegistry};
use gitpass_core::store::{self, Item, Manifest, Session, UnlockOpts};
use std::fs;
use std::path::Path;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn unlock(root: &Path, sk: &SecretKey, name: &str) -> anyhow::Result<Session> {
    Session::unlock_device(root, PW, sk, &opts(), Some(name))
}
fn item(t: &str) -> Item { let mut i = Item::default(); i.title = t.into(); i.password = Some("pw".into()); i.updated_at = "1".into(); i }
fn manifest(s: &Session) -> Manifest { s.load_manifest().unwrap() }
/// Simulate a second machine: same synced vault files, different local device id.
fn switch_device(root: &Path) { fs::remove_file(root.join(device::LOCAL_DEVICE_FILE)).unwrap(); }
fn set_device(root: &Path, id: &str) {
    fs::write(root.join(device::LOCAL_DEVICE_FILE), format!("{{\"id\":\"{id}\"}}")).unwrap();
}

#[test]
fn seal_open_roundtrip_and_aad_binding() {
    let ak = Key32::random();
    let mut r = DeviceRegistry::default();
    r.touch("11111111-1111-4111-8111-111111111111", "laptop", "2026-10-26T00:00:00Z");
    let env = device::seal_registry(&ak, "acct", &r).unwrap();
    assert_eq!(device::open_registry(&ak, "acct", &env).unwrap(), r);
    assert!(device::open_registry(&ak, "other-acct", &env).is_err(), "AAD must bind account id");
    assert!(device::open_registry(&Key32::random(), "acct", &env).is_err());
    // key is the documented HKDF derivation
    assert_eq!(device::devices_key(&ak).0, hkdf32(&ak.0, None, b"gitpass/devices/v3").0);
    assert_eq!(device::devices_aad("acct"), b"acct|devices|3".to_vec());
}

#[test]
fn iso8601_formatting() {
    assert_eq!(device::iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(device::iso8601(951782400), "2000-02-29T00:00:00Z");
    assert_eq!(device::iso8601(1793000000), "2026-10-26T07:33:20Z");
}

#[test]
fn unlock_registers_device_and_no_plaintext_on_disk() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    assert!(!r.join("devices.enc").exists());
    let s = unlock(r, &sk, "MyLaptopXYZ").unwrap();
    let m = manifest(&s);
    assert_eq!(m.format, store::MANIFEST_FORMAT_V3);
    assert_eq!(m.devices_hash.as_deref(), Some(store::hash_file(&r.join("devices.enc")).unwrap().as_str()));
    let reg = s.devices().unwrap();
    assert_eq!(reg.format, 3);
    assert_eq!(reg.devices.len(), 1);
    let dv = &reg.devices[0];
    assert_eq!((dv.id.as_str(), dv.name.as_str(), dv.revoked), (s.device_id.as_str(), "MyLaptopXYZ", false));
    assert!(dv.created_at.ends_with('Z') && dv.created_at.contains('T'));
    let raw = fs::read_to_string(r.join("devices.enc")).unwrap();
    assert!(!raw.contains("MyLaptopXYZ") && !raw.contains(&s.device_id));
    // same machine reuses its id; second machine adds an entry
    let id1 = s.device_id.clone();
    drop(s);
    let s = unlock(r, &sk, "MyLaptopXYZ").unwrap();
    assert_eq!(s.device_id, id1);
    assert_eq!(s.devices().unwrap().devices.len(), 1);
    switch_device(r);
    let s2 = unlock(r, &sk, "phone").unwrap();
    assert_ne!(s2.device_id, id1);
    let names: Vec<String> = s2.devices().unwrap().devices.iter().map(|d| d.name.clone()).collect();
    assert_eq!(names, vec!["MyLaptopXYZ", "phone"]);
    assert!(fs::read_to_string(r.join(".gitignore")).unwrap().contains(".gitpass-device.json"));
}

#[test]
fn rename_and_prefix_resolution() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    let s = unlock(r, &sk, "a").unwrap();
    let id = s.rename_device(&s.device_id[..8], "renamed").unwrap();
    assert_eq!(id, s.device_id);
    assert_eq!(s.devices().unwrap().get(&id).unwrap().name, "renamed");
    assert!(s.rename_device("zzzz", "x").is_err());
    // name not overwritten by subsequent unlock with a different default name
    let s = unlock(r, &sk, "other").unwrap();
    assert_eq!(s.devices().unwrap().get(&id).unwrap().name, "renamed");
}

#[test]
fn revoke_blocks_mutation_other_devices_keep_working() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    let a = unlock(r, &sk, "lost-laptop").unwrap();
    let vid = a.create_vault("personal").unwrap();
    let iid = a.add_item(&vid, &item("GitHub")).unwrap();
    let lost = a.device_id.clone();
    drop(a);
    switch_device(r);
    let mut b = unlock(r, &sk, "desktop").unwrap();
    let good = b.device_id.clone();
    b.revoke_device(&lost[..6]).unwrap();
    let dv = b.devices().unwrap().get(&lost).unwrap().clone();
    assert!(dv.revoked && dv.revoked_at.is_some());
    // unrevoked device still works
    b.add_item(&vid, &item("still-ok")).unwrap();
    drop(b);

    // the lost device unlocks (it still has the secrets -- documented limitation) but is flagged
    set_device(r, &lost);
    let mut a = unlock(r, &sk, "lost-laptop").unwrap();
    assert!(a.device_revoked);
    let before = a.devices().unwrap().get(&lost).unwrap().clone();
    assert_eq!(before, dv, "revoked device's entry must not be refreshed");
    let e = a.add_item(&vid, &item("nope")).unwrap_err().to_string();
    assert!(e.contains("REVOKED"), "{e}");
    assert!(a.create_vault("x").is_err());
    assert!(a.update_item(&vid, &iid, &item("edit")).is_err());
    assert!(a.remove_item(&vid, &iid).is_err());
    assert!(a.rename_device(&good, "x").is_err());
    assert!(a.revoke_device(&good).is_err());
    assert!(a.unrevoke_device(&lost).is_err());
    assert!(a.change_password("another password", &sk, fast()).is_err());
    // read-only listing still allowed
    assert_eq!(a.devices().unwrap().devices.len(), 2);
    // recovery path: rotate-account-key still allowed, registry re-sealed under the new key
    a.rotate_account_key(PW, &sk).unwrap();
    for (v, _) in a.list_vaults().unwrap() { a.rotate_vault(&v).unwrap(); }
    drop(a);

    // good device re-opens after rotation (manifest + devices.enc verify) and can unrevoke
    set_device(r, &good);
    let b = unlock(r, &sk, "desktop").unwrap();
    assert!(!b.device_revoked);
    assert!(b.devices().unwrap().get(&lost).unwrap().revoked);
    b.unrevoke_device(&lost).unwrap();
    drop(b);
    set_device(r, &lost);
    let a = unlock(r, &sk, "lost-laptop").unwrap();
    assert!(!a.device_revoked);
    a.add_item(&vid, &item("back")).unwrap();
}

#[test]
fn self_revoke_takes_effect_immediately() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let mut s = unlock(d.path(), &sk, "me").unwrap();
    let id = s.device_id.clone();
    s.revoke_device(&id).unwrap();
    assert!(s.device_revoked);
    assert!(s.create_vault("v").is_err());
}

#[test]
fn devices_enc_tamper_and_deletion_detected() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    let s = unlock(r, &sk, "a").unwrap();
    let good = fs::read(r.join("devices.enc")).unwrap();
    drop(s);
    // substitute a validly-sealed but different registry (e.g. older copy with device unrevoked)
    let mut v: serde_json::Value = serde_json::from_slice(&good).unwrap();
    v["nonce"] = serde_json::Value::String("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into());
    fs::write(r.join("devices.enc"), serde_json::to_vec(&v).unwrap()).unwrap();
    let e = unlock(r, &sk, "a").unwrap_err().to_string();
    assert!(e.contains("devices.enc"), "{e}");
    fs::remove_file(r.join("devices.enc")).unwrap();
    let e = unlock(r, &sk, "a").unwrap_err().to_string();
    assert!(e.contains("devices.enc") && e.contains("missing"), "{e}");
}

#[test]
fn v2_manifest_without_devices_is_upgraded_on_first_unlock() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    // freshly-initialised vault is still a pure v2 manifest (no devices_hash field serialized)
    {
        let s = Session::unlock_with(r, PW, &sk, &opts()); // registers device
        let s = s.unwrap();
        assert_eq!(manifest(&s).format, 3);
    }
    let d2 = tempfile::tempdir().unwrap();
    let r2 = d2.path();
    let sk2 = store::init(r2, PW, fast()).unwrap();
    // before any unlock: devices.enc absent; v2 manifest JSON decodes with devices_hash = None
    assert!(!r2.join("devices.enc").exists());
    let s = unlock(r2, &sk2, "first").unwrap();
    assert!(r2.join("devices.enc").exists());
    assert_eq!(s.devices().unwrap().devices.len(), 1);
    // format.json (keyset layout) stays v2
    assert_eq!(s.fmt.version, 2);
}

#[test]
fn v2_manifest_json_is_backward_compatible() {
    let j = r#"{"format":2,"account_id":"x","epoch":1,"keyset_hash":"h","vaults":{}}"#;
    let m: Manifest = serde_json::from_str(j).unwrap();
    assert_eq!(m.devices_hash, None);
    assert_eq!(serde_json::to_string(&m).unwrap(), j, "None devices_hash must not change v2 encoding");
}
