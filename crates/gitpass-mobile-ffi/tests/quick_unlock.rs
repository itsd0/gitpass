//! #473: quick (biometric) unlock key over the bridge. Temporary vaults, fake secrets only.

use gitpass_core::crypto::KdfParams;
use gitpass_core::store::{self, Session, UnlockOpts};
use gitpass_mobile_ffi::{
    default_unlock_options, derive_quick_unlock_key, open_vault_with_quick_unlock_key, quick_unlock_stale_message,
    UnlockOptions,
};

const PASSWORD: &str = "FAKE correct horse battery staple";
fn kdf() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

fn account() -> (tempfile::TempDir, String, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PASSWORD, kdf()).unwrap();
    Session::unlock_with(d.path(), PASSWORD, &sk, &UnlockOpts::default()).unwrap().create_vault("Personal").unwrap();
    let root = d.path().to_string_lossy().into_owned();
    (d, root, sk.encode())
}

#[test]
fn derived_key_opens_the_vault_and_is_32_bytes() {
    let (_d, root, sk) = account();
    let key = derive_quick_unlock_key(root.clone(), PASSWORD.into(), sk).unwrap();
    assert_eq!(key.len(), 32);
    let vault = open_vault_with_quick_unlock_key(root, key, default_unlock_options()).unwrap();
    assert_eq!(vault.list_vaults().unwrap().len(), 1);
}

#[test]
fn wrong_inputs_fail_with_fixed_text_and_stale_key_is_detectable() {
    let (d, root, sk) = account();
    let e = derive_quick_unlock_key(root.clone(), "FAKE wrong".into(), sk.clone()).unwrap_err().to_string();
    assert!(!e.contains("FAKE wrong"), "no password in errors");
    for bad in [vec![], vec![1u8; 31], vec![1u8; 32], vec![1u8; 64]] {
        let e = open_vault_with_quick_unlock_key(root.clone(), bad, default_unlock_options()).err().unwrap();
        assert_eq!(e.to_string(), quick_unlock_stale_message());
    }
    let key = derive_quick_unlock_key(root.clone(), PASSWORD.into(), sk.clone()).unwrap();
    let parsed = gitpass_core::crypto::SecretKey::parse(&sk).unwrap();
    Session::unlock_with(d.path(), PASSWORD, &parsed, &UnlockOpts::default()).unwrap()
        .change_password("FAKE new master password", &parsed, kdf()).unwrap();
    let e = open_vault_with_quick_unlock_key(root, key, default_unlock_options()).err().unwrap();
    assert_eq!(e.to_string(), quick_unlock_stale_message());
}

#[test]
fn quick_unlock_refuses_password_only_side_effects() {
    let (_d, root, sk) = account();
    let key = derive_quick_unlock_key(root.clone(), PASSWORD.into(), sk).unwrap();
    for o in [
        UnlockOptions { register_device: true, ..Default::default() },
        UnlockOptions { upgrade_kdf: true, ..Default::default() },
    ] {
        assert!(open_vault_with_quick_unlock_key(root.clone(), key.clone(), o).is_err());
    }
}
