//! Quick (biometric) unlock with a stored Account Unlock Key (#473). Temp dirs only; all data FAKE.
//! Test vaults use the KDF floor only to keep the suite fast; the derived key is the production KDF
//! output for whatever parameters the vault has (nothing is lowered or skipped on derivation).
use gitpass_core::crypto::*;
use gitpass_core::store::{self, Item, Session, UnlockEffects, UnlockOpts, STALE_UNLOCK_KEY};
use std::fs;
use std::path::Path;

const PW: &str = "FAKE correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn item(t: &str) -> Item { let mut i = Item::default(); i.title = t.into(); i.password = Some("pw-FAKE".into()); i.updated_at = "1".into(); i }

fn tracked(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            // Dot-files are device-local state (throttle, last-used), not synced vault content.
            if rel.split('/').any(|c| c.starts_with('.')) { continue }
            if e.file_type().unwrap().is_dir() { stack.push(p) } else { out.push((rel, fs::read(&p).unwrap())) }
        }
    }
    out.sort();
    out
}

#[test]
fn derived_key_unlocks_without_the_password_and_without_side_effects() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let (vid, iid) = {
        let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
        let v = s.create_vault("A").unwrap();
        let i = s.add_item(&v, &item("x")).unwrap();
        (v, i)
    };
    let key = Session::derive_unlock_key(d.path(), PW, &sk).unwrap();
    let before = tracked(d.path());
    let s = Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.title, "x");
    // Writes work on a quick-unlocked session (same Account Key).
    s.add_item(&vid, &item("y")).unwrap();
    drop(s);
    assert_ne!(tracked(d.path()), before);
    let after_write = tracked(d.path());
    let _s = Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).unwrap();
    assert_eq!(tracked(d.path()), after_write, "a quick unlock writes no tracked vault file");
    // The derived key is not the password and does not contain it.
    let pw = PW.as_bytes();
    assert!(!key.0.windows(pw.len().min(32)).any(|w| w == &pw[..w.len()]));
}

#[test]
fn password_or_kdf_change_makes_the_key_stale() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let key = Session::derive_unlock_key(d.path(), PW, &sk).unwrap();
    {
        let mut s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
        s.change_password("FAKE a new master password", &sk, fast()).unwrap();
    }
    let e = Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).unwrap_err();
    assert_eq!(e.to_string(), STALE_UNLOCK_KEY);
    // A fresh key for the new password works; a KDF upgrade makes it stale again.
    let key2 = Session::derive_unlock_key(d.path(), "FAKE a new master password", &sk).unwrap();
    Session::unlock_with_unlock_key(d.path(), &key2, &opts(), UnlockEffects::NONE).unwrap();
    {
        let stronger = KdfParams { alg: "argon2id".into(), m_kib: 16384, t: 2, p: 1 };
        let mut s = Session::unlock_with(d.path(), "FAKE a new master password", &sk, &opts()).unwrap();
        s.rewrap_keyset("FAKE a new master password", &sk, stronger).unwrap();
    }
    assert_eq!(Session::unlock_with_unlock_key(d.path(), &key2, &opts(), UnlockEffects::NONE).unwrap_err().to_string(), STALE_UNLOCK_KEY);
}

#[test]
fn wrong_credentials_wrong_keys_and_password_only_effects_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    assert!(Session::derive_unlock_key(d.path(), "FAKE wrong password", &sk).is_err());
    let o = tempfile::tempdir().unwrap();
    let other = store::init(o.path(), PW, fast()).unwrap();
    assert!(Session::derive_unlock_key(d.path(), PW, &other).is_err(), "wrong Secret Key");
    let garbage = Key32([7u8; 32]);
    assert_eq!(Session::unlock_with_unlock_key(d.path(), &garbage, &opts(), UnlockEffects::NONE).unwrap_err().to_string(), STALE_UNLOCK_KEY);
    // A key from another account never opens this one.
    let foreign = Session::derive_unlock_key(o.path(), PW, &other).unwrap();
    assert!(Session::unlock_with_unlock_key(d.path(), &foreign, &opts(), UnlockEffects::NONE).is_err());
    let key = Session::derive_unlock_key(d.path(), PW, &sk).unwrap();
    for effects in [UnlockEffects::ALL, UnlockEffects { touch_device: true, purge_expired: false, upgrade_kdf: false }, UnlockEffects { touch_device: false, purge_expired: false, upgrade_kdf: true }] {
        assert!(Session::unlock_with_unlock_key(d.path(), &key, &opts(), effects).is_err());
    }
    // A pending password change refuses both derivation and quick unlock.
    fs::write(d.path().join("format.json.new"), b"{}").unwrap();
    assert!(Session::derive_unlock_key(d.path(), PW, &sk).is_err());
    assert_eq!(Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).unwrap_err().to_string(), STALE_UNLOCK_KEY);
}

#[test]
fn quick_unlock_still_rejects_rollback() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let vid = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap().create_vault("A").unwrap();
    let old = fs::read(d.path().join("manifest.enc")).unwrap();
    let old_vault: Vec<_> = tracked(d.path());
    Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap().add_item(&vid, &item("x")).unwrap();
    let key = Session::derive_unlock_key(d.path(), PW, &sk).unwrap();
    Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).unwrap();
    // Restore the older manifest and files: the epoch went backwards.
    for (p, b) in &old_vault { fs::write(d.path().join(p), b).unwrap(); }
    fs::write(d.path().join("manifest.enc"), old).unwrap();
    assert!(Session::unlock_with_unlock_key(d.path(), &key, &opts(), UnlockEffects::NONE).is_err());
}
